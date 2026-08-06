use sqlx::{PgPool, postgres::PgPoolOptions, query, query_scalar};
use techatlas_database::{PostgresPublicRefreshRepository, run_migrations};
use techatlas_models::{CanonicalDomain, PublicRefreshError, PublicRefreshOperations};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ImageExt, runners::AsyncRunner},
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[tokio::test]
async fn records_one_refresh_per_cooldown_and_only_advances_scheduler_eligibility()
-> Result<(), Box<dyn std::error::Error>> {
    let (_container, pool) = database().await?;
    let enabled = insert_domain_with_policy(&pool, "refresh.example", true).await?;
    let disabled = insert_domain_with_policy(&pool, "disabled-refresh.example", false).await?;
    let repository = PostgresPublicRefreshRepository::new(pool.clone());
    let requested_at = OffsetDateTime::now_utc();
    let requested_at =
        requested_at.replace_nanosecond(requested_at.nanosecond() / 1_000 * 1_000)?;
    let cooldown = Duration::hours(24);

    let request = repository
        .request_refresh(
            &CanonicalDomain::parse("refresh.example")?,
            requested_at,
            cooldown,
        )
        .await?;
    assert_eq!(request.accepted_at, requested_at);
    assert_eq!(request.next_allowed_at, requested_at + cooldown);

    let recorded: i64 =
        query_scalar("SELECT COUNT(*) FROM public_refresh_requests WHERE domain_id = $1")
            .bind(enabled)
            .fetch_one(&pool)
            .await?;
    assert_eq!(recorded, 1);
    let next_crawl_at: OffsetDateTime =
        query_scalar("SELECT next_crawl_at FROM crawl_policies WHERE domain_id = $1")
            .bind(enabled)
            .fetch_one(&pool)
            .await?;
    assert_eq!(next_crawl_at, requested_at);
    let outbox: i64 = query_scalar("SELECT COUNT(*) FROM crawl_job_outbox")
        .fetch_one(&pool)
        .await?;
    assert_eq!(outbox, 0);

    assert_eq!(
        repository
            .request_refresh(
                &CanonicalDomain::parse("refresh.example")?,
                requested_at + Duration::minutes(1),
                cooldown,
            )
            .await,
        Err(PublicRefreshError::RateLimited {
            retry_after_seconds: 86_340,
        })
    );
    assert_eq!(
        repository
            .request_refresh(
                &CanonicalDomain::parse("disabled-refresh.example")?,
                requested_at,
                cooldown,
            )
            .await,
        Err(PublicRefreshError::Disabled)
    );
    assert_eq!(
        repository
            .request_refresh(
                &CanonicalDomain::parse("missing-refresh.example")?,
                requested_at,
                cooldown,
            )
            .await,
        Err(PublicRefreshError::NotFound)
    );

    let disabled_requests: i64 =
        query_scalar("SELECT COUNT(*) FROM public_refresh_requests WHERE domain_id = $1")
            .bind(disabled)
            .fetch_one(&pool)
            .await?;
    assert_eq!(disabled_requests, 0);
    pool.close().await;
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
    is_enabled: bool,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let domain_id: Uuid =
        query_scalar("INSERT INTO domains (canonical_domain) VALUES ($1) RETURNING id")
            .bind(canonical_domain)
            .fetch_one(pool)
            .await?;
    query(
        "INSERT INTO crawl_policies (domain_id, is_enabled, priority, desired_interval, next_crawl_at) \
         VALUES ($1, $2, 'medium', INTERVAL '7 days', now() + INTERVAL '7 days')",
    )
    .bind(domain_id)
    .bind(is_enabled)
    .execute(pool)
    .await?;
    Ok(domain_id)
}
