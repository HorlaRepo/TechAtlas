#![forbid(unsafe_code)]

mod admin_operations_repository;
mod admin_repository;
mod admin_workflow_repository;
mod adoption_projection_repository;
mod artifact_retention;
mod csv_import_repository;
mod domain_repository;
mod public_read_repository;
mod public_refresh_repository;
mod reprocessing_repository;
mod scheduler_repository;
mod search_index_repository;
mod worker_repository;

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use sqlx::{PgPool, migrate::Migrator, postgres::PgPoolOptions};
use std::time::Duration;
use techatlas_common::{DependencyProbe, DependencyStatus};
use thiserror::Error;

pub use admin_operations_repository::PostgresAdminOperationsRepository;
pub use admin_repository::PostgresAdminRepository;
pub use admin_workflow_repository::PostgresAdminWorkflowRepository;
pub use adoption_projection_repository::PostgresAdoptionProjectionRepository;
pub use artifact_retention::{RawArtifactRetentionError, expired_raw_artifact_locations};
pub use csv_import_repository::PostgresCsvImportRepository;
pub use domain_repository::PostgresDomainRepository;
pub use public_read_repository::PostgresPublicReadRepository;
pub use public_refresh_repository::PostgresPublicRefreshRepository;
pub use reprocessing_repository::PostgresReprocessingRepository;
pub use scheduler_repository::PostgresCrawlScheduleRepository;
pub use search_index_repository::{
    PostgresSearchIndexRepository, SearchIndexJob, SearchIndexLag, SearchIndexRepositoryError,
};
pub use worker_repository::PostgresCrawlWorkerRepository;

/// The ordered, forward-only PostgreSQL migrations owned by this repository.
pub static MIGRATOR: Migrator = sqlx::migrate!("../../database/migrations");

pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await
}

pub fn connect_lazy_pool(
    database_url: &SecretString,
    timeout: Duration,
) -> Result<PgPool, PostgresProbeError> {
    PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(timeout)
        .connect_lazy(database_url.expose_secret())
        .map_err(|_| PostgresProbeError::InvalidConnectionConfiguration)
}

pub struct PostgresProbe {
    pool: PgPool,
    timeout: Duration,
}

impl PostgresProbe {
    pub fn new(database_url: &SecretString, timeout: Duration) -> Result<Self, PostgresProbeError> {
        let pool = connect_lazy_pool(database_url, timeout)?;

        Ok(Self { pool, timeout })
    }

    pub fn from_pool(pool: PgPool, timeout: Duration) -> Self {
        Self { pool, timeout }
    }
}

#[async_trait]
impl DependencyProbe for PostgresProbe {
    async fn check(&self) -> DependencyStatus {
        match tokio::time::timeout(
            self.timeout,
            sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&self.pool),
        )
        .await
        {
            Ok(Ok(1)) => DependencyStatus::Ready,
            _ => DependencyStatus::Unavailable,
        }
    }
}

#[derive(Debug, Error)]
pub enum PostgresProbeError {
    #[error("invalid PostgreSQL connection configuration")]
    InvalidConnectionConfiguration,
}
