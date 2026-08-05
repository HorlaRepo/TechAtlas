use std::{error::Error, sync::Arc};
use techatlas_api::{
    admin_auth::OidcAdminAuthorizer,
    app::{AppState, router},
};
use techatlas_common::AppConfig;
use techatlas_database::{
    PostgresAdminOperationsRepository, PostgresAdminRepository, PostgresAdminWorkflowRepository,
    PostgresCsvImportRepository, PostgresDomainRepository, PostgresProbe,
    PostgresPublicReadRepository, PostgresPublicRefreshRepository, connect_lazy_pool,
};
use techatlas_models::DomainService;
use techatlas_queue::{RedisProbe, RedisStreamsConfig, RedisStreamsCrawlJobQueue};
use techatlas_search::{MeilisearchDomainIndex, MeilisearchProbe};
use techatlas_telemetry::{TelemetryMetrics, init_tracing};
use tokio::{net::TcpListener, signal};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = AppConfig::from_env()?;
    init_tracing("techatlas-api", config.log_filter())?;

    let postgres_pool =
        connect_lazy_pool(config.database_url(), config.dependency_check_timeout())?;
    let state = AppState {
        metrics: Arc::new(TelemetryMetrics::new("techatlas_api")?),
        postgres: Arc::new(PostgresProbe::from_pool(
            postgres_pool.clone(),
            config.dependency_check_timeout(),
        )),
        redis: Arc::new(RedisProbe::new(
            config.redis_url(),
            config.dependency_check_timeout(),
        )?),
        meilisearch: Arc::new(MeilisearchProbe::new(
            config.meilisearch_url(),
            config.meilisearch_master_key(),
            config.dependency_check_timeout(),
        )?),
        admin_domains: Arc::new(DomainService::with_default_policy(
            PostgresDomainRepository::new(postgres_pool.clone()),
        )),
        admin_auth: Arc::new(OidcAdminAuthorizer::new(
            config.admin_oidc_issuer().clone(),
            config.admin_oidc_audience().to_owned(),
            config.admin_oidc_jwks_url().clone(),
            config.dependency_check_timeout(),
            config.admin_rate_limit_per_minute(),
            config.admin_mutation_rate_limit_per_minute(),
        )?),
        admin_imports: Arc::new(PostgresCsvImportRepository::new(postgres_pool.clone())),
        admin_policies: Arc::new(PostgresAdminRepository::new(postgres_pool.clone())),
        admin_audits: Arc::new(PostgresAdminRepository::new(postgres_pool.clone())),
        admin_operations: Arc::new(PostgresAdminOperationsRepository::new(
            postgres_pool.clone(),
        )),
        admin_queue: Arc::new(RedisStreamsCrawlJobQueue::new(
            config.redis_url(),
            RedisStreamsConfig::default(),
        )?),
        admin_scheduler: Arc::new(PostgresAdminWorkflowRepository::new(postgres_pool.clone())),
        admin_rules: Arc::new(PostgresAdminWorkflowRepository::new(postgres_pool.clone())),
        admin_reprocessing: Arc::new(PostgresAdminWorkflowRepository::new(postgres_pool.clone())),
        public_reads: Arc::new(PostgresPublicReadRepository::new(postgres_pool.clone())),
        public_refreshes: Arc::new(PostgresPublicRefreshRepository::new(postgres_pool)),
        public_search: Arc::new(MeilisearchDomainIndex::new(
            config.meilisearch_url(),
            config.meilisearch_master_key(),
            config.dependency_check_timeout(),
        )?),
        public_stale_after_days: config.public_stale_after_days(),
        public_refresh_cooldown_hours: config.public_refresh_cooldown_hours(),
    };
    let listener = TcpListener::bind(config.bind_address()).await?;
    let address = listener.local_addr()?;

    tracing::info!(%address, "TechAtlas API listening");

    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
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
    tracing::info!("shutdown signal received");
}
