#![forbid(unsafe_code)]

use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use secrecy::SecretString;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    env,
    error::Error,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use techatlas_common::{DependencyProbe, DependencyStatus};
use techatlas_crawler::{Crawler, CrawlerConfig};
use techatlas_database::{
    PostgresAdminOperationsRepository, PostgresCrawlWorkerRepository, PostgresProbe,
    PostgresReprocessingRepository, PostgresSearchIndexRepository, connect_lazy_pool,
};
use techatlas_models::{SchedulerSettings, WorkerHeartbeat, WorkerHeartbeatOperations};
use techatlas_queue::{
    CrawlJobConsumer, CrawlJobConsumerName, RedisPolitenessConfig, RedisPolitenessGate, RedisProbe,
    RedisStreamsConfig, RedisStreamsCrawlJobQueue,
};
use techatlas_search::{
    DomainSearchDocument, DomainSearchIndex, MeilisearchDomainIndex, MeilisearchProbe,
};
use techatlas_storage::LocalRawArtifactStore;
use techatlas_telemetry::{TelemetryMetrics, init_tracing};
use techatlas_worker::{
    CountryEnricher, NoCountryEnricher, ReprocessingProcessor, WorkerProcessor,
    process_and_acknowledge,
};
use thiserror::Error;
use tokio::{net::TcpListener, signal, sync::Semaphore, task::JoinSet, time::sleep};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = WorkerConfig::from_env()?;
    init_tracing("techatlas-worker", config.log_filter())?;

    let pool = connect_lazy_pool(config.database_url(), config.operation_timeout())?;
    let queue = Arc::new(RedisStreamsCrawlJobQueue::new(
        config.redis_url(),
        config.queue_config(),
    )?);
    let repository = Arc::new(PostgresCrawlWorkerRepository::new(pool.clone()));
    let heartbeats: Arc<dyn WorkerHeartbeatOperations> =
        Arc::new(PostgresAdminOperationsRepository::new(pool.clone()));
    let crawler = Arc::new(Crawler::with_system_resolver(
        config.crawler_config().clone(),
    )?);
    let gate = Arc::new(RedisPolitenessGate::new(
        config.redis_url(),
        config.politeness_config(),
    )?);
    let artifact_store: Arc<dyn techatlas_storage::RawArtifactStore> =
        Arc::new(LocalRawArtifactStore::new(config.artifact_root())?);
    let country_enricher: Arc<dyn CountryEnricher> = match config.geoip_database() {
        Some(geoip_database) => Arc::new(GeoLiteCountryEnricher::open(
            geoip_database.path(),
            geoip_database.version(),
        )?),
        None => Arc::new(NoCountryEnricher),
    };
    let telemetry_metrics = Arc::new(TelemetryMetrics::new("techatlas_worker")?);
    let processor = Arc::new(
        WorkerProcessor::new(
            repository,
            crawler,
            gate,
            Arc::clone(&artifact_store),
            config.crawler_config().politeness_delay,
            config.artifact_retention(),
            config.scheduler_settings(),
        )
        .with_country_enricher(country_enricher)
        .with_metrics(Arc::clone(&telemetry_metrics)),
    );
    let reprocessing_processor = Arc::new(ReprocessingProcessor::new(
        Arc::new(PostgresReprocessingRepository::new(pool.clone())),
        artifact_store,
        config.reprocessing_max_attempts(),
        config.reprocessing_retry_delay(),
    ));
    let index_repository = Arc::new(PostgresSearchIndexRepository::new(pool.clone()));
    let search_index = Arc::new(MeilisearchDomainIndex::new(
        config.meilisearch_url(),
        config.meilisearch_master_key(),
        config.index_operation_timeout(),
    )?);
    let health = HealthState {
        metrics: Arc::clone(&telemetry_metrics),
        postgres: Arc::new(PostgresProbe::from_pool(pool, config.operation_timeout())),
        redis: Arc::new(RedisProbe::new(
            config.redis_url(),
            config.operation_timeout(),
        )?),
        meilisearch: Arc::new(MeilisearchProbe::new(
            config.meilisearch_url(),
            config.meilisearch_master_key(),
            config.index_operation_timeout(),
        )?),
    };
    let cancellation = CancellationToken::new();
    let in_flight_work = Arc::new(AtomicUsize::new(0));
    let worker = tokio::spawn(run_worker(
        Arc::clone(&queue),
        processor,
        config.clone(),
        cancellation.clone(),
        Arc::clone(&telemetry_metrics),
        Arc::clone(&heartbeats),
        Arc::clone(&in_flight_work),
    ));
    let heartbeat = tokio::spawn(run_heartbeats(
        heartbeats,
        config.clone(),
        cancellation.clone(),
        in_flight_work,
    ));
    let reprocessing = tokio::spawn(run_reprocessing(
        reprocessing_processor,
        config.clone(),
        cancellation.clone(),
        Arc::clone(&telemetry_metrics),
    ));
    let indexer = tokio::spawn(run_indexer(
        index_repository,
        search_index,
        config.clone(),
        cancellation.clone(),
        Arc::clone(&telemetry_metrics),
    ));
    let listener = TcpListener::bind(config.bind_address()).await?;
    let address = listener.local_addr()?;
    info!(%address, consumer = %config.consumer_name().as_str(), "TechAtlas worker health server listening");

    let result = axum::serve(listener, health_router(health))
        .with_graceful_shutdown(shutdown_signal())
        .await;
    cancellation.cancel();
    let _ = worker.await;
    let _ = heartbeat.await;
    let _ = reprocessing.await;
    let _ = indexer.await;
    result?;
    Ok(())
}

async fn run_reprocessing(
    processor: Arc<ReprocessingProcessor>,
    config: WorkerConfig,
    cancellation: CancellationToken,
    metrics: Arc<TelemetryMetrics>,
) {
    while !cancellation.is_cancelled() {
        let started = std::time::Instant::now();
        match processor.process_next().await {
            Ok(true) => {
                metrics.record_operation("detection_reprocessing", "success", started.elapsed())
            }
            Ok(false) => tokio::select! {
                _ = cancellation.cancelled() => break,
                _ = sleep(config.reprocessing_poll_interval()) => {},
            },
            Err(error) => {
                metrics.record_operation("detection_reprocessing", "error", started.elapsed());
                warn!(error = %error, "reprocessing item could not be processed");
                wait_after_error(&cancellation, config.error_backoff()).await;
            }
        }
    }
}

async fn run_worker(
    queue: Arc<RedisStreamsCrawlJobQueue>,
    processor: Arc<WorkerProcessor>,
    config: WorkerConfig,
    cancellation: CancellationToken,
    metrics: Arc<TelemetryMetrics>,
    heartbeats: Arc<dyn WorkerHeartbeatOperations>,
    in_flight_work: Arc<AtomicUsize>,
) {
    let semaphore = Arc::new(Semaphore::new(config.max_concurrency()));
    while !cancellation.is_cancelled() {
        let mut jobs = match queue.reclaim_stale(config.consumer_name()).await {
            Ok(jobs) => jobs,
            Err(error) => {
                warn!(error = %error, "worker failed to reclaim stale crawl jobs");
                wait_after_error(&cancellation, config.error_backoff()).await;
                continue;
            }
        };
        match queue.receive(config.consumer_name()).await {
            Ok(mut received) => jobs.append(&mut received),
            Err(error) => {
                warn!(error = %error, "worker failed to receive crawl jobs");
                wait_after_error(&cancellation, config.error_backoff()).await;
                continue;
            }
        }
        if jobs.is_empty() {
            continue;
        }

        let mut tasks = JoinSet::new();
        for job in jobs {
            metrics.set_in_flight_work(
                i64::try_from(tasks.len().saturating_add(1)).unwrap_or(i64::MAX),
            );
            in_flight_work.store(tasks.len().saturating_add(1), Ordering::Relaxed);
            let permit = match semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => break,
            };
            let queue = Arc::clone(&queue);
            let processor = Arc::clone(&processor);
            let consumer = config.consumer_name().clone();
            let cancellation = cancellation.clone();
            let metrics = Arc::clone(&metrics);
            let heartbeats = Arc::clone(&heartbeats);
            let worker_name = config.consumer_name().as_str().to_owned();
            let worker_region = config.worker_region().to_owned();
            let in_flight_work = Arc::clone(&in_flight_work);
            let job_id = job.job().job_id().as_uuid();
            let domain_id = job.job().domain_id().as_uuid();
            let correlation_id = job.job().correlation_id().as_uuid();
            tasks.spawn(async move {
                let _permit = permit;
                let started = std::time::Instant::now();
                match process_and_acknowledge(&*queue, &processor, job, &cancellation).await {
                    Ok(()) => {
                        metrics.record_operation("crawl_job", "success", started.elapsed());
                        metrics.mark_successful_work(time::OffsetDateTime::now_utc().unix_timestamp());
                        if let Err(error) = heartbeats
                            .record_completion(WorkerHeartbeat {
                                name: worker_name.clone(),
                                region: worker_region.clone(),
                                in_flight_work: u32::try_from(in_flight_work.load(Ordering::Relaxed)).unwrap_or(u32::MAX),
                                observed_at: time::OffsetDateTime::now_utc(),
                            })
                            .await
                        {
                            warn!(error = %error, worker = %worker_name, "worker completion heartbeat could not be recorded");
                        }
                        info!(%job_id, %domain_id, %correlation_id, consumer = %consumer.as_str(), "crawl job completed and acknowledged")
                    }
                    Err(error) => {
                        metrics.record_operation("crawl_job", "error", started.elapsed());
                        warn!(%job_id, %domain_id, %correlation_id, consumer = %consumer.as_str(), error = %error, "crawl job not acknowledged; Redis will redeliver it")
                    }
                }
                let _ = in_flight_work.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| current.checked_sub(1));
            }.instrument(tracing::info_span!("worker.crawl_job", %job_id, %domain_id, %correlation_id)));
        }
        while tasks.join_next().await.is_some() {}
        in_flight_work.store(0, Ordering::Relaxed);
        metrics.set_in_flight_work(0);
    }
    info!(consumer = %config.consumer_name().as_str(), "worker stopped");
}

async fn run_heartbeats(
    heartbeats: Arc<dyn WorkerHeartbeatOperations>,
    config: WorkerConfig,
    cancellation: CancellationToken,
    in_flight_work: Arc<AtomicUsize>,
) {
    while !cancellation.is_cancelled() {
        let result = heartbeats
            .heartbeat(WorkerHeartbeat {
                name: config.consumer_name().as_str().to_owned(),
                region: config.worker_region().to_owned(),
                in_flight_work: u32::try_from(in_flight_work.load(Ordering::Relaxed))
                    .unwrap_or(u32::MAX),
                observed_at: time::OffsetDateTime::now_utc(),
            })
            .await;
        if let Err(error) = result {
            warn!(error = %error, worker = %config.consumer_name().as_str(), "worker heartbeat could not be recorded");
        }
        wait_after_error(&cancellation, config.heartbeat_interval()).await;
    }
}

async fn wait_after_error(cancellation: &CancellationToken, delay: Duration) {
    tokio::select! {
        _ = cancellation.cancelled() => {}
        _ = sleep(delay) => {}
    }
}

async fn run_indexer(
    repository: Arc<PostgresSearchIndexRepository>,
    index: Arc<MeilisearchDomainIndex>,
    config: WorkerConfig,
    cancellation: CancellationToken,
    metrics: Arc<TelemetryMetrics>,
) {
    while !cancellation.is_cancelled() {
        let now = time::OffsetDateTime::now_utc();
        let jobs = match repository
            .claim_ready(
                now,
                config.index_operation_timeout(),
                config.index_batch_size(),
            )
            .await
        {
            Ok(jobs) => jobs,
            Err(error) => {
                warn!(error = %error, "search index outbox claim failed");
                wait_after_error(&cancellation, config.index_poll_interval()).await;
                continue;
            }
        };
        for job in jobs {
            let result = match repository.projection(job.domain_id).await {
                Ok(Some(projection)) => {
                    index
                        .upsert(vec![DomainSearchDocument::from(projection)])
                        .await
                }
                Ok(None) => Ok(()),
                Err(error) => {
                    warn!(domain_id = %job.domain_id, error = %error, "search projection load failed");
                    Err(techatlas_search::SearchIndexError::Unavailable)
                }
            };
            match result {
                Ok(()) => {
                    if let Err(error) = repository
                        .complete(job, time::OffsetDateTime::now_utc())
                        .await
                    {
                        warn!(domain_id = %job.domain_id, error = %error, "search index completion could not be recorded");
                    }
                }
                Err(error) => {
                    let retry_after = retry_delay(config.index_retry_base(), job.attempt_count);
                    if let Err(repository_error) = repository
                        .fail(
                            job,
                            time::OffsetDateTime::now_utc(),
                            retry_after,
                            config.index_max_attempts(),
                            &error.to_string(),
                        )
                        .await
                    {
                        warn!(domain_id = %job.domain_id, error = %repository_error, "search index failure could not be recorded");
                    } else {
                        warn!(domain_id = %job.domain_id, attempt = job.attempt_count, error = %error, "search index update failed");
                    }
                }
            }
        }
        match repository.lag(time::OffsetDateTime::now_utc()).await {
            Ok(lag) => {
                metrics.set_value(
                    "index_lag_seconds",
                    lag.pending_age.map_or(0, |age| age.whole_seconds()),
                );
                metrics.set_value("index_dead_jobs", lag.dead_count);
                info!(pending_age_seconds = ?lag.pending_age.map(|age| age.whole_seconds()), dead_count = lag.dead_count, "search index lag")
            }
            Err(error) => warn!(error = %error, "search index lag query failed"),
        }
        wait_after_error(&cancellation, config.index_poll_interval()).await;
    }
}

fn retry_delay(base: Duration, attempt: u8) -> Duration {
    let exponent = u32::from(attempt.saturating_sub(1)).min(10);
    base.checked_mul(2_u32.pow(exponent)).unwrap_or(base)
}

#[derive(Clone)]
struct HealthState {
    metrics: Arc<TelemetryMetrics>,
    postgres: Arc<dyn DependencyProbe>,
    redis: Arc<dyn DependencyProbe>,
    meilisearch: Arc<dyn DependencyProbe>,
}

fn health_router(state: HealthState) -> Router {
    Router::new()
        .route("/healthz", get(liveness))
        .route("/readyz", get(readiness))
        .route("/metrics", get(metrics_endpoint))
        .with_state(state)
}

async fn liveness() -> Json<LivenessResponse> {
    Json(LivenessResponse { status: "ok" })
}

async fn readiness(State(state): State<HealthState>) -> impl axum::response::IntoResponse {
    let (postgres, redis, meilisearch) = tokio::join!(
        state.postgres.check(),
        state.redis.check(),
        state.meilisearch.check(),
    );
    let ready = postgres.is_ready() && redis.is_ready() && meilisearch.is_ready();
    state
        .metrics
        .set_dependency_ready("postgres", postgres.is_ready());
    state
        .metrics
        .set_dependency_ready("redis", redis.is_ready());
    state
        .metrics
        .set_dependency_ready("meilisearch", meilisearch.is_ready());
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(ReadinessResponse {
            status: if ready { "ready" } else { "not_ready" },
            postgres,
            redis,
            meilisearch,
        }),
    )
}

async fn metrics_endpoint(State(state): State<HealthState>) -> Response {
    let (postgres, redis, meilisearch) = tokio::join!(
        state.postgres.check(),
        state.redis.check(),
        state.meilisearch.check(),
    );
    state
        .metrics
        .set_dependency_ready("postgres", postgres.is_ready());
    state
        .metrics
        .set_dependency_ready("redis", redis.is_ready());
    state
        .metrics
        .set_dependency_ready("meilisearch", meilisearch.is_ready());
    match state.metrics.encode() {
        Ok(body) => (
            [(
                header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            body,
        )
            .into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

#[derive(Serialize)]
struct LivenessResponse {
    status: &'static str,
}

#[derive(Serialize)]
struct ReadinessResponse {
    status: &'static str,
    postgres: DependencyStatus,
    redis: DependencyStatus,
    meilisearch: DependencyStatus,
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = signal::ctrl_c() => {},
                    _ = terminate.recv() => {},
                }
            }
            Err(_) => {
                let _ = signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = signal::ctrl_c().await;
    }
    info!("shutdown signal received");
}

#[derive(Clone, Debug)]
struct WorkerConfig {
    bind_address: SocketAddr,
    database_url: SecretString,
    redis_url: SecretString,
    consumer_name: CrawlJobConsumerName,
    worker_region: String,
    log_filter: String,
    max_concurrency: usize,
    operation_timeout: Duration,
    error_backoff: Duration,
    heartbeat_interval: Duration,
    queue_config: RedisStreamsConfig,
    politeness_config: RedisPolitenessConfig,
    crawler_config: CrawlerConfig,
    artifact_root: PathBuf,
    artifact_retention_days: u16,
    scheduler_settings: SchedulerSettings,
    meilisearch_url: url::Url,
    meilisearch_master_key: SecretString,
    index_poll_interval: Duration,
    index_batch_size: usize,
    index_max_attempts: u8,
    index_retry_base: Duration,
    index_operation_timeout: Duration,
    reprocessing_poll_interval: Duration,
    reprocessing_retry_delay: time::Duration,
    reprocessing_max_attempts: u16,
    geoip_database: Option<GeoipDatabaseConfig>,
}

#[derive(Clone, Debug)]
struct GeoipDatabaseConfig {
    path: PathBuf,
    version: String,
}

impl GeoipDatabaseConfig {
    fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn version(&self) -> &str {
        &self.version
    }
}

impl WorkerConfig {
    fn from_env() -> Result<Self, WorkerConfigError> {
        Self::from_pairs(env::vars())
    }

    fn from_pairs<I>(pairs: I) -> Result<Self, WorkerConfigError>
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let values = pairs.into_iter().collect::<BTreeMap<_, _>>();
        let bind_address = optional(&values, "WORKER_BIND_ADDRESS", "0.0.0.0:3002")
            .parse()
            .map_err(|_| WorkerConfigError::Invalid("WORKER_BIND_ADDRESS"))?;
        let database_url = required_url(&values, "DATABASE_URL", &["postgres", "postgresql"])?;
        let redis_url = required_url(&values, "REDIS_URL", &["redis", "rediss"])?;
        let consumer_name = CrawlJobConsumerName::parse(&optional(
            &values,
            "WORKER_CONSUMER_NAME",
            "worker-local-1",
        ))
        .map_err(|_| WorkerConfigError::Invalid("WORKER_CONSUMER_NAME"))?;
        let worker_region = optional(&values, "WORKER_REGION", "local");
        if worker_region.trim().is_empty() {
            return Err(WorkerConfigError::Invalid("WORKER_REGION"));
        }
        let operation_timeout = positive_millis(&values, "WORKER_OPERATION_TIMEOUT_MS", 5_000)?;
        let queue_batch_size = positive_usize(&values, "WORKER_QUEUE_BATCH_SIZE", 10)?;
        let queue_read_block = positive_millis(&values, "WORKER_QUEUE_READ_BLOCK_MS", 1_000)?;
        let queue_claim_idle = positive_millis(&values, "WORKER_QUEUE_CLAIM_IDLE_MS", 60_000)?;
        let queue_max_deliveries = positive_usize(&values, "WORKER_QUEUE_MAX_DELIVERIES", 5)?;
        let queue_config = RedisStreamsConfig::default()
            .with_read_options(queue_batch_size, queue_read_block, operation_timeout)
            .map_err(|_| WorkerConfigError::Invalid("worker queue read configuration"))?
            .with_recovery_policy(queue_claim_idle, queue_max_deliveries)
            .map_err(|_| WorkerConfigError::Invalid("worker queue recovery configuration"))?;
        let politeness_config = RedisPolitenessConfig::new(
            &optional(
                &values,
                "WORKER_POLITENESS_KEY_PREFIX",
                "techatlas:crawl-politeness:v1",
            ),
            operation_timeout,
        )
        .map_err(|_| WorkerConfigError::Invalid("worker politeness configuration"))?;
        let crawler_config = crawler_config(&values)?;
        let artifact_root = PathBuf::from(optional(
            &values,
            "WORKER_ARTIFACT_ROOT",
            ".techatlas/artifacts",
        ));
        if artifact_root.as_os_str().is_empty() {
            return Err(WorkerConfigError::Invalid("WORKER_ARTIFACT_ROOT"));
        }
        let artifact_retention_days = positive_u16(&values, "WORKER_ARTIFACT_RETENTION_DAYS", 30)?;
        let max_crawl_retries = zero_or_positive_u8(&values, "SCHEDULER_MAX_CRAWL_RETRIES", 3)?;
        let scheduler_settings = SchedulerSettings::new(
            max_crawl_retries,
            positive_millis(&values, "SCHEDULER_CRAWL_RETRY_BASE_MS", 300_000)?,
            positive_u8(&values, "SCHEDULER_MAX_OUTBOX_PUBLISH_ATTEMPTS", 5)?,
            positive_millis(&values, "SCHEDULER_OUTBOX_RETRY_BASE_MS", 5_000)?,
        )
        .map_err(|_| WorkerConfigError::Invalid("scheduler retry configuration"))?;
        let meilisearch_url = url::Url::parse(&required_url(
            &values,
            "MEILISEARCH_URL",
            &["http", "https"],
        )?)
        .map_err(|_| WorkerConfigError::Invalid("MEILISEARCH_URL"))?;
        let meilisearch_master_key = values
            .get("MEILI_MASTER_KEY")
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .ok_or(WorkerConfigError::Missing("MEILI_MASTER_KEY"))?;
        let geoip_database = values
            .get("WORKER_GEOIP_DATABASE_PATH")
            .filter(|value| !value.trim().is_empty())
            .map(|path| {
                let path = PathBuf::from(path);
                if path.as_os_str().is_empty() {
                    return Err(WorkerConfigError::Invalid("WORKER_GEOIP_DATABASE_PATH"));
                }
                Ok(GeoipDatabaseConfig {
                    path,
                    version: required(&values, "WORKER_GEOIP_DATABASE_VERSION")?,
                })
            })
            .transpose()?;
        let reprocessing_poll_interval =
            positive_millis(&values, "WORKER_REPROCESSING_POLL_INTERVAL_MS", 1_000)?;
        let reprocessing_retry_delay = time::Duration::try_from(positive_millis(
            &values,
            "WORKER_REPROCESSING_RETRY_DELAY_MS",
            5_000,
        )?)
        .map_err(|_| WorkerConfigError::Invalid("WORKER_REPROCESSING_RETRY_DELAY_MS"))?;
        let reprocessing_max_attempts =
            positive_u16(&values, "WORKER_REPROCESSING_MAX_ATTEMPTS", 3)?;

        Ok(Self {
            bind_address,
            database_url: SecretString::from(database_url),
            redis_url: SecretString::from(redis_url),
            consumer_name,
            worker_region,
            log_filter: optional(&values, "RUST_LOG", "techatlas_worker=info"),
            max_concurrency: positive_usize(&values, "WORKER_MAX_CONCURRENCY", 4)?,
            operation_timeout,
            error_backoff: positive_millis(&values, "WORKER_ERROR_BACKOFF_MS", 1_000)?,
            heartbeat_interval: positive_millis(&values, "WORKER_HEARTBEAT_INTERVAL_MS", 15_000)?,
            queue_config,
            politeness_config,
            crawler_config,
            artifact_root,
            artifact_retention_days,
            scheduler_settings,
            meilisearch_url,
            meilisearch_master_key: SecretString::from(meilisearch_master_key),
            index_poll_interval: positive_millis(&values, "WORKER_INDEX_POLL_INTERVAL_MS", 5_000)?,
            index_batch_size: positive_usize(&values, "WORKER_INDEX_BATCH_SIZE", 100)?,
            index_max_attempts: positive_u8(&values, "WORKER_INDEX_MAX_ATTEMPTS", 5)?,
            index_retry_base: positive_millis(&values, "WORKER_INDEX_RETRY_BASE_MS", 1_000)?,
            index_operation_timeout: positive_millis(
                &values,
                "WORKER_INDEX_OPERATION_TIMEOUT_MS",
                10_000,
            )?,
            reprocessing_poll_interval,
            reprocessing_retry_delay,
            reprocessing_max_attempts,
            geoip_database,
        })
    }

    fn bind_address(&self) -> SocketAddr {
        self.bind_address
    }
    fn database_url(&self) -> &SecretString {
        &self.database_url
    }
    fn redis_url(&self) -> &SecretString {
        &self.redis_url
    }
    fn consumer_name(&self) -> &CrawlJobConsumerName {
        &self.consumer_name
    }

    fn reprocessing_poll_interval(&self) -> Duration {
        self.reprocessing_poll_interval
    }
    fn reprocessing_retry_delay(&self) -> time::Duration {
        self.reprocessing_retry_delay
    }
    fn reprocessing_max_attempts(&self) -> u16 {
        self.reprocessing_max_attempts
    }
    fn worker_region(&self) -> &str {
        &self.worker_region
    }
    fn log_filter(&self) -> &str {
        &self.log_filter
    }
    fn max_concurrency(&self) -> usize {
        self.max_concurrency
    }
    fn operation_timeout(&self) -> Duration {
        self.operation_timeout
    }
    fn error_backoff(&self) -> Duration {
        self.error_backoff
    }
    fn heartbeat_interval(&self) -> Duration {
        self.heartbeat_interval
    }
    fn queue_config(&self) -> RedisStreamsConfig {
        self.queue_config.clone()
    }
    fn politeness_config(&self) -> RedisPolitenessConfig {
        self.politeness_config.clone()
    }
    fn crawler_config(&self) -> &CrawlerConfig {
        &self.crawler_config
    }
    fn artifact_root(&self) -> PathBuf {
        self.artifact_root.clone()
    }
    fn artifact_retention(&self) -> time::Duration {
        time::Duration::days(i64::from(self.artifact_retention_days))
    }
    fn scheduler_settings(&self) -> SchedulerSettings {
        self.scheduler_settings
    }
    fn meilisearch_url(&self) -> &url::Url {
        &self.meilisearch_url
    }
    fn meilisearch_master_key(&self) -> &SecretString {
        &self.meilisearch_master_key
    }
    fn index_poll_interval(&self) -> Duration {
        self.index_poll_interval
    }
    fn index_batch_size(&self) -> usize {
        self.index_batch_size
    }
    fn index_max_attempts(&self) -> u8 {
        self.index_max_attempts
    }
    fn index_retry_base(&self) -> Duration {
        self.index_retry_base
    }
    fn index_operation_timeout(&self) -> Duration {
        self.index_operation_timeout
    }
    fn geoip_database(&self) -> Option<&GeoipDatabaseConfig> {
        self.geoip_database.as_ref()
    }
}

fn crawler_config(values: &BTreeMap<String, String>) -> Result<CrawlerConfig, WorkerConfigError> {
    let mut config = CrawlerConfig::default();
    config.user_agent = optional(values, "WORKER_CRAWLER_USER_AGENT", &config.user_agent);
    config.robots_user_agent = optional(
        values,
        "WORKER_CRAWLER_ROBOTS_USER_AGENT",
        &config.robots_user_agent,
    );
    config.contact = optional(values, "WORKER_CRAWLER_CONTACT", &config.contact);
    config.connect_timeout = positive_millis(values, "WORKER_CRAWLER_CONNECT_TIMEOUT_MS", 5_000)?;
    config.read_timeout = positive_millis(values, "WORKER_CRAWLER_READ_TIMEOUT_MS", 15_000)?;
    config.total_timeout = positive_millis(values, "WORKER_CRAWLER_TOTAL_TIMEOUT_MS", 30_000)?;
    config.dns_timeout = positive_millis(values, "WORKER_CRAWLER_DNS_TIMEOUT_MS", 5_000)?;
    config.dns_max_addresses = positive_usize(values, "WORKER_CRAWLER_DNS_MAX_ADDRESSES", 16)?;
    config.tls_timeout = positive_millis(values, "WORKER_CRAWLER_TLS_TIMEOUT_MS", 5_000)?;
    config.tls_max_addresses = positive_usize(values, "WORKER_CRAWLER_TLS_MAX_ADDRESSES", 4)?;
    config.max_redirects = positive_usize(values, "WORKER_CRAWLER_MAX_REDIRECTS", 10)?;
    config.max_response_bytes = positive_u64(
        values,
        "WORKER_CRAWLER_MAX_RESPONSE_BYTES",
        10 * 1024 * 1024,
    )?;
    config.politeness_delay = positive_millis(values, "WORKER_CRAWLER_POLITENESS_DELAY_MS", 1_000)?;
    config
        .validate()
        .map_err(|_| WorkerConfigError::Invalid("worker crawler configuration"))?;
    Ok(config)
}

fn optional(values: &BTreeMap<String, String>, name: &str, default: &str) -> String {
    values
        .get(name)
        .cloned()
        .unwrap_or_else(|| default.to_owned())
}

fn required_url(
    values: &BTreeMap<String, String>,
    name: &'static str,
    allowed_schemes: &[&str],
) -> Result<String, WorkerConfigError> {
    let value = values.get(name).ok_or(WorkerConfigError::Missing(name))?;
    let parsed = url::Url::parse(value).map_err(|_| WorkerConfigError::Invalid(name))?;
    if !allowed_schemes.contains(&parsed.scheme()) || parsed.host_str().is_none() {
        return Err(WorkerConfigError::Invalid(name));
    }
    Ok(value.to_owned())
}

fn required(
    values: &BTreeMap<String, String>,
    name: &'static str,
) -> Result<String, WorkerConfigError> {
    values
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or(WorkerConfigError::Missing(name))
}

struct GeoLiteCountryEnricher {
    reader: maxminddb::Reader<Vec<u8>>,
    version: String,
}

impl GeoLiteCountryEnricher {
    fn open(path: &std::path::Path, version: &str) -> Result<Self, WorkerConfigError> {
        let reader = maxminddb::Reader::open_readfile(path)
            .map_err(|_| WorkerConfigError::Invalid("WORKER_GEOIP_DATABASE_PATH"))?;
        Ok(Self {
            reader,
            version: version.to_owned(),
        })
    }
}

impl CountryEnricher for GeoLiteCountryEnricher {
    fn observe(
        &self,
        addresses: &[std::net::IpAddr],
    ) -> Option<techatlas_models::CountryObservation> {
        for address in addresses {
            let Ok(lookup) = self.reader.lookup(*address) else {
                continue;
            };
            let Ok(Some(country)) = lookup.decode::<maxminddb::geoip2::Country<'_>>() else {
                continue;
            };
            let Some(code) = country.country.iso_code else {
                continue;
            };
            if let Ok(observation) = techatlas_models::CountryObservation::new(
                code,
                "maxmind-geolite2-country",
                &self.version,
            ) {
                return Some(observation);
            }
        }
        None
    }
}

fn positive_millis(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: u64,
) -> Result<Duration, WorkerConfigError> {
    let value = values.get(name).map(String::as_str).unwrap_or_default();
    let value = if value.is_empty() {
        default
    } else {
        value
            .parse()
            .map_err(|_| WorkerConfigError::Invalid(name))?
    };
    if value == 0 {
        return Err(WorkerConfigError::Invalid(name));
    }
    Ok(Duration::from_millis(value))
}

fn positive_usize(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: usize,
) -> Result<usize, WorkerConfigError> {
    let value = values.get(name).map(String::as_str).unwrap_or_default();
    let value = if value.is_empty() {
        default
    } else {
        value
            .parse()
            .map_err(|_| WorkerConfigError::Invalid(name))?
    };
    if value == 0 {
        return Err(WorkerConfigError::Invalid(name));
    }
    Ok(value)
}

fn positive_u64(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: u64,
) -> Result<u64, WorkerConfigError> {
    let value = values.get(name).map(String::as_str).unwrap_or_default();
    let value = if value.is_empty() {
        default
    } else {
        value
            .parse()
            .map_err(|_| WorkerConfigError::Invalid(name))?
    };
    if value == 0 {
        return Err(WorkerConfigError::Invalid(name));
    }
    Ok(value)
}

fn positive_u8(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: u8,
) -> Result<u8, WorkerConfigError> {
    let value = values.get(name).map(String::as_str).unwrap_or_default();
    let value = if value.is_empty() {
        default
    } else {
        value
            .parse()
            .map_err(|_| WorkerConfigError::Invalid(name))?
    };
    if value == 0 {
        return Err(WorkerConfigError::Invalid(name));
    }
    Ok(value)
}

fn positive_u16(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: u16,
) -> Result<u16, WorkerConfigError> {
    let value = values.get(name).map(String::as_str).unwrap_or_default();
    let value = if value.is_empty() {
        default
    } else {
        value
            .parse()
            .map_err(|_| WorkerConfigError::Invalid(name))?
    };
    if value == 0 {
        return Err(WorkerConfigError::Invalid(name));
    }
    Ok(value)
}

fn zero_or_positive_u8(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: u8,
) -> Result<u8, WorkerConfigError> {
    values.get(name).map_or(Ok(default), |value| {
        value.parse().map_err(|_| WorkerConfigError::Invalid(name))
    })
}

#[derive(Debug, Error, PartialEq, Eq)]
enum WorkerConfigError {
    #[error("missing required environment variable: {0}")]
    Missing(&'static str),
    #[error("invalid worker configuration: {0}")]
    Invalid(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_configuration_uses_safe_defaults() {
        let config = WorkerConfig::from_pairs([
            (
                "DATABASE_URL".to_owned(),
                "postgres://localhost/techatlas".to_owned(),
            ),
            ("REDIS_URL".to_owned(), "redis://localhost".to_owned()),
            (
                "MEILISEARCH_URL".to_owned(),
                "http://localhost:7700".to_owned(),
            ),
            ("MEILI_MASTER_KEY".to_owned(), "test-master-key".to_owned()),
        ])
        .expect("minimum worker configuration should be valid");
        assert_eq!(config.max_concurrency(), 4);
        assert_eq!(config.crawler_config().max_response_bytes, 10 * 1024 * 1024);
        assert_eq!(config.crawler_config().dns_timeout, Duration::from_secs(5));
        assert_eq!(config.crawler_config().dns_max_addresses, 16);
        assert_eq!(config.crawler_config().tls_timeout, Duration::from_secs(5));
        assert_eq!(config.crawler_config().tls_max_addresses, 4);
        assert_eq!(config.consumer_name().as_str(), "worker-local-1");
        assert_eq!(
            config.artifact_root(),
            PathBuf::from(".techatlas/artifacts")
        );
        assert_eq!(config.artifact_retention(), time::Duration::days(30));
        assert!(config.geoip_database().is_none());
    }

    #[test]
    fn worker_configuration_requires_a_version_when_geoip_is_configured() {
        let error = WorkerConfig::from_pairs([
            (
                "DATABASE_URL".to_owned(),
                "postgres://localhost/techatlas".to_owned(),
            ),
            ("REDIS_URL".to_owned(), "redis://localhost".to_owned()),
            (
                "MEILISEARCH_URL".to_owned(),
                "http://localhost:7700".to_owned(),
            ),
            ("MEILI_MASTER_KEY".to_owned(), "test-master-key".to_owned()),
            (
                "WORKER_GEOIP_DATABASE_PATH".to_owned(),
                "/tmp/GeoLite2-Country.mmdb".to_owned(),
            ),
        ])
        .expect_err("a GeoIP database requires its provenance version");

        assert_eq!(
            error,
            WorkerConfigError::Missing("WORKER_GEOIP_DATABASE_VERSION")
        );
    }
}
