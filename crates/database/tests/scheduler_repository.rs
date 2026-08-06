use sqlx::{PgPool, postgres::PgPoolOptions, query, query_scalar};
use techatlas_database::{PostgresCrawlScheduleRepository, run_migrations};
use techatlas_models::{
    CrawlAttemptOutcome, CrawlFailure, CrawlScheduleRepository, SchedulerSettings,
};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ImageExt, runners::AsyncRunner},
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[tokio::test]
async fn reserves_only_due_active_domains_in_priority_order_and_writes_outbox_records()
-> Result<(), Box<dyn std::error::Error>> {
    let (_container, pool) = database().await?;
    let now = postgres_timestamp(OffsetDateTime::now_utc())?;
    let high = insert_domain_with_policy(
        &pool,
        "high.example",
        "high",
        true,
        Some(Duration::days(1)),
        now,
    )
    .await?;
    let medium =
        insert_domain_with_policy(&pool, "medium.example", "medium", true, None, now).await?;
    let disabled =
        insert_domain_with_policy(&pool, "disabled.example", "high", false, None, now).await?;
    let archived =
        insert_domain_with_policy(&pool, "archived.example", "high", true, None, now).await?;
    query("UPDATE domains SET archived_at = $1 WHERE id = $2")
        .bind(now)
        .bind(archived)
        .execute(&pool)
        .await?;
    let in_flight =
        insert_domain_with_policy(&pool, "in-flight.example", "high", true, None, now).await?;
    query(
        "INSERT INTO crawl_attempts (domain_id, job_id, correlation_id, idempotency_key, created_at, queued_at) \
         VALUES ($1, $2, $3, $4, $5, $5)",
    )
    .bind(in_flight)
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind("in-flight-attempt")
    .bind(now)
    .execute(&pool)
    .await?;

    let repository = PostgresCrawlScheduleRepository::new(pool.clone());
    let jobs = repository.reserve_due(now, 10).await?;

    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].domain_id().as_uuid(), high);
    assert_eq!(jobs[1].domain_id().as_uuid(), medium);
    assert!(!jobs.iter().any(|job| job.domain_id().as_uuid() == disabled));
    assert!(!jobs.iter().any(|job| job.domain_id().as_uuid() == archived));
    assert!(
        !jobs
            .iter()
            .any(|job| job.domain_id().as_uuid() == in_flight)
    );

    let attempts: i64 = query_scalar("SELECT COUNT(*) FROM crawl_attempts")
        .fetch_one(&pool)
        .await?;
    let outbox: i64 = query_scalar("SELECT COUNT(*) FROM crawl_job_outbox")
        .fetch_one(&pool)
        .await?;
    assert_eq!(attempts, 3);
    assert_eq!(outbox, 2);

    let high_next: OffsetDateTime =
        query_scalar("SELECT next_crawl_at FROM crawl_policies WHERE domain_id = $1")
            .bind(high)
            .fetch_one(&pool)
            .await?;
    assert_eq!(high_next, now + Duration::days(1));
    Ok(())
}

#[tokio::test]
async fn schedules_bounded_retry_then_records_terminal_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let (_container, pool) = database().await?;
    let now = postgres_timestamp(OffsetDateTime::now_utc())?;
    let domain = insert_domain_with_policy(
        &pool,
        "retry.example",
        "medium",
        true,
        Some(Duration::days(7)),
        now,
    )
    .await?;
    let repository = PostgresCrawlScheduleRepository::new(pool.clone());
    let settings = SchedulerSettings::default();
    let first = repository.reserve_due(now, 1).await?.remove(0);
    let failure = CrawlFailure::new("network_timeout", "connection timed out")?;

    let mut job = first;
    let mut completed_at = now;
    for retry_number in 1..=3 {
        completed_at += Duration::minutes(1);
        repository
            .mark_published(job.job_id(), completed_at)
            .await?;
        repository
            .record_attempt_outcome(
                job.job_id(),
                CrawlAttemptOutcome::Failed(failure.clone()),
                completed_at,
                settings,
            )
            .await?;
        let expected_available = completed_at
            + Duration::try_from(
                settings
                    .crawl_retry_delay(retry_number)
                    .expect("retry should exist"),
            )?;
        let pending = repository
            .pending_publications(expected_available, 1)
            .await?;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].available_at(), expected_available);
        job = pending[0].job().clone();
        completed_at = expected_available;
    }

    completed_at += Duration::minutes(1);
    repository
        .mark_published(job.job_id(), completed_at)
        .await?;
    repository
        .record_attempt_outcome(
            job.job_id(),
            CrawlAttemptOutcome::Failed(failure),
            completed_at,
            settings,
        )
        .await?;

    let terminal_failure: Option<OffsetDateTime> =
        query_scalar("SELECT terminal_failure_at FROM crawl_policies WHERE domain_id = $1")
            .bind(domain)
            .fetch_one(&pool)
            .await?;
    assert_eq!(terminal_failure, Some(completed_at));
    let queued_retries: i64 = query_scalar(
        "SELECT COUNT(*) FROM crawl_attempts WHERE domain_id = $1 AND status = 'queued'",
    )
    .bind(domain)
    .fetch_one(&pool)
    .await?;
    assert_eq!(queued_retries, 0);
    Ok(())
}

#[tokio::test]
async fn caps_unpublished_outbox_records_with_visible_terminal_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let (_container, pool) = database().await?;
    let now = postgres_timestamp(OffsetDateTime::now_utc())?;
    insert_domain_with_policy(&pool, "outbox.example", "medium", true, None, now).await?;
    let repository = PostgresCrawlScheduleRepository::new(pool.clone());
    let job = repository.reserve_due(now, 1).await?.remove(0);
    let settings = SchedulerSettings::default();

    for attempt in 1..=settings.max_outbox_publish_attempts() {
        repository
            .record_publish_failure(
                job.job_id(),
                now + Duration::seconds(i64::from(attempt)),
                settings,
            )
            .await?;
    }

    let state: (i16, Option<OffsetDateTime>) =
        query_as_outbox_state(&pool, job.job_id().as_uuid()).await?;
    assert_eq!(state.0, i16::from(settings.max_outbox_publish_attempts()));
    assert!(state.1.is_some());
    Ok(())
}

async fn database() -> Result<
    (
        testcontainers_modules::testcontainers::ContainerAsync<Postgres>,
        PgPool,
    ),
    Box<dyn std::error::Error>,
> {
    let container = Postgres::default().with_tag("16-alpine").start().await?;
    let port = container.get_host_port_ipv4(5432).await?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&format!(
            "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
        ))
        .await?;
    run_migrations(&pool).await?;
    Ok((container, pool))
}

async fn insert_domain_with_policy(
    pool: &PgPool,
    canonical_domain: &str,
    priority: &str,
    enabled: bool,
    interval: Option<Duration>,
    now: OffsetDateTime,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let domain_id: Uuid =
        query_scalar("INSERT INTO domains (canonical_domain) VALUES ($1) RETURNING id")
            .bind(canonical_domain)
            .fetch_one(pool)
            .await?;
    let desired_interval = interval.unwrap_or(Duration::days(7));
    query(
        "INSERT INTO crawl_policies (domain_id, is_enabled, priority, desired_interval, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $5)",
    )
    .bind(domain_id)
    .bind(enabled)
    .bind(priority)
    .bind(postgres_interval(desired_interval)?)
    .bind(now - Duration::days(1))
    .execute(pool)
    .await?;
    Ok(domain_id)
}

fn postgres_interval(
    duration: Duration,
) -> Result<sqlx::postgres::types::PgInterval, std::io::Error> {
    let duration = std::time::Duration::try_from(duration)
        .map_err(|_| std::io::Error::other("interval must be positive"))?;
    sqlx::postgres::types::PgInterval::try_from(duration)
        .map_err(|_| std::io::Error::other("interval is too large"))
}

fn postgres_timestamp(
    timestamp: OffsetDateTime,
) -> Result<OffsetDateTime, time::error::ComponentRange> {
    timestamp.replace_nanosecond(timestamp.nanosecond() / 1_000 * 1_000)
}

async fn query_as_outbox_state(
    pool: &PgPool,
    job_id: Uuid,
) -> Result<(i16, Option<OffsetDateTime>), sqlx::Error> {
    sqlx::query_as(
        "SELECT publish_attempt_count, terminal_failure_at FROM crawl_job_outbox WHERE job_id = $1",
    )
    .bind(job_id)
    .fetch_one(pool)
    .await
}
