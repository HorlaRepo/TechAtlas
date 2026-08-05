use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction, types::Json};
use std::collections::BTreeMap;
use techatlas_models::{
    ActiveDetectionRule, RawArtifactCompression, RawArtifactMetadata, ReprocessingEvaluation,
    ReprocessingRepository, ReprocessingRepositoryError, ReprocessingWorkItem,
};
use time::OffsetDateTime;
use uuid::Uuid;

pub struct PostgresReprocessingRepository {
    pool: PgPool,
}

impl PostgresReprocessingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ReprocessingRepository for PostgresReprocessingRepository {
    async fn claim_next_reprocessing_item(
        &self,
        now: OffsetDateTime,
        _: u16,
    ) -> Result<Option<ReprocessingWorkItem>, ReprocessingRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(map_error)?;
        let row = sqlx::query(
            "SELECT items.id, items.run_id, items.crawl_snapshot_id, items.domain_id, items.attempt_count, \
                    snapshots.captured_at, snapshots.response_headers, artifacts.storage_location, \
                    artifacts.checksum_sha256, artifacts.compression, artifacts.uncompressed_size_bytes, \
                    artifacts.compressed_size_bytes, artifacts.retention_expires_at, technologies.id AS technology_id, \
                    versions.id AS detection_rule_version_id, technologies.slug AS technology_slug, \
                    categories.slug AS category_slug, rules.slug AS rule_slug, versions.version, versions.definition \
             FROM detection_reprocessing_work_items AS items \
             JOIN detection_reprocessing_runs AS runs ON runs.id = items.run_id \
             JOIN crawl_snapshots AS snapshots ON snapshots.id = items.crawl_snapshot_id \
             LEFT JOIN raw_artifacts AS artifacts ON artifacts.crawl_snapshot_id = snapshots.id \
             JOIN detection_rule_versions AS versions ON versions.id = runs.detection_rule_version_id \
             JOIN detection_rules AS rules ON rules.id = versions.detection_rule_id \
             JOIN technologies ON technologies.id = rules.technology_id \
             JOIN technology_categories AS categories ON categories.id = technologies.category_id \
             WHERE items.status = 'queued' AND items.next_attempt_at <= $1 \
             ORDER BY items.next_attempt_at, items.created_at \
             FOR UPDATE OF items SKIP LOCKED LIMIT 1",
        )
        .bind(now)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_error)?;
        let Some(row) = row else {
            transaction.commit().await.map_err(map_error)?;
            return Ok(None);
        };
        let id: Uuid = row.try_get("id").map_err(map_error)?;
        let run_id: Uuid = row.try_get("run_id").map_err(map_error)?;
        sqlx::query(
            "UPDATE detection_reprocessing_work_items \
             SET status = 'running', attempt_count = attempt_count + 1, claimed_at = $1, updated_at = $1 \
             WHERE id = $2",
        )
        .bind(now)
        .bind(id)
        .execute(&mut *transaction)
        .await
        .map_err(map_error)?;
        sqlx::query(
            "UPDATE detection_reprocessing_runs \
             SET status = 'running', started_at = COALESCE(started_at, $1) \
             WHERE id = $2 AND status = 'queued'",
        )
        .bind(now)
        .bind(run_id)
        .execute(&mut *transaction)
        .await
        .map_err(map_error)?;
        let headers: BTreeMap<String, String> = row
            .try_get::<Json<BTreeMap<String, String>>, _>("response_headers")
            .map_err(map_error)?
            .0;
        let artifact = match row
            .try_get::<Option<String>, _>("storage_location")
            .map_err(map_error)?
        {
            Some(storage_location) => Some(
                RawArtifactMetadata::response_body(
                    &storage_location,
                    &row.try_get::<String, _>("checksum_sha256")
                        .map_err(map_error)?,
                    match row
                        .try_get::<String, _>("compression")
                        .map_err(map_error)?
                        .as_str()
                    {
                        "zstd" => RawArtifactCompression::Zstd,
                        _ => return Err(ReprocessingRepositoryError::Unavailable),
                    },
                    u64::try_from(
                        row.try_get::<i64, _>("uncompressed_size_bytes")
                            .map_err(map_error)?,
                    )
                    .map_err(|_| ReprocessingRepositoryError::Unavailable)?,
                    u64::try_from(
                        row.try_get::<i64, _>("compressed_size_bytes")
                            .map_err(map_error)?,
                    )
                    .map_err(|_| ReprocessingRepositoryError::Unavailable)?,
                    row.try_get("retention_expires_at").map_err(map_error)?,
                )
                .map_err(|_| ReprocessingRepositoryError::Unavailable)?,
            ),
            None => None,
        };
        let item = ReprocessingWorkItem {
            id,
            run_id,
            snapshot_id: row.try_get("crawl_snapshot_id").map_err(map_error)?,
            domain_id: row.try_get("domain_id").map_err(map_error)?,
            technology_id: row.try_get("technology_id").map_err(map_error)?,
            detection_rule_version_id: row
                .try_get("detection_rule_version_id")
                .map_err(map_error)?,
            captured_at: row.try_get("captured_at").map_err(map_error)?,
            attempt_count: u16::try_from(
                row.try_get::<i16, _>("attempt_count").map_err(map_error)? + 1,
            )
            .map_err(|_| ReprocessingRepositoryError::Unavailable)?,
            response_headers: headers,
            artifact,
            rule: ActiveDetectionRule {
                technology_slug: row.try_get("technology_slug").map_err(map_error)?,
                technology_category_slug: row.try_get("category_slug").map_err(map_error)?,
                rule_slug: row.try_get("rule_slug").map_err(map_error)?,
                version: u16::try_from(row.try_get::<i16, _>("version").map_err(map_error)?)
                    .map_err(|_| ReprocessingRepositoryError::Unavailable)?,
                definition: row
                    .try_get::<Json<Value>, _>("definition")
                    .map_err(map_error)?
                    .0,
            },
        };
        transaction.commit().await.map_err(map_error)?;
        Ok(Some(item))
    }

    async fn record_reprocessing_success(
        &self,
        item: &ReprocessingWorkItem,
        evaluation: ReprocessingEvaluation,
        completed_at: OffsetDateTime,
    ) -> Result<(), ReprocessingRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(map_error)?;
        sqlx::query(
            "INSERT INTO reprocessed_detection_observations \
             (run_id, crawl_snapshot_id, detection_rule_version_id, status, observed_at, created_at) \
             VALUES ($1, $2, $3, $4, $5, $5)",
        )
        .bind(item.run_id)
        .bind(item.snapshot_id)
        .bind(item.detection_rule_version_id)
        .bind(evaluation.observation.status().as_str())
        .bind(item.captured_at)
        .execute(&mut *transaction)
        .await
        .map_err(map_error)?;
        if let Some(detection) = evaluation.detection {
            let detection_id: Uuid = sqlx::query_scalar(
                "INSERT INTO reprocessed_detections \
                 (run_id, crawl_snapshot_id, technology_id, detection_rule_version_id, confidence, method, technology_version, observed_at, created_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8) RETURNING id",
            )
            .bind(item.run_id)
            .bind(item.snapshot_id)
            .bind(item.technology_id)
            .bind(item.detection_rule_version_id)
            .bind(i16::from(detection.confidence().get()))
            .bind(detection.method().as_str())
            .bind(evaluation.technology_version)
            .bind(item.captured_at)
            .fetch_one(&mut *transaction)
            .await
            .map_err(map_error)?;
            for evidence in detection.evidence() {
                sqlx::query(
                    "INSERT INTO reprocessed_detection_evidence \
                     (reprocessed_detection_id, source, evidence_key, evidence_value, created_at) \
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(detection_id)
                .bind(evidence.source().as_str())
                .bind(evidence.key())
                .bind(evidence.value())
                .bind(item.captured_at)
                .execute(&mut *transaction)
                .await
                .map_err(map_error)?;
            }
        }
        sqlx::query(
            "UPDATE detection_reprocessing_work_items \
             SET status = 'succeeded', completed_at = $1, updated_at = $1, failure_summary = NULL \
             WHERE id = $2 AND status = 'running'",
        )
        .bind(completed_at)
        .bind(item.id)
        .execute(&mut *transaction)
        .await
        .map_err(map_error)?;
        refresh_run(&mut transaction, item.run_id, completed_at).await?;
        transaction.commit().await.map_err(map_error)
    }

    async fn record_reprocessing_failure(
        &self,
        item_id: Uuid,
        summary: &str,
        retry_at: OffsetDateTime,
        terminal: bool,
        completed_at: OffsetDateTime,
    ) -> Result<(), ReprocessingRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(map_error)?;
        let run_id: Uuid = sqlx::query_scalar(
            "SELECT run_id FROM detection_reprocessing_work_items WHERE id = $1 FOR UPDATE",
        )
        .bind(item_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(map_error)?;
        let summary = bounded_summary(summary);
        if terminal {
            sqlx::query(
                "UPDATE detection_reprocessing_work_items \
                 SET status = 'dead_lettered', failure_summary = $1, completed_at = $2, updated_at = $2 \
                 WHERE id = $3 AND status = 'running'",
            )
            .bind(summary)
            .bind(completed_at)
            .bind(item_id)
            .execute(&mut *transaction)
            .await
            .map_err(map_error)?;
        } else {
            sqlx::query(
                "UPDATE detection_reprocessing_work_items \
                 SET status = 'queued', failure_summary = $1, next_attempt_at = $2, claimed_at = NULL, updated_at = $3 \
                 WHERE id = $4 AND status = 'running'",
            )
            .bind(summary)
            .bind(retry_at)
            .bind(completed_at)
            .bind(item_id)
            .execute(&mut *transaction)
            .await
            .map_err(map_error)?;
        }
        refresh_run(&mut transaction, run_id, completed_at).await?;
        transaction.commit().await.map_err(map_error)
    }
}

async fn refresh_run(
    transaction: &mut Transaction<'_, Postgres>,
    run_id: Uuid,
    now: OffsetDateTime,
) -> Result<(), ReprocessingRepositoryError> {
    let counts = sqlx::query(
        "SELECT COUNT(*)::BIGINT AS total, \
                COUNT(*) FILTER (WHERE status = 'succeeded')::BIGINT AS succeeded, \
                COUNT(*) FILTER (WHERE status = 'dead_lettered')::BIGINT AS failed, \
                COUNT(*) FILTER (WHERE status IN ('queued', 'running'))::BIGINT AS pending \
         FROM detection_reprocessing_work_items WHERE run_id = $1",
    )
    .bind(run_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(map_error)?;
    let total = counts.try_get::<i64, _>("total").map_err(map_error)?;
    let succeeded = counts.try_get::<i64, _>("succeeded").map_err(map_error)?;
    let failed = counts.try_get::<i64, _>("failed").map_err(map_error)?;
    let pending = counts.try_get::<i64, _>("pending").map_err(map_error)?;
    let status = if pending == 0 {
        if failed == 0 {
            "completed"
        } else {
            "partial_failed"
        }
    } else {
        "running"
    };
    sqlx::query(
        "UPDATE detection_reprocessing_runs \
         SET total_snapshot_count = $1, succeeded_snapshot_count = $2, failed_snapshot_count = $3, \
             status = $4, finished_at = CASE WHEN $5 THEN $6 ELSE NULL END \
         WHERE id = $7",
    )
    .bind(i32::try_from(total).map_err(|_| ReprocessingRepositoryError::Unavailable)?)
    .bind(i32::try_from(succeeded).map_err(|_| ReprocessingRepositoryError::Unavailable)?)
    .bind(i32::try_from(failed).map_err(|_| ReprocessingRepositoryError::Unavailable)?)
    .bind(status)
    .bind(pending == 0)
    .bind(now)
    .bind(run_id)
    .execute(&mut **transaction)
    .await
    .map_err(map_error)?;
    if pending == 0 {
        sqlx::query(
            "WITH ranked AS ( \
                SELECT detections.id, observations.crawl_snapshot_id, snapshots.domain_id, technologies.id AS technology_id, \
                       detections.technology_version, observations.observed_at, \
                       LAG(detections.id) OVER ordered AS prior_detection_id, \
                       LAG(observations.crawl_snapshot_id) OVER ordered AS prior_snapshot_id, \
                       LAG(detections.technology_version) OVER ordered AS prior_version \
                FROM reprocessed_detection_observations AS observations \
                JOIN crawl_snapshots AS snapshots ON snapshots.id = observations.crawl_snapshot_id \
                JOIN detection_rule_versions AS versions ON versions.id = observations.detection_rule_version_id \
                JOIN detection_rules AS rules ON rules.id = versions.detection_rule_id \
                JOIN technologies ON technologies.id = rules.technology_id \
                LEFT JOIN reprocessed_detections AS detections \
                  ON detections.run_id = observations.run_id \
                 AND detections.crawl_snapshot_id = observations.crawl_snapshot_id \
                 AND detections.detection_rule_version_id = observations.detection_rule_version_id \
                WHERE observations.run_id = $1 \
                WINDOW ordered AS (PARTITION BY snapshots.domain_id, technologies.id ORDER BY observations.observed_at, observations.crawl_snapshot_id) \
             ) \
             INSERT INTO technology_version_changes \
                 (reprocessing_run_id, domain_id, technology_id, prior_snapshot_id, current_snapshot_id, \
                  prior_reprocessed_detection_id, current_reprocessed_detection_id, from_version, to_version, observed_at, created_at) \
             SELECT $1, domain_id, technology_id, prior_snapshot_id, crawl_snapshot_id, prior_detection_id, id, \
                    prior_version, technology_version, observed_at, $2 \
             FROM ranked \
             WHERE prior_version IS NOT NULL AND technology_version IS NOT NULL AND prior_version <> technology_version \
             ON CONFLICT (reprocessing_run_id, current_reprocessed_detection_id) DO NOTHING",
        )
        .bind(run_id)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(map_error)?;
    }
    Ok(())
}

fn bounded_summary(summary: &str) -> String {
    summary.trim().chars().take(1024).collect::<String>()
}

fn map_error(_: sqlx::Error) -> ReprocessingRepositoryError {
    ReprocessingRepositoryError::Unavailable
}
