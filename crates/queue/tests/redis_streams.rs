use redis::{AsyncCommands, FromRedisValue};
use secrecy::SecretString;
use std::time::Duration;
use techatlas_models::{CorrelationId, CrawlJobId, CrawlJobV1, DomainId, IdempotencyKey};
use techatlas_queue::{
    CrawlJobConsumer, CrawlJobConsumerName, CrawlJobPublisher, RedisStreamsConfig,
    RedisStreamsCrawlJobQueue,
};
use testcontainers_modules::testcontainers::{
    ContainerAsync, GenericImage,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use uuid::Uuid;

#[tokio::test]
async fn publishes_consumes_acknowledges_and_reclaims_a_crawl_job()
-> Result<(), Box<dyn std::error::Error>> {
    let (container, queue, config) = queue_with_policy(5).await?;
    let first_consumer = CrawlJobConsumerName::parse("worker-a")?;
    let recovering_consumer = CrawlJobConsumerName::parse("worker-b")?;
    let job = crawl_job();

    let published = queue.publish(&job).await?;
    let received = queue.receive(&first_consumer).await?;
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].receipt(), &published);
    assert_eq!(received[0].job(), &job);
    assert_eq!(received[0].delivery_count(), 1);

    let replayed = queue.reclaim_stale(&recovering_consumer).await?;
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].job(), &job);
    assert_eq!(replayed[0].receipt(), &published);
    assert_eq!(replayed[0].delivery_count(), 2);
    queue.acknowledge(replayed[0].receipt()).await?;

    assert_eq!(pending_count(&container, &config).await?, 0);
    Ok(())
}

#[tokio::test]
async fn moves_an_exhausted_job_to_the_dead_letter_stream_once()
-> Result<(), Box<dyn std::error::Error>> {
    let (container, queue, config) = queue_with_policy(2).await?;
    let first_consumer = CrawlJobConsumerName::parse("worker-a")?;
    let recovering_consumer = CrawlJobConsumerName::parse("worker-b")?;

    queue.publish(&crawl_job()).await?;
    assert_eq!(queue.receive(&first_consumer).await?.len(), 1);
    assert!(queue.reclaim_stale(&recovering_consumer).await?.is_empty());
    assert!(queue.reclaim_stale(&recovering_consumer).await?.is_empty());

    assert_eq!(pending_count(&container, &config).await?, 0);
    let dead_letters = dead_letters(&container, &config).await?;
    assert_eq!(dead_letters.len(), 1);
    let reason = dead_letters[0]
        .map
        .get("reason")
        .and_then(|value| String::from_redis_value(value).ok());
    assert_eq!(reason.as_deref(), Some("delivery_limit_exceeded"));
    Ok(())
}

async fn queue_with_policy(
    max_deliveries: usize,
) -> Result<
    (
        ContainerAsync<GenericImage>,
        RedisStreamsCrawlJobQueue,
        RedisStreamsConfig,
    ),
    Box<dyn std::error::Error>,
> {
    let container = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
        .start()
        .await?;
    let port = container.get_host_port_ipv4(6379).await?;
    let redis_url = SecretString::from(format!("redis://127.0.0.1:{port}"));
    let config = RedisStreamsConfig::default()
        .with_stream_names(
            "test:crawl-jobs:v1",
            "test:crawl-workers:v1",
            "test:crawl-jobs:dead-letter:v1",
            "test:crawl-jobs:dead-letter-index:v1",
        )?
        .with_read_options(10, Duration::ZERO, Duration::from_secs(2))?
        .with_recovery_policy(Duration::ZERO, max_deliveries)?;
    let queue = RedisStreamsCrawlJobQueue::new(&redis_url, config.clone())?;

    Ok((container, queue, config))
}

async fn pending_count(
    container: &ContainerAsync<GenericImage>,
    config: &RedisStreamsConfig,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut connection = redis_connection(container).await?;
    let pending: redis::streams::StreamPendingReply = connection
        .xpending(config.stream_key(), config.consumer_group())
        .await?;
    Ok(pending.count())
}

async fn dead_letters(
    container: &ContainerAsync<GenericImage>,
    config: &RedisStreamsConfig,
) -> Result<Vec<redis::streams::StreamId>, Box<dyn std::error::Error>> {
    let mut connection = redis_connection(container).await?;
    let entries: redis::streams::StreamRangeReply = connection
        .xrange_all(config.dead_letter_stream_key())
        .await?;
    Ok(entries.ids)
}

async fn redis_connection(
    container: &ContainerAsync<GenericImage>,
) -> Result<redis::aio::MultiplexedConnection, Box<dyn std::error::Error>> {
    let port = container.get_host_port_ipv4(6379).await?;
    let client = redis::Client::open(format!("redis://127.0.0.1:{port}"))?;
    Ok(client.get_multiplexed_async_connection().await?)
}

fn crawl_job() -> CrawlJobV1 {
    CrawlJobV1::new(
        CrawlJobId::from_uuid(Uuid::from_u128(1)),
        DomainId::from_uuid(Uuid::from_u128(2)),
        CorrelationId::from_uuid(Uuid::from_u128(3)),
        IdempotencyKey::parse("crawl-job:1").expect("test idempotency key should be valid"),
    )
}
