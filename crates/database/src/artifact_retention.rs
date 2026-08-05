use sqlx::PgPool;
use thiserror::Error;
use time::OffsetDateTime;

/// Returns content-addressed artifact locations whose every immutable reference has expired.
///
/// Metadata is retained for provenance; callers may remove only the backing file.
pub async fn expired_raw_artifact_locations(
    pool: &PgPool,
    now: OffsetDateTime,
) -> Result<Vec<String>, RawArtifactRetentionError> {
    sqlx::query_scalar(
        "SELECT storage_location \
         FROM raw_artifacts \
         GROUP BY storage_location \
         HAVING max(retention_expires_at) < $1 \
         ORDER BY storage_location",
    )
    .bind(now)
    .fetch_all(pool)
    .await
    .map_err(|_| RawArtifactRetentionError::Unavailable)
}

#[derive(Debug, Error)]
pub enum RawArtifactRetentionError {
    #[error("raw artifact retention query is unavailable")]
    Unavailable,
}
