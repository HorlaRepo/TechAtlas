use async_trait::async_trait;
use secrecy::SecretString;
use sqlx::{PgPool, Row, postgres::PgPoolOptions, query, query_scalar};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};
use techatlas_crawler::{
    Capture, CaptureUnavailableReason, CrawlError, CrawlResponse, CrawlUrl, PolitenessGate,
    PolitenessGateError, PolitenessRequest,
};
use techatlas_database::{
    PostgresCrawlScheduleRepository, PostgresCrawlWorkerRepository, run_migrations,
};
use techatlas_models::{
    CrawlScheduleRepository, RawArtifactCompression, RawArtifactMetadata, SchedulerSettings,
};
use techatlas_queue::{
    CrawlJobConsumerName, CrawlJobPublisher, RedisStreamsConfig, RedisStreamsCrawlJobQueue,
};
use techatlas_storage::{LocalRawArtifactStore, RawArtifactStore};
use techatlas_worker::{CrawlAcquirer, WorkerProcessor, receive_and_process_once};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{
        GenericImage, ImageExt,
        core::{IntoContainerPort, WaitFor},
        runners::AsyncRunner,
    },
};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const DETECTION_FIXTURE: &[u8] =
    include_bytes!("../../../tests/fixtures/detector/nextjs-worker.html");

#[tokio::test]
async fn queued_fixture_crawl_creates_one_terminal_attempt_and_one_snapshot()
-> Result<(), Box<dyn std::error::Error>> {
    let postgres = Postgres::default().with_tag("16-alpine").start().await?;
    let postgres_port = postgres.get_host_port_ipv4(5432).await?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&format!(
            "postgres://postgres:postgres@127.0.0.1:{postgres_port}/postgres"
        ))
        .await?;
    run_migrations(&pool).await?;
    let domain_id = insert_domain_with_policy(&pool).await?;
    let scheduler = PostgresCrawlScheduleRepository::new(pool.clone());
    let job = scheduler
        .reserve_due(OffsetDateTime::now_utc(), 1)
        .await?
        .remove(0);
    assert_eq!(job.domain_id().as_uuid(), domain_id);

    let redis = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
        .start()
        .await?;
    let redis_port = redis.get_host_port_ipv4(6379).await?;
    let redis_url = SecretString::from(format!("redis://127.0.0.1:{redis_port}"));
    let queue_config = RedisStreamsConfig::default()
        .with_stream_names(
            "test:worker:crawl-jobs:v1",
            "test:worker:crawl-workers:v1",
            "test:worker:crawl-jobs:dead-letter:v1",
            "test:worker:crawl-jobs:dead-letter-index:v1",
        )?
        .with_read_options(10, Duration::ZERO, Duration::from_secs(2))?
        .with_recovery_policy(Duration::ZERO, 5)?;
    let queue = RedisStreamsCrawlJobQueue::new(&redis_url, queue_config)?;
    queue.publish(&job).await?;
    let artifact_root = temporary_artifact_root();
    let artifact_store = Arc::new(LocalRawArtifactStore::new(artifact_root.clone())?);

    let processor = WorkerProcessor::new(
        Arc::new(PostgresCrawlWorkerRepository::new(pool.clone())),
        Arc::new(FixtureAcquirer),
        Arc::new(OpenGate),
        artifact_store.clone(),
        Duration::from_millis(1),
        time::Duration::days(30),
        SchedulerSettings::default(),
    );
    let consumer = CrawlJobConsumerName::parse("fixture-worker")?;
    let processed =
        receive_and_process_once(&queue, &consumer, &processor, &CancellationToken::new()).await?;
    assert_eq!(processed, 1);

    let status: String = query_scalar("SELECT status FROM crawl_attempts WHERE job_id = $1")
        .bind(job.job_id().as_uuid())
        .fetch_one(&pool)
        .await?;
    let snapshot_count: i64 = query_scalar(
        "SELECT COUNT(*) FROM crawl_snapshots WHERE domain_id = $1 AND crawl_attempt_id IN \
         (SELECT id FROM crawl_attempts WHERE job_id = $2)",
    )
    .bind(domain_id)
    .bind(job.job_id().as_uuid())
    .fetch_one(&pool)
    .await?;
    assert_eq!(status, "succeeded");
    assert_eq!(snapshot_count, 1);
    let dns_observation = query(
        "SELECT source, unavailable_reason, addresses::text AS addresses FROM crawl_dns_observations",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        dns_observation.try_get::<String, _>("source")?,
        "crawler_dns_v1"
    );
    assert_eq!(
        dns_observation.try_get::<Option<String>, _>("unavailable_reason")?,
        Some("connection_failed".to_owned())
    );
    assert_eq!(dns_observation.try_get::<String, _>("addresses")?, "[]");
    let tls_observation =
        query("SELECT source, unavailable_reason, validation_status FROM crawl_tls_observations")
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        tls_observation.try_get::<String, _>("source")?,
        "crawler_tls_v1"
    );
    assert_eq!(
        tls_observation.try_get::<Option<String>, _>("unavailable_reason")?,
        Some("not_applicable".to_owned())
    );
    assert_eq!(
        tls_observation.try_get::<Option<String>, _>("validation_status")?,
        None
    );
    assert!(
        query("UPDATE crawl_dns_observations SET source = 'mutated'")
            .execute(&pool)
            .await
            .is_err()
    );
    let detection = query(
        "SELECT technologies.slug, detections.confidence, detections.method, detection_rules.slug AS rule_slug, \
             detection_rule_versions.version \
         FROM detections \
         JOIN technologies ON technologies.id = detections.technology_id \
         JOIN detection_rule_versions ON detection_rule_versions.id = detections.detection_rule_version_id \
         JOIN detection_rules ON detection_rules.id = detection_rule_versions.detection_rule_id",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(detection.try_get::<String, _>("slug")?, "nextjs");
    assert_eq!(detection.try_get::<i16, _>("confidence")?, 100);
    assert_eq!(
        detection.try_get::<String, _>("method")?,
        "deterministic_rule"
    );
    assert_eq!(detection.try_get::<String, _>("rule_slug")?, "nextjs-v1");
    assert_eq!(detection.try_get::<i16, _>("version")?, 1);
    let evidence_count: i64 = query_scalar("SELECT COUNT(*) FROM detection_evidence")
        .fetch_one(&pool)
        .await?;
    assert_eq!(evidence_count, 2);
    let current_count: i64 = query_scalar("SELECT COUNT(*) FROM domain_current_technologies")
        .fetch_one(&pool)
        .await?;
    assert_eq!(current_count, 1);
    let index_outbox_count: i64 = query_scalar("SELECT COUNT(*) FROM search_index_outbox")
        .fetch_one(&pool)
        .await?;
    assert_eq!(index_outbox_count, 1);
    let change_kind: String = query_scalar("SELECT change_kind FROM technology_changes")
        .fetch_one(&pool)
        .await?;
    assert_eq!(change_kind, "added");
    let observation_counts =
        query("SELECT status, COUNT(*) AS count FROM detection_rule_observations GROUP BY status")
            .fetch_all(&pool)
            .await?;
    assert_eq!(observation_counts.len(), 2);
    assert!(observation_counts.iter().any(|row| {
        row.try_get::<String, _>("status").ok().as_deref() == Some("detected")
            && row.try_get::<i64, _>("count").ok() == Some(1)
    }));
    assert!(observation_counts.iter().any(|row| {
        row.try_get::<String, _>("status").ok().as_deref() == Some("confirmed_absent")
            && row.try_get::<i64, _>("count").ok() == Some(4)
    }));
    let artifact_row = query(
        "SELECT storage_location, checksum_sha256, uncompressed_size_bytes, compressed_size_bytes, \
             retention_expires_at FROM raw_artifacts",
    )
    .fetch_one(&pool)
    .await?;
    let artifact = RawArtifactMetadata::response_body(
        &artifact_row.try_get::<String, _>("storage_location")?,
        &artifact_row.try_get::<String, _>("checksum_sha256")?,
        RawArtifactCompression::Zstd,
        u64::try_from(artifact_row.try_get::<i64, _>("uncompressed_size_bytes")?)?,
        u64::try_from(artifact_row.try_get::<i64, _>("compressed_size_bytes")?)?,
        artifact_row.try_get("retention_expires_at")?,
    )?;
    assert_eq!(artifact_store.retrieve(&artifact).await?, DETECTION_FIXTURE);

    // Reclaimed/redelivered terminal deliveries are acknowledged without creating another snapshot.
    queue.publish(&job).await?;
    let replayed =
        receive_and_process_once(&queue, &consumer, &processor, &CancellationToken::new()).await?;
    assert_eq!(replayed, 1);
    let snapshot_count: i64 =
        query_scalar("SELECT COUNT(*) FROM crawl_snapshots WHERE domain_id = $1")
            .bind(domain_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(snapshot_count, 1);
    let artifact_count: i64 = query_scalar("SELECT COUNT(*) FROM raw_artifacts")
        .fetch_one(&pool)
        .await?;
    assert_eq!(artifact_count, 1);
    std::fs::remove_dir_all(artifact_root)?;
    Ok(())
}

fn temporary_artifact_root() -> PathBuf {
    std::env::temp_dir().join(format!("techatlas-worker-artifacts-{}", Uuid::new_v4()))
}

struct FixtureAcquirer;

#[async_trait]
impl CrawlAcquirer for FixtureAcquirer {
    async fn fetch(&self, _: CrawlUrl, _: &CancellationToken) -> Result<CrawlResponse, CrawlError> {
        Ok(CrawlResponse {
            final_url: CrawlUrl::parse("https://fixture.example/final")
                .expect("fixture URL should be valid"),
            resolved_addresses: Vec::new(),
            redirects: Vec::new(),
            status: 200,
            headers: BTreeMap::from([
                ("content-type".to_owned(), "text/html".to_owned()),
                ("x-powered-by".to_owned(), "Next.js".to_owned()),
            ]),
            body: DETECTION_FIXTURE.to_vec(),
            dns: Capture::Unavailable(CaptureUnavailableReason::ConnectionFailed),
            tls: Capture::Unavailable(CaptureUnavailableReason::NotApplicable),
        })
    }
}

struct OpenGate;

#[async_trait]
impl PolitenessGate for OpenGate {
    async fn acquire(
        &self,
        _: &PolitenessRequest,
        _: &CancellationToken,
    ) -> Result<(), PolitenessGateError> {
        Ok(())
    }
}

async fn insert_domain_with_policy(pool: &PgPool) -> Result<Uuid, sqlx::Error> {
    let domain_id: Uuid = query_scalar(
        "INSERT INTO domains (canonical_domain) VALUES ('fixture.example') RETURNING id",
    )
    .fetch_one(pool)
    .await?;
    query(
        "INSERT INTO crawl_policies (domain_id, desired_interval) VALUES ($1, INTERVAL '7 days')",
    )
    .bind(domain_id)
    .execute(pool)
    .await?;
    Ok(domain_id)
}
