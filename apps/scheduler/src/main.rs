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
use std::{collections::BTreeMap, env, error::Error, net::SocketAddr, sync::Arc, time::Duration};
use techatlas_common::{DependencyProbe, DependencyStatus};
use techatlas_database::{
    PostgresAdoptionProjectionRepository, PostgresCrawlScheduleRepository, PostgresProbe,
    connect_lazy_pool,
};
use techatlas_models::{AdoptionProjectionOperations, CrawlScheduleRepository, SchedulerSettings};
use techatlas_queue::{
    CrawlJobPublisher, RedisProbe, RedisStreamsConfig, RedisStreamsCrawlJobQueue,
};
use techatlas_telemetry::{TelemetryMetrics, init_tracing};
use thiserror::Error;
use time::OffsetDateTime;
use tokio::{net::TcpListener, signal, time::MissedTickBehavior};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = SchedulerConfig::from_env()?;
    init_tracing("techatlas-scheduler", config.log_filter())?;

    let pool = connect_lazy_pool(config.database_url(), config.operation_timeout())?;
    let repository = Arc::new(PostgresCrawlScheduleRepository::new(pool.clone()));
    let adoption_projection = Arc::new(PostgresAdoptionProjectionRepository::new(pool.clone()));
    let publisher = Arc::new(RedisStreamsCrawlJobQueue::new(
        config.redis_url(),
        RedisStreamsConfig::default(),
    )?);
    let metrics = Arc::new(TelemetryMetrics::new("techatlas_scheduler")?);
    let state = HealthState {
        metrics: Arc::clone(&metrics),
        postgres: Arc::new(PostgresProbe::from_pool(pool, config.operation_timeout())),
        redis: Arc::new(RedisProbe::new(
            config.redis_url(),
            config.operation_timeout(),
        )?),
    };
    let scheduler = tokio::spawn(run_scheduler(
        Arc::clone(&repository),
        Arc::clone(&adoption_projection),
        Arc::clone(&publisher),
        config.clone(),
        Arc::clone(&metrics),
    ));
    let listener = TcpListener::bind(config.bind_address()).await?;
    let address = listener.local_addr()?;
    info!(%address, "TechAtlas scheduler health server listening");

    let result = axum::serve(listener, health_router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await;
    scheduler.abort();
    let _ = scheduler.await;
    result?;
    Ok(())
}

async fn run_scheduler(
    repository: Arc<PostgresCrawlScheduleRepository>,
    adoption_projection: Arc<PostgresAdoptionProjectionRepository>,
    publisher: Arc<RedisStreamsCrawlJobQueue>,
    config: SchedulerConfig,
    metrics: Arc<TelemetryMetrics>,
) {
    let mut interval = tokio::time::interval(config.poll_interval());
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let started = std::time::Instant::now();
        if let Err(error) = scheduler_tick(
            &*repository,
            &*adoption_projection,
            &*publisher,
            &config,
            &metrics,
        )
        .await
        {
            metrics.record_operation("scheduler_tick", "error", started.elapsed());
            warn!(error = %error, "scheduler tick failed");
        } else {
            metrics.record_operation("scheduler_tick", "success", started.elapsed());
            metrics.mark_successful_work(OffsetDateTime::now_utc().unix_timestamp());
        }
    }
}

#[tracing::instrument(name = "scheduler.tick", skip_all)]
async fn scheduler_tick<R, A, P>(
    repository: &R,
    adoption_projection: &A,
    publisher: &P,
    config: &SchedulerConfig,
    metrics: &TelemetryMetrics,
) -> Result<(), SchedulerRuntimeError>
where
    R: CrawlScheduleRepository,
    A: AdoptionProjectionOperations,
    P: CrawlJobPublisher,
{
    let now = OffsetDateTime::now_utc();
    let recovered = repository
        .recover_stale_attempts(
            now,
            config.stale_attempt_timeout(),
            config.batch_size(),
            config.scheduler_settings(),
        )
        .await?;
    metrics.set_value(
        "scheduler_stale_attempts_recovered",
        i64::try_from(recovered).unwrap_or(i64::MAX),
    );
    if recovered != 0 {
        info!(
            count = recovered,
            "recovered stale crawl attempts through bounded retry policy"
        );
    }
    let reserved = repository.reserve_due(now, config.batch_size()).await?;
    metrics.set_value(
        "scheduler_jobs_reserved",
        i64::try_from(reserved.len()).unwrap_or(i64::MAX),
    );
    if !reserved.is_empty() {
        info!(count = reserved.len(), "reserved due crawl jobs");
    }

    let pending = repository
        .pending_publications(now, config.batch_size())
        .await?;
    metrics.set_value(
        "queue_age_seconds",
        pending
            .first()
            .map_or(0, |job| (now - job.available_at()).whole_seconds().max(0)),
    );
    for pending_job in pending {
        let job = pending_job.job();
        let publish_started = std::time::Instant::now();
        match publisher.publish(job).await {
            Ok(_) => {
                repository.mark_published(job.job_id(), now).await?;
                metrics.record_operation("crawl_job_publish", "success", publish_started.elapsed());
            }
            Err(_) => {
                repository
                    .record_publish_failure(job.job_id(), now, config.scheduler_settings())
                    .await?;
                metrics.record_operation("crawl_job_publish", "error", publish_started.elapsed());
                warn!(job_id = %job.job_id().as_uuid(), "crawl job publication failed");
            }
        }
    }

    capture_daily_adoption_if_due(
        adoption_projection,
        now,
        config.adoption_snapshot_delay_minutes(),
        metrics,
    )
    .await?;

    Ok(())
}

async fn capture_daily_adoption_if_due<A>(
    projection: &A,
    now: OffsetDateTime,
    delay_minutes: u16,
    metrics: &TelemetryMetrics,
) -> Result<(), SchedulerRuntimeError>
where
    A: AdoptionProjectionOperations,
{
    let current_minutes = u16::from(now.hour()) * 60 + u16::from(now.minute());
    if current_minutes < delay_minutes {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let result = match projection.capture_daily_adoption(now.date(), now).await {
        Ok(result) => result,
        Err(error) => {
            metrics.record_operation("adoption_snapshot_capture", "error", started.elapsed());
            return Err(error.into());
        }
    };
    metrics.set_value(
        "adoption_snapshot_rows",
        i64::try_from(result.inserted_rows).unwrap_or(i64::MAX),
    );
    metrics.record_operation("adoption_snapshot_capture", "success", started.elapsed());
    if !result.already_captured {
        info!(
            observed_on = %now.date(),
            inserted_rows = result.inserted_rows,
            "captured daily technology adoption projection"
        );
    }
    Ok(())
}

#[derive(Clone)]
struct HealthState {
    metrics: Arc<TelemetryMetrics>,
    postgres: Arc<dyn DependencyProbe>,
    redis: Arc<dyn DependencyProbe>,
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
    let (postgres, redis) = tokio::join!(state.postgres.check(), state.redis.check());
    let ready = postgres.is_ready() && redis.is_ready();
    state
        .metrics
        .set_dependency_ready("postgres", postgres.is_ready());
    state
        .metrics
        .set_dependency_ready("redis", redis.is_ready());
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(ReadinessResponse {
            status: if ready { "ready" } else { "not_ready" },
            postgres,
            redis,
        }),
    )
}

async fn metrics_endpoint(State(state): State<HealthState>) -> Response {
    let (postgres, redis) = tokio::join!(state.postgres.check(), state.redis.check());
    state
        .metrics
        .set_dependency_ready("postgres", postgres.is_ready());
    state
        .metrics
        .set_dependency_ready("redis", redis.is_ready());
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
struct SchedulerConfig {
    bind_address: SocketAddr,
    database_url: SecretString,
    redis_url: SecretString,
    log_filter: String,
    poll_interval: Duration,
    batch_size: usize,
    stale_attempt_timeout: Duration,
    operation_timeout: Duration,
    adoption_snapshot_delay_minutes: u16,
    scheduler_settings: SchedulerSettings,
}

impl SchedulerConfig {
    fn from_env() -> Result<Self, SchedulerConfigError> {
        Self::from_pairs(env::vars())
    }

    fn from_pairs<I>(pairs: I) -> Result<Self, SchedulerConfigError>
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let values = pairs.into_iter().collect::<BTreeMap<_, _>>();
        let bind_address = optional(&values, "SCHEDULER_BIND_ADDRESS", "0.0.0.0:3001")
            .parse()
            .map_err(|_| SchedulerConfigError::Invalid("SCHEDULER_BIND_ADDRESS"))?;
        let database_url = required_url(&values, "DATABASE_URL", &["postgres", "postgresql"])?;
        let redis_url = required_url(&values, "REDIS_URL", &["redis", "rediss"])?;
        let poll_interval = positive_millis(&values, "SCHEDULER_POLL_INTERVAL_MS", 1_000)?;
        let operation_timeout = positive_millis(&values, "SCHEDULER_OPERATION_TIMEOUT_MS", 5_000)?;
        let batch_size = positive_usize(&values, "SCHEDULER_BATCH_SIZE", 100)?;
        let stale_attempt_timeout =
            positive_millis(&values, "SCHEDULER_STALE_ATTEMPT_TIMEOUT_MS", 900_000)?;
        let max_crawl_retries = positive_u8_or_zero(&values, "SCHEDULER_MAX_CRAWL_RETRIES", 3)?;
        let crawl_retry_base = positive_millis(&values, "SCHEDULER_CRAWL_RETRY_BASE_MS", 300_000)?;
        let max_outbox_publish_attempts =
            positive_u8(&values, "SCHEDULER_MAX_OUTBOX_PUBLISH_ATTEMPTS", 5)?;
        let outbox_retry_base = positive_millis(&values, "SCHEDULER_OUTBOX_RETRY_BASE_MS", 5_000)?;
        let adoption_snapshot_delay_minutes =
            bounded_minutes(&values, "SCHEDULER_ADOPTION_SNAPSHOT_DELAY_MINUTES", 5)?;
        let scheduler_settings = SchedulerSettings::new(
            max_crawl_retries,
            crawl_retry_base,
            max_outbox_publish_attempts,
            outbox_retry_base,
        )
        .map_err(|_| SchedulerConfigError::Invalid("scheduler retry configuration"))?;

        Ok(Self {
            bind_address,
            database_url: SecretString::from(database_url),
            redis_url: SecretString::from(redis_url),
            log_filter: optional(&values, "RUST_LOG", "techatlas_scheduler=info"),
            poll_interval,
            batch_size,
            stale_attempt_timeout,
            operation_timeout,
            adoption_snapshot_delay_minutes,
            scheduler_settings,
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

    fn log_filter(&self) -> &str {
        &self.log_filter
    }

    fn poll_interval(&self) -> Duration {
        self.poll_interval
    }

    fn batch_size(&self) -> usize {
        self.batch_size
    }

    fn stale_attempt_timeout(&self) -> Duration {
        self.stale_attempt_timeout
    }

    fn operation_timeout(&self) -> Duration {
        self.operation_timeout
    }

    fn adoption_snapshot_delay_minutes(&self) -> u16 {
        self.adoption_snapshot_delay_minutes
    }

    fn scheduler_settings(&self) -> SchedulerSettings {
        self.scheduler_settings
    }
}

fn required_url(
    values: &BTreeMap<String, String>,
    name: &'static str,
    accepted_schemes: &[&str],
) -> Result<String, SchedulerConfigError> {
    let value = values
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or(SchedulerConfigError::Missing(name))?;
    has_accepted_scheme(&value, accepted_schemes)
        .then_some(())
        .ok_or(SchedulerConfigError::Invalid(name))?;
    Ok(value)
}

fn has_accepted_scheme(value: &str, accepted_schemes: &[&str]) -> bool {
    let scheme = value.split(':').next().unwrap_or_default();
    accepted_schemes.contains(&scheme)
}

fn optional(values: &BTreeMap<String, String>, name: &str, default: &str) -> String {
    values
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| default.to_owned())
}

fn positive_millis(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: u64,
) -> Result<Duration, SchedulerConfigError> {
    let value = optional(values, name, &default.to_string())
        .parse::<u64>()
        .map_err(|_| SchedulerConfigError::Invalid(name))?;
    if value == 0 {
        return Err(SchedulerConfigError::Invalid(name));
    }
    Ok(Duration::from_millis(value))
}

fn positive_usize(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: usize,
) -> Result<usize, SchedulerConfigError> {
    let value = optional(values, name, &default.to_string())
        .parse::<usize>()
        .map_err(|_| SchedulerConfigError::Invalid(name))?;
    if value == 0 {
        return Err(SchedulerConfigError::Invalid(name));
    }
    Ok(value)
}

fn positive_u8(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: u8,
) -> Result<u8, SchedulerConfigError> {
    let value = optional(values, name, &default.to_string())
        .parse::<u8>()
        .map_err(|_| SchedulerConfigError::Invalid(name))?;
    if value == 0 {
        return Err(SchedulerConfigError::Invalid(name));
    }
    Ok(value)
}

fn positive_u8_or_zero(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: u8,
) -> Result<u8, SchedulerConfigError> {
    optional(values, name, &default.to_string())
        .parse::<u8>()
        .map_err(|_| SchedulerConfigError::Invalid(name))
}

fn bounded_minutes(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: u16,
) -> Result<u16, SchedulerConfigError> {
    let value = optional(values, name, &default.to_string())
        .parse::<u16>()
        .map_err(|_| SchedulerConfigError::Invalid(name))?;
    if value > 1_439 {
        return Err(SchedulerConfigError::Invalid(name));
    }
    Ok(value)
}

#[derive(Debug, Error)]
enum SchedulerConfigError {
    #[error("missing required configuration: {0}")]
    Missing(&'static str),
    #[error("invalid configuration: {0}")]
    Invalid(&'static str),
}

#[derive(Debug, Error)]
enum SchedulerRuntimeError {
    #[error(transparent)]
    Repository(#[from] techatlas_models::SchedulerRepositoryError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_configuration_uses_safe_defaults() {
        let config = SchedulerConfig::from_pairs([
            (
                "DATABASE_URL".to_owned(),
                "postgres://localhost/test".to_owned(),
            ),
            ("REDIS_URL".to_owned(), "redis://localhost".to_owned()),
        ])
        .expect("configuration should parse");

        assert_eq!(config.batch_size(), 100);
        assert_eq!(config.stale_attempt_timeout(), Duration::from_secs(900));
        assert_eq!(config.adoption_snapshot_delay_minutes(), 5);
        assert_eq!(
            config.scheduler_settings().crawl_retry_delay(3),
            Some(Duration::from_secs(1_200))
        );
    }
}
