use crate::PostgresCrawlScheduleRepository;
use async_trait::async_trait;
use serde_json::json;
use sqlx::{PgPool, Postgres, Row, Transaction, types::Json};
use std::collections::BTreeMap;
use techatlas_models::{
    ActiveDetectionRule, CanonicalDomain, ClaimedCrawlAttempt, CrawlAttemptOutcome, CrawlFailure,
    CrawlJobV1, CrawlScheduleRepository, CrawlWorkerRepository, CrawlWorkerRepositoryError,
    CurrentTechnology, RuleObservation, SchedulerSettings, SuccessfulCrawlRecord,
    derive_projection,
};
use time::OffsetDateTime;
use uuid::Uuid;

pub struct PostgresCrawlWorkerRepository {
    pool: PgPool,
}

impl PostgresCrawlWorkerRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CrawlWorkerRepository for PostgresCrawlWorkerRepository {
    async fn active_detection_rules(
        &self,
    ) -> Result<Vec<ActiveDetectionRule>, CrawlWorkerRepositoryError> {
        let rows = sqlx::query(
            "SELECT technologies.slug AS technology_slug, categories.slug AS category_slug, rules.slug AS rule_slug, versions.version, versions.definition \
             FROM detection_rules AS rules \
             JOIN technologies ON technologies.id = rules.technology_id \
             JOIN technology_categories AS categories ON categories.id = technologies.category_id \
             JOIN detection_rule_versions AS versions ON versions.id = rules.active_version_id \
             ORDER BY rules.slug",
        ).fetch_all(&self.pool).await.map_err(map_database_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(ActiveDetectionRule {
                    technology_slug: row.try_get("technology_slug").map_err(map_database_error)?,
                    technology_category_slug: row
                        .try_get("category_slug")
                        .map_err(map_database_error)?,
                    rule_slug: row.try_get("rule_slug").map_err(map_database_error)?,
                    version: u16::try_from(
                        row.try_get::<i16, _>("version")
                            .map_err(map_database_error)?,
                    )
                    .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?,
                    definition: row
                        .try_get::<Json<serde_json::Value>, _>("definition")
                        .map_err(map_database_error)?
                        .0,
                })
            })
            .collect()
    }

    async fn claim_attempt(
        &self,
        job: &CrawlJobV1,
        started_at: OffsetDateTime,
    ) -> Result<ClaimedCrawlAttempt, CrawlWorkerRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let row = sqlx::query(
            "SELECT crawl_attempts.id, crawl_attempts.status, domains.canonical_domain \
             FROM crawl_attempts \
             JOIN domains ON domains.id = crawl_attempts.domain_id \
             WHERE crawl_attempts.job_id = $1 \
               AND crawl_attempts.domain_id = $2 \
               AND crawl_attempts.correlation_id = $3 \
               AND crawl_attempts.idempotency_key = $4 \
             FOR UPDATE OF crawl_attempts",
        )
        .bind(job.job_id().as_uuid())
        .bind(job.domain_id().as_uuid())
        .bind(job.correlation_id().as_uuid())
        .bind(job.idempotency_key().as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database_error)?
        .ok_or(CrawlWorkerRepositoryError::NotFound)?;
        let status: String = row
            .try_get("status")
            .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;
        if matches!(status.as_str(), "succeeded" | "failed" | "cancelled") {
            transaction.commit().await.map_err(map_database_error)?;
            return Ok(ClaimedCrawlAttempt::Terminal);
        }
        let attempt_id: Uuid = row
            .try_get("id")
            .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;
        let canonical_domain: String = row
            .try_get("canonical_domain")
            .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;
        let canonical_domain = CanonicalDomain::parse(&canonical_domain)
            .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;

        sqlx::query(
            "UPDATE crawl_attempts \
             SET status = 'running', started_at = COALESCE(started_at, $1) \
             WHERE id = $2",
        )
        .bind(started_at)
        .bind(attempt_id)
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        transaction.commit().await.map_err(map_database_error)?;

        Ok(ClaimedCrawlAttempt::Ready { canonical_domain })
    }

    async fn record_success(
        &self,
        job: &CrawlJobV1,
        record: SuccessfulCrawlRecord,
    ) -> Result<(), CrawlWorkerRepositoryError> {
        let SuccessfulCrawlRecord {
            snapshot,
            artifact,
            dns_observation,
            tls_observation,
            detections,
            observations,
        } = record;
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let row = sqlx::query(
            "SELECT crawl_attempts.id, crawl_attempts.domain_id, crawl_attempts.status \
             FROM crawl_attempts \
             JOIN crawl_policies ON crawl_policies.domain_id = crawl_attempts.domain_id \
             WHERE crawl_attempts.job_id = $1 \
               AND crawl_attempts.domain_id = $2 \
               AND crawl_attempts.correlation_id = $3 \
               AND crawl_attempts.idempotency_key = $4 \
             FOR UPDATE OF crawl_attempts, crawl_policies",
        )
        .bind(job.job_id().as_uuid())
        .bind(job.domain_id().as_uuid())
        .bind(job.correlation_id().as_uuid())
        .bind(job.idempotency_key().as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database_error)?
        .ok_or(CrawlWorkerRepositoryError::NotFound)?;
        let status: String = row
            .try_get("status")
            .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;
        if matches!(status.as_str(), "succeeded" | "failed" | "cancelled") {
            transaction.commit().await.map_err(map_database_error)?;
            return Ok(());
        }
        let attempt_id: Uuid = row
            .try_get("id")
            .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;
        let domain_id: Uuid = row
            .try_get("domain_id")
            .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;
        let body_size = i64::try_from(snapshot.body_size_bytes())
            .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;
        let redirects = snapshot
            .redirects()
            .iter()
            .map(|redirect| {
                json!({
                    "from_url": redirect.from_url(),
                    "to_url": redirect.to_url(),
                    "status": redirect.status(),
                })
            })
            .collect::<Vec<_>>();
        let headers = serde_json::to_value(snapshot.response_headers())
            .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;

        sqlx::query(
            "UPDATE crawl_attempts \
             SET status = 'succeeded', started_at = COALESCE(started_at, $1), finished_at = $1 \
             WHERE id = $2",
        )
        .bind(snapshot.captured_at())
        .bind(attempt_id)
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        let snapshot_id: Uuid = sqlx::query_scalar(
            "INSERT INTO crawl_snapshots (crawl_attempt_id, domain_id, requested_url, final_url, \
                 redirect_chain, response_status, response_headers, response_body_bytes, captured_at, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9) \
             RETURNING id",
        )
        .bind(attempt_id)
        .bind(domain_id)
        .bind(snapshot.requested_url())
        .bind(snapshot.final_url())
        .bind(Json(redirects))
        .bind(
            i16::try_from(snapshot.response_status())
                .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?,
        )
        .bind(Json(headers))
        .bind(body_size)
        .bind(snapshot.captured_at())
        .fetch_one(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        if let Some(country) = snapshot.country_observation() {
            sqlx::query(
                "INSERT INTO crawl_country_observations \
                 (crawl_snapshot_id, country_code, source, source_version, observed_at, created_at) \
                 VALUES ($1, $2, $3, $4, $5, $5)",
            )
            .bind(snapshot_id)
            .bind(country.country_code())
            .bind(country.source())
            .bind(country.source_version())
            .bind(snapshot.captured_at())
            .execute(&mut *transaction)
            .await
            .map_err(map_database_error)?;
        }
        let dns_addresses = serde_json::to_value(
            dns_observation
                .addresses()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        )
        .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;
        sqlx::query(
            "INSERT INTO crawl_dns_observations \
             (crawl_snapshot_id, source, observed_at, queried_name, addresses, unavailable_reason, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $3)",
        )
        .bind(snapshot_id)
        .bind(dns_observation.source())
        .bind(dns_observation.observed_at())
        .bind(dns_observation.queried_name())
        .bind(Json(dns_addresses))
        .bind(dns_observation.unavailable_reason().map(|reason| reason.as_str()))
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        let tls_names = serde_json::to_value(tls_observation.subject_alternative_names())
            .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;
        sqlx::query(
            "INSERT INTO crawl_tls_observations \
             (crawl_snapshot_id, source, observed_at, unavailable_reason, validation_status, protocol, \
              cipher_suite, certificate_subject, certificate_issuer, subject_alternative_names, \
              certificate_not_before, certificate_not_after, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $3)",
        )
        .bind(snapshot_id)
        .bind(tls_observation.source())
        .bind(tls_observation.observed_at())
        .bind(tls_observation.unavailable_reason().map(|reason| reason.as_str()))
        .bind(tls_observation.validation().map(|validation| validation.as_str()))
        .bind(tls_observation.protocol())
        .bind(tls_observation.cipher_suite())
        .bind(tls_observation.certificate_subject())
        .bind(tls_observation.certificate_issuer())
        .bind(Json(tls_names))
        .bind(tls_observation.certificate_not_before())
        .bind(tls_observation.certificate_not_after())
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        if let Some(artifact) = artifact {
            let uncompressed_size = i64::try_from(artifact.uncompressed_size_bytes())
                .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;
            let compressed_size = i64::try_from(artifact.compressed_size_bytes())
                .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;
            sqlx::query(
                "INSERT INTO raw_artifacts (crawl_snapshot_id, kind, storage_location, checksum_sha256, \
                     compression, uncompressed_size_bytes, compressed_size_bytes, retention_expires_at, \
                     captured_at, created_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)",
            )
            .bind(snapshot_id)
            .bind(artifact.kind().as_str())
            .bind(artifact.storage_location())
            .bind(artifact.checksum_sha256())
            .bind(artifact.compression().as_str())
            .bind(uncompressed_size)
            .bind(compressed_size)
            .bind(artifact.retention_expires_at())
            .bind(snapshot.captured_at())
            .execute(&mut *transaction)
            .await
            .map_err(map_database_error)?;
        }
        let mut detection_ids = BTreeMap::new();
        for detection in detections {
            let detection_id: Uuid = sqlx::query_scalar(
                "INSERT INTO detections (crawl_snapshot_id, technology_id, detection_rule_version_id, \
                     confidence, method, observed_at, created_at) \
                 SELECT $1, technologies.id, rule_versions.id, $2, $3, $4, $4 \
                 FROM technologies \
                 JOIN technology_categories ON technology_categories.id = technologies.category_id \
                 JOIN detection_rules ON detection_rules.technology_id = technologies.id \
                 JOIN detection_rule_versions AS rule_versions \
                     ON rule_versions.detection_rule_id = detection_rules.id \
                 WHERE technologies.slug = $5 \
                   AND detection_rules.slug = $6 \
                   AND rule_versions.version = $7 \
                   AND technology_categories.slug = $8 \
                 RETURNING id",
            )
            .bind(snapshot_id)
            .bind(i16::from(detection.confidence().get()))
            .bind(detection.method().as_str())
            .bind(snapshot.captured_at())
            .bind(detection.technology_slug().as_str())
            .bind(detection.rule_slug().as_str())
            .bind(i16::try_from(detection.rule_version().get())
                .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?)
            .bind(detection.technology_category_slug().as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(map_database_error)?
            .ok_or(CrawlWorkerRepositoryError::Unavailable)?;
            detection_ids.insert(
                detection.technology_slug().as_str().to_owned(),
                detection_id,
            );
            for evidence in detection.evidence() {
                sqlx::query(
                    "INSERT INTO detection_evidence (detection_id, source, evidence_key, evidence_value, created_at) \
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(detection_id)
                .bind(evidence.source().as_str())
                .bind(evidence.key())
                .bind(evidence.value())
                .bind(snapshot.captured_at())
                .execute(&mut *transaction)
                .await
                .map_err(map_database_error)?;
            }
        }
        apply_projection(
            &mut transaction,
            domain_id,
            snapshot_id,
            snapshot.captured_at(),
            &observations,
            &detection_ids,
        )
        .await?;
        sqlx::query(
            "INSERT INTO search_index_outbox (domain_id, status, attempt_count, next_attempt_at, created_at, updated_at) \
             VALUES ($1, 'pending', 0, $2, $2, $2) \
             ON CONFLICT (domain_id) DO UPDATE SET \
                 status = 'pending', attempt_count = 0, next_attempt_at = EXCLUDED.next_attempt_at, \
                 claimed_at = NULL, last_error = NULL, updated_at = EXCLUDED.updated_at",
        )
        .bind(domain_id)
        .bind(snapshot.captured_at())
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        sqlx::query(
            "UPDATE crawl_policies \
             SET last_crawl_at = $1, last_successful_crawl_at = $1, next_crawl_at = $1 + desired_interval, \
                 consecutive_failure_count = 0, last_failure_at = NULL, \
                 terminal_failure_at = NULL, updated_at = $1 \
             WHERE domain_id = $2",
        )
        .bind(snapshot.captured_at())
        .bind(domain_id)
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;

        transaction.commit().await.map_err(map_database_error)
    }

    async fn record_failure(
        &self,
        job: &CrawlJobV1,
        failure: CrawlFailure,
        finished_at: OffsetDateTime,
        settings: SchedulerSettings,
    ) -> Result<(), CrawlWorkerRepositoryError> {
        PostgresCrawlScheduleRepository::new(self.pool.clone())
            .record_attempt_outcome(
                job.job_id(),
                CrawlAttemptOutcome::Failed(failure),
                finished_at,
                settings,
            )
            .await
            .map_err(|error| match error {
                techatlas_models::SchedulerRepositoryError::NotFound => {
                    CrawlWorkerRepositoryError::NotFound
                }
                techatlas_models::SchedulerRepositoryError::Unavailable => {
                    CrawlWorkerRepositoryError::Unavailable
                }
            })
    }
}

fn map_database_error(_: sqlx::Error) -> CrawlWorkerRepositoryError {
    CrawlWorkerRepositoryError::Unavailable
}

#[derive(Clone)]
struct PersistedCurrentTechnology {
    technology_id: Uuid,
    category_id: Uuid,
    detection_id: Uuid,
    snapshot_id: Uuid,
}

async fn apply_projection(
    transaction: &mut Transaction<'_, Postgres>,
    domain_id: Uuid,
    snapshot_id: Uuid,
    captured_at: OffsetDateTime,
    observations: &[RuleObservation],
    detection_ids: &BTreeMap<String, Uuid>,
) -> Result<(), CrawlWorkerRepositoryError> {
    for observation in observations {
        let result = sqlx::query(
            "INSERT INTO detection_rule_observations \
                 (crawl_snapshot_id, detection_rule_version_id, status, observed_at, created_at) \
             SELECT $1, rule_versions.id, $2, $3, $3 \
             FROM technologies \
             JOIN technology_categories ON technology_categories.id = technologies.category_id \
             JOIN detection_rules ON detection_rules.technology_id = technologies.id \
             JOIN detection_rule_versions AS rule_versions \
                 ON rule_versions.detection_rule_id = detection_rules.id \
             WHERE technologies.slug = $4 AND technology_categories.slug = $5 \
               AND detection_rules.slug = $6 AND rule_versions.version = $7",
        )
        .bind(snapshot_id)
        .bind(observation.status().as_str())
        .bind(captured_at)
        .bind(observation.technology_slug().as_str())
        .bind(observation.technology_category_slug().as_str())
        .bind(observation.rule_slug().as_str())
        .bind(
            i16::try_from(observation.rule_version().get())
                .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?,
        )
        .execute(&mut **transaction)
        .await
        .map_err(map_database_error)?;
        if result.rows_affected() != 1 {
            return Err(CrawlWorkerRepositoryError::Unavailable);
        }
    }

    let rows = sqlx::query(
        "SELECT current.technology_id, categories.id AS category_id, technologies.slug AS technology_slug, \
             categories.slug AS category_slug, current.current_detection_id, current.last_snapshot_id \
         FROM domain_current_technologies AS current \
         JOIN technologies ON technologies.id = current.technology_id \
         JOIN technology_categories AS categories ON categories.id = technologies.category_id \
         WHERE current.domain_id = $1 \
         FOR UPDATE OF current",
    )
    .bind(domain_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(map_database_error)?;
    let mut previous = Vec::new();
    let mut previous_by_slug = BTreeMap::new();
    for row in rows {
        let technology_slug: String = row
            .try_get("technology_slug")
            .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;
        let category_slug: String = row
            .try_get("category_slug")
            .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?;
        let state = CurrentTechnology::new(
            techatlas_models::TechnologySlug::parse(&technology_slug)
                .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?,
            techatlas_models::TechnologyCategorySlug::parse(&category_slug)
                .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?,
        );
        let persisted = PersistedCurrentTechnology {
            technology_id: row
                .try_get("technology_id")
                .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?,
            category_id: row
                .try_get("category_id")
                .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?,
            detection_id: row
                .try_get("current_detection_id")
                .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?,
            snapshot_id: row
                .try_get("last_snapshot_id")
                .map_err(|_| CrawlWorkerRepositoryError::Unavailable)?,
        };
        previous_by_slug.insert(technology_slug, persisted);
        previous.push(state);
    }
    let transition = derive_projection(&previous, observations);

    for state in transition.deactivate() {
        sqlx::query(
            "DELETE FROM domain_current_technologies \
             USING technologies \
             WHERE domain_current_technologies.domain_id = $1 \
               AND domain_current_technologies.technology_id = technologies.id \
               AND technologies.slug = $2",
        )
        .bind(domain_id)
        .bind(state.technology_slug().as_str())
        .execute(&mut **transaction)
        .await
        .map_err(map_database_error)?;
    }
    for state in transition.activate() {
        let detection_id = detection_ids
            .get(state.technology_slug().as_str())
            .copied()
            .ok_or(CrawlWorkerRepositoryError::Unavailable)?;
        sqlx::query(
            "INSERT INTO domain_current_technologies \
                 (domain_id, technology_id, current_detection_id, first_snapshot_id, last_snapshot_id, \
                  first_observed_at, last_observed_at, updated_at) \
             SELECT $1, technologies.id, $2, $3, $3, $4, $4, $4 \
             FROM technologies \
             JOIN technology_categories ON technology_categories.id = technologies.category_id \
             WHERE technologies.slug = $5 AND technology_categories.slug = $6 \
             ON CONFLICT (domain_id, technology_id) DO UPDATE SET \
                 current_detection_id = EXCLUDED.current_detection_id, \
                 last_snapshot_id = EXCLUDED.last_snapshot_id, \
                 last_observed_at = EXCLUDED.last_observed_at, \
                 updated_at = EXCLUDED.updated_at",
        )
        .bind(domain_id)
        .bind(detection_id)
        .bind(snapshot_id)
        .bind(captured_at)
        .bind(state.technology_slug().as_str())
        .bind(state.technology_category_slug().as_str())
        .execute(&mut **transaction)
        .await
        .map_err(map_database_error)?;
    }
    for change in transition.changes() {
        let from = change
            .from()
            .and_then(|state| previous_by_slug.get(state.technology_slug().as_str()));
        let to = change.to();
        let category_id = match (from, to) {
            (Some(previous), _) => previous.category_id,
            (None, Some(current)) => {
                category_id_for(transaction, current.technology_category_slug().as_str()).await?
            }
            (None, None) => return Err(CrawlWorkerRepositoryError::Unavailable),
        };
        let current_detection_id =
            to.and_then(|state| detection_ids.get(state.technology_slug().as_str()).copied());
        let from_technology_id = from.map(|previous| previous.technology_id);
        let to_technology_id = if let Some(state) = to {
            technology_id_for(transaction, state.technology_slug().as_str()).await?
        } else {
            None
        };
        sqlx::query(
            "INSERT INTO technology_changes \
                 (domain_id, change_kind, category_id, prior_snapshot_id, current_snapshot_id, \
                  prior_detection_id, current_detection_id, from_technology_id, to_technology_id, observed_at, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)",
        )
        .bind(domain_id)
        .bind(change.kind().as_str())
        .bind(category_id)
        .bind(from.map(|previous| previous.snapshot_id))
        .bind(snapshot_id)
        .bind(from.map(|previous| previous.detection_id))
        .bind(current_detection_id)
        .bind(from_technology_id)
        .bind(to_technology_id)
        .bind(captured_at)
        .execute(&mut **transaction)
        .await
        .map_err(map_database_error)?;
    }
    Ok(())
}

async fn category_id_for(
    transaction: &mut Transaction<'_, Postgres>,
    category_slug: &str,
) -> Result<Uuid, CrawlWorkerRepositoryError> {
    sqlx::query_scalar("SELECT id FROM technology_categories WHERE slug = $1")
        .bind(category_slug)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(map_database_error)?
        .ok_or(CrawlWorkerRepositoryError::Unavailable)
}

async fn technology_id_for(
    transaction: &mut Transaction<'_, Postgres>,
    technology_slug: &str,
) -> Result<Option<Uuid>, CrawlWorkerRepositoryError> {
    sqlx::query_scalar("SELECT id FROM technologies WHERE slug = $1")
        .bind(technology_slug)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(map_database_error)
}
