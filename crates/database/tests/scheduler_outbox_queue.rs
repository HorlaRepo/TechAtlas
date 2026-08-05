use secrecy::SecretString;
use sqlx::{PgPool, postgres::PgPoolOptions, query, query_scalar};
use techatlas_database::{PostgresCrawlScheduleRepository, run_migrations};
use techatlas_models::CrawlScheduleRepository;
use techatlas_queue::{
    CrawlJobConsumer, CrawlJobConsumerName, CrawlJobPublisher, RedisStreamsConfig,
    RedisStreamsCrawlJobQueue,
};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{
        ContainerAsync, GenericImage, ImageExt,
        core::{IntoContainerPort, WaitFor},
        runners::AsyncRunner,
    },
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[tokio::test]
async fn outbox_replays_an_unmarked_redis_publish_with_the_same_idempotency_key()
-> Result<(), Box<dyn std::error::Error>> {
    let (_postgres, pool) = database().await?;
    let redis = redis().await?;
    let now = OffsetDateTime::now_utc();
    insert_due_domain(&pool, now).await?;
    let repository = PostgresCrawlScheduleRepository::new(pool);
    let queue = RedisStreamsCrawlJobQueue::new(
        &SecretString::from(redis_url(&redis).await?),
        RedisStreamsConfig::default(),
    )?;

    let job = repository.reserve_due(now, 1).await?.remove(0);
    let first = repository.pending_publications(now, 1).await?.remove(0);
    queue.publish(first.job()).await?;

    let replay = repository.pending_publications(now, 1).await?.remove(0);
    assert_eq!(replay.job().job_id(), first.job().job_id());
    assert_eq!(
        replay.job().idempotency_key(),
        first.job().idempotency_key()
    );
    queue.publish(replay.job()).await?;
    repository.mark_published(job.job_id(), now).await?;
    assert!(repository.pending_publications(now, 1).await?.is_empty());

    let consumer = CrawlJobConsumerName::parse("scheduler-outbox-test")?;
    let received = queue.receive(&consumer).await?;
    assert_eq!(received.len(), 2);
    assert!(
        received
            .iter()
            .all(|entry| entry.job().idempotency_key() == job.idempotency_key())
    );
    Ok(())
}

async fn database() -> Result<(ContainerAsync<Postgres>, PgPool), Box<dyn std::error::Error>> {
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

async fn redis() -> Result<ContainerAsync<GenericImage>, Box<dyn std::error::Error>> {
    Ok(GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
        .start()
        .await?)
}

async fn redis_url(
    container: &ContainerAsync<GenericImage>,
) -> Result<String, Box<dyn std::error::Error>> {
    let port = container.get_host_port_ipv4(6379).await?;
    Ok(format!("redis://127.0.0.1:{port}"))
}

async fn insert_due_domain(
    pool: &PgPool,
    now: OffsetDateTime,
) -> Result<(), Box<dyn std::error::Error>> {
    let domain_id: Uuid = query_scalar(
        "INSERT INTO domains (canonical_domain) VALUES ('outbox-queue.example') RETURNING id",
    )
    .fetch_one(pool)
    .await?;
    let interval = sqlx::postgres::types::PgInterval::try_from(std::time::Duration::from_secs(
        7 * 24 * 60 * 60,
    ))
    .map_err(|_| std::io::Error::other("interval is too large"))?;
    query(
        "INSERT INTO crawl_policies (domain_id, desired_interval, created_at, updated_at) \
         VALUES ($1, $2, $3, $3)",
    )
    .bind(domain_id)
    .bind(interval)
    .bind(now - Duration::days(1))
    .execute(pool)
    .await?;
    Ok(())
}
