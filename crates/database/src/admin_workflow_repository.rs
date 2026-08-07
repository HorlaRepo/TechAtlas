use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgPool, Row, types::Json};
use techatlas_models::{
    AdminCrawlAttempt, AdminCrawlRetryState, AdminDetectionRule, AdminDetectionRuleOperations,
    AdminDetectionRuleVersion, AdminImportBatchRecrawl, AdminOperationError,
    AdminReprocessingOperations, AdminReprocessingRun, AdminSchedulerOperations,
};
use time::OffsetDateTime;
use uuid::Uuid;

pub struct PostgresAdminWorkflowRepository {
    pool: PgPool,
}

impl PostgresAdminWorkflowRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn rule_by_slug(
        &self,
        rule_slug: &str,
    ) -> Result<AdminDetectionRule, AdminOperationError> {
        let row = sqlx::query(
            "SELECT rules.id, rules.slug AS rule_slug, technologies.slug AS technology_slug, \
                    technologies.display_name AS technology_name, rules.active_version_id \
             FROM detection_rules AS rules \
             JOIN technologies ON technologies.id = rules.technology_id \
             WHERE rules.slug = $1",
        )
        .bind(rule_slug)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_database_error)?
        .ok_or(AdminOperationError::NotFound)?;
        let rule_id: Uuid = row.try_get("id").map_err(map_row_error)?;
        let active_version_id: Option<Uuid> =
            row.try_get("active_version_id").map_err(map_row_error)?;
        let draft = sqlx::query(
            "SELECT definition, updated_at FROM detection_rule_drafts WHERE detection_rule_id = $1",
        )
        .bind(rule_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_database_error)?;
        let versions = sqlx::query(
            "SELECT id, version, definition, published_at \
             FROM detection_rule_versions WHERE detection_rule_id = $1 ORDER BY version DESC",
        )
        .bind(rule_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_database_error)?
        .into_iter()
        .map(|version| {
            Ok(AdminDetectionRuleVersion {
                version: u16::try_from(
                    version
                        .try_get::<i16, _>("version")
                        .map_err(map_row_error)?,
                )
                .map_err(|_| AdminOperationError::Unavailable)?,
                definition: version
                    .try_get::<Json<Value>, _>("definition")
                    .map_err(map_row_error)?
                    .0,
                published_at: version.try_get("published_at").map_err(map_row_error)?,
                is_active: Some(version.try_get::<Uuid, _>("id").map_err(map_row_error)?)
                    == active_version_id,
            })
        })
        .collect::<Result<Vec<_>, AdminOperationError>>()?;
        Ok(AdminDetectionRule {
            rule_slug: row.try_get("rule_slug").map_err(map_row_error)?,
            technology_slug: row.try_get("technology_slug").map_err(map_row_error)?,
            technology_name: row.try_get("technology_name").map_err(map_row_error)?,
            active_version: versions
                .iter()
                .find(|version| version.is_active)
                .map(|version| version.version),
            draft_definition: draft
                .as_ref()
                .map(|draft| {
                    draft
                        .try_get::<Json<Value>, _>("definition")
                        .map(|definition| definition.0)
                })
                .transpose()
                .map_err(map_row_error)?,
            draft_updated_at: draft
                .as_ref()
                .map(|draft| draft.try_get("updated_at"))
                .transpose()
                .map_err(map_row_error)?,
            versions,
        })
    }
}

#[async_trait]
impl AdminSchedulerOperations for PostgresAdminWorkflowRepository {
    async fn request_domain_crawl(
        &self,
        domain: &techatlas_models::CanonicalDomain,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<(), AdminOperationError> {
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let policy = sqlx::query(
            "SELECT policies.domain_id, policies.is_enabled \
             FROM crawl_policies AS policies \
             JOIN domains ON domains.id = policies.domain_id \
             WHERE domains.canonical_domain = $1 AND domains.archived_at IS NULL \
             FOR UPDATE OF policies",
        )
        .bind(domain.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database_error)?
        .ok_or(AdminOperationError::NotFound)?;
        let domain_id: Uuid = policy.try_get("domain_id").map_err(map_row_error)?;
        let is_enabled: bool = policy.try_get("is_enabled").map_err(map_row_error)?;
        if !is_enabled {
            return Err(AdminOperationError::Conflict);
        }
        sqlx::query(
            "UPDATE crawl_policies SET next_crawl_at = $1, updated_at = $1 WHERE domain_id = $2",
        )
        .bind(now)
        .bind(domain_id)
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        sqlx::query(
            "INSERT INTO admin_audit_events (actor_subject, action, resource_kind, resource_id, metadata) \
             VALUES ($1, 'domain.crawl_requested', 'domain', $2, jsonb_build_object('scheduled_at', $3))",
        )
        .bind(actor_subject)
        .bind(domain_id)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        transaction.commit().await.map_err(map_database_error)
    }

    async fn crawl_attempts(
        &self,
        limit: usize,
    ) -> Result<Vec<AdminCrawlAttempt>, AdminOperationError> {
        let rows = sqlx::query(
            "SELECT attempts.job_id, domains.canonical_domain, attempts.attempt_number, attempts.status, \
                    attempts.queued_at, attempts.finished_at, attempts.failure_code, attempts.failure_summary, \
                    policies.terminal_failure_at, \
                    EXISTS (SELECT 1 FROM crawl_attempts AS later \
                        WHERE later.domain_id = attempts.domain_id AND later.queued_at > attempts.queued_at) \
                     AS has_later_attempt, \
                    (attempts.status = 'failed' AND policies.terminal_failure_at IS NOT NULL \
                     AND attempts.failure_code NOT IN ('unsafe_target', 'robots_denied', 'cancelled') \
                     AND NOT EXISTS (SELECT 1 FROM crawl_attempts AS later \
                         WHERE later.domain_id = attempts.domain_id AND later.queued_at > attempts.queued_at)) \
                     AS retry_eligible \
             FROM crawl_attempts AS attempts \
             JOIN domains ON domains.id = attempts.domain_id \
             JOIN crawl_policies AS policies ON policies.domain_id = attempts.domain_id \
             ORDER BY attempts.queued_at DESC LIMIT $1",
        )
        .bind(i64::try_from(limit).map_err(|_| AdminOperationError::Validation)?)
        .fetch_all(&self.pool)
        .await
        .map_err(map_database_error)?;
        rows.into_iter().map(row_to_attempt).collect()
    }

    async fn retry_terminal_attempt(
        &self,
        job_id: &str,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<AdminCrawlAttempt, AdminOperationError> {
        let job_id = Uuid::parse_str(job_id).map_err(|_| AdminOperationError::Validation)?;
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let row = sqlx::query(
            "SELECT attempts.id, attempts.domain_id, attempts.correlation_id, attempts.attempt_number, \
                    attempts.status, attempts.failure_code, domains.canonical_domain, policies.terminal_failure_at \
             FROM crawl_attempts AS attempts \
             JOIN domains ON domains.id = attempts.domain_id \
             JOIN crawl_policies AS policies ON policies.domain_id = attempts.domain_id \
             WHERE attempts.job_id = $1 \
             FOR UPDATE OF attempts, policies",
        )
        .bind(job_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database_error)?
        .ok_or(AdminOperationError::NotFound)?;
        let status: String = row.try_get("status").map_err(map_row_error)?;
        let failure_code: Option<String> = row.try_get("failure_code").map_err(map_row_error)?;
        let terminal_failure_at: Option<OffsetDateTime> =
            row.try_get("terminal_failure_at").map_err(map_row_error)?;
        if status != "failed"
            || terminal_failure_at.is_none()
            || matches!(
                failure_code.as_deref(),
                Some("unsafe_target" | "robots_denied" | "cancelled")
            )
        {
            return Err(AdminOperationError::Conflict);
        }
        let domain_id: Uuid = row.try_get("domain_id").map_err(map_row_error)?;
        let correlation_id: Uuid = row.try_get("correlation_id").map_err(map_row_error)?;
        let attempt_id: Uuid = row.try_get("id").map_err(map_row_error)?;
        let attempt_number: i16 = row.try_get("attempt_number").map_err(map_row_error)?;
        let next_attempt_number = attempt_number
            .checked_add(1)
            .ok_or(AdminOperationError::Unavailable)?;
        let next_job_id = Uuid::new_v4();
        let idempotency_key = format!("crawl:{next_job_id}");
        let new_attempt_id: Uuid = sqlx::query_scalar(
            "INSERT INTO crawl_attempts (domain_id, job_id, correlation_id, idempotency_key, attempt_number, queued_at, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $6) RETURNING id",
        )
        .bind(domain_id)
        .bind(next_job_id)
        .bind(correlation_id)
        .bind(&idempotency_key)
        .bind(next_attempt_number)
        .bind(now)
        .fetch_one(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        sqlx::query(
            "INSERT INTO crawl_job_outbox (crawl_attempt_id, domain_id, job_id, correlation_id, idempotency_key, available_at, next_publish_at, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $6, $6, $6)",
        )
        .bind(new_attempt_id)
        .bind(domain_id)
        .bind(next_job_id)
        .bind(correlation_id)
        .bind(&idempotency_key)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        sqlx::query(
            "UPDATE crawl_policies SET terminal_failure_at = NULL, next_crawl_at = $1, updated_at = $1 WHERE domain_id = $2",
        )
        .bind(now)
        .bind(domain_id)
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        sqlx::query(
            "INSERT INTO admin_audit_events (actor_subject, action, resource_kind, resource_id, metadata) \
             VALUES ($1, 'crawl_attempt.retry_requested', 'crawl_attempt', $2, jsonb_build_object('previous_job_id', $3, 'next_job_id', $4))",
        )
        .bind(actor_subject)
        .bind(attempt_id)
        .bind(job_id)
        .bind(next_job_id)
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        transaction.commit().await.map_err(map_database_error)?;
        Ok(AdminCrawlAttempt {
            job_id: next_job_id.to_string(),
            canonical_domain: row.try_get("canonical_domain").map_err(map_row_error)?,
            attempt_number: u16::try_from(next_attempt_number)
                .map_err(|_| AdminOperationError::Unavailable)?,
            status: "queued".to_owned(),
            queued_at: now,
            finished_at: None,
            failure_code: None,
            failure_summary: None,
            retry_state: AdminCrawlRetryState::InProgress,
            retry_eligible: false,
        })
    }

    async fn schedule_country_enrichment_recrawl(
        &self,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<u64, AdminOperationError> {
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let scheduled_domain_ids = sqlx::query_scalar::<_, Uuid>(
            "UPDATE crawl_policies AS policies \
             SET next_crawl_at = $1, updated_at = $1 \
             WHERE policies.is_enabled \
               AND EXISTS (SELECT 1 FROM crawl_snapshots AS snapshots \
                           WHERE snapshots.domain_id = policies.domain_id) \
               AND NOT EXISTS (SELECT 1 FROM crawl_country_observations AS observations \
                               JOIN crawl_snapshots AS snapshots \
                                 ON snapshots.id = observations.crawl_snapshot_id \
                               WHERE snapshots.domain_id = policies.domain_id) \
               AND NOT EXISTS (SELECT 1 FROM crawl_attempts AS attempts \
                               WHERE attempts.domain_id = policies.domain_id \
                                 AND attempts.status IN ('queued', 'running')) \
             RETURNING policies.domain_id",
        )
        .bind(now)
        .fetch_all(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        let scheduled_count = u64::try_from(scheduled_domain_ids.len())
            .map_err(|_| AdminOperationError::Unavailable)?;
        sqlx::query(
            "INSERT INTO admin_audit_events (actor_subject, action, resource_kind, resource_id, metadata) \
             VALUES ($1, 'country_enrichment.recrawl_requested', 'country_enrichment', $2, \
                     jsonb_build_object('scheduled_domain_count', $3))",
        )
        .bind(actor_subject)
        .bind(Uuid::nil())
        .bind(i64::try_from(scheduled_count).map_err(|_| AdminOperationError::Unavailable)?)
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        transaction.commit().await.map_err(map_database_error)?;
        Ok(scheduled_count)
    }

    async fn schedule_import_batch_recrawl(
        &self,
        import_id: &str,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<AdminImportBatchRecrawl, AdminOperationError> {
        let import_id = Uuid::parse_str(import_id).map_err(|_| AdminOperationError::Validation)?;
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let batch = sqlx::query(
            "SELECT imports.id, sources.name AS source_name \
             FROM imports \
             JOIN domain_sources AS sources ON sources.id = imports.source_id \
             WHERE imports.id = $1 AND imports.status = 'completed' \
             FOR UPDATE OF imports",
        )
        .bind(import_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database_error)?
        .ok_or(AdminOperationError::NotFound)?;
        let source_name: String = batch.try_get("source_name").map_err(map_row_error)?;
        let requested_domain_count = u64::try_from(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(DISTINCT domain_id) FROM import_rows \
                 WHERE import_id = $1 AND domain_id IS NOT NULL",
            )
            .bind(import_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(map_database_error)?,
        )
        .map_err(|_| AdminOperationError::Unavailable)?;
        let scheduled_domain_ids = sqlx::query_scalar::<_, Uuid>(
            "UPDATE crawl_policies AS policies \
             SET next_crawl_at = $1, updated_at = $1 \
             WHERE policies.is_enabled \
               AND EXISTS (SELECT 1 \
                           FROM import_rows AS rows \
                           JOIN domains ON domains.id = rows.domain_id \
                           WHERE rows.import_id = $2 \
                             AND rows.domain_id = policies.domain_id \
                             AND domains.archived_at IS NULL) \
               AND NOT EXISTS (SELECT 1 FROM crawl_attempts AS attempts \
                               WHERE attempts.domain_id = policies.domain_id \
                                 AND attempts.status IN ('queued', 'running')) \
             RETURNING policies.domain_id",
        )
        .bind(now)
        .bind(import_id)
        .fetch_all(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        let scheduled_domain_count = u64::try_from(scheduled_domain_ids.len())
            .map_err(|_| AdminOperationError::Unavailable)?;
        let skipped_domain_count = requested_domain_count
            .checked_sub(scheduled_domain_count)
            .ok_or(AdminOperationError::Unavailable)?;
        sqlx::query(
            "INSERT INTO admin_audit_events (actor_subject, action, resource_kind, resource_id, metadata) \
             VALUES ($1, 'import.recrawl_requested', 'import', $2, jsonb_build_object( \
                 'requested_domain_count', $3, 'scheduled_domain_count', $4, 'skipped_domain_count', $5))",
        )
        .bind(actor_subject)
        .bind(import_id)
        .bind(i64::try_from(requested_domain_count).map_err(|_| AdminOperationError::Unavailable)?)
        .bind(i64::try_from(scheduled_domain_count).map_err(|_| AdminOperationError::Unavailable)?)
        .bind(i64::try_from(skipped_domain_count).map_err(|_| AdminOperationError::Unavailable)?)
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        transaction.commit().await.map_err(map_database_error)?;
        Ok(AdminImportBatchRecrawl {
            import_id: import_id.to_string(),
            source_name,
            requested_domain_count,
            scheduled_domain_count,
            skipped_domain_count,
        })
    }
}

#[async_trait]
impl AdminDetectionRuleOperations for PostgresAdminWorkflowRepository {
    async fn rules(&self) -> Result<Vec<AdminDetectionRule>, AdminOperationError> {
        let slugs =
            sqlx::query_scalar::<_, String>("SELECT slug FROM detection_rules ORDER BY slug")
                .fetch_all(&self.pool)
                .await
                .map_err(map_database_error)?;
        let mut rules = Vec::with_capacity(slugs.len());
        for slug in slugs {
            rules.push(self.rule_by_slug(&slug).await?);
        }
        Ok(rules)
    }

    async fn save_draft(
        &self,
        rule_slug: &str,
        definition: Value,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<AdminDetectionRule, AdminOperationError> {
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let rule_id: Uuid =
            sqlx::query_scalar("SELECT id FROM detection_rules WHERE slug = $1 FOR UPDATE")
                .bind(rule_slug)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(map_database_error)?
                .ok_or(AdminOperationError::NotFound)?;
        sqlx::query(
            "INSERT INTO detection_rule_drafts (detection_rule_id, definition, updated_by, updated_at) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (detection_rule_id) DO UPDATE SET definition = EXCLUDED.definition, updated_by = EXCLUDED.updated_by, updated_at = EXCLUDED.updated_at, tested_at = NULL",
        )
        .bind(rule_id).bind(Json(definition)).bind(actor_subject).bind(now).execute(&mut *transaction).await.map_err(map_database_error)?;
        sqlx::query(
            "INSERT INTO admin_audit_events (actor_subject, action, resource_kind, resource_id) VALUES ($1, 'detection_rule.draft_saved', 'detection_rule', $2)",
        ).bind(actor_subject).bind(rule_id).execute(&mut *transaction).await.map_err(map_database_error)?;
        transaction.commit().await.map_err(map_database_error)?;
        self.rule_by_slug(rule_slug).await
    }

    async fn publish_draft(
        &self,
        rule_slug: &str,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<AdminDetectionRuleVersion, AdminOperationError> {
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let row = sqlx::query(
            "SELECT rules.id, drafts.definition FROM detection_rules AS rules \
             JOIN detection_rule_drafts AS drafts ON drafts.detection_rule_id = rules.id \
             WHERE rules.slug = $1 AND drafts.tested_at IS NOT NULL FOR UPDATE OF rules, drafts",
        )
        .bind(rule_slug)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database_error)?
        .ok_or(AdminOperationError::NotFound)?;
        let rule_id: Uuid = row.try_get("id").map_err(map_row_error)?;
        let definition: Json<Value> = row.try_get("definition").map_err(map_row_error)?;
        let version: i16 = sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) + 1 FROM detection_rule_versions WHERE detection_rule_id = $1")
            .bind(rule_id).fetch_one(&mut *transaction).await.map_err(map_database_error)?;
        let _version_id: Uuid = sqlx::query_scalar(
            "INSERT INTO detection_rule_versions (detection_rule_id, version, definition, published_at, created_at) \
             VALUES ($1, $2, $3, $4, $4) RETURNING id",
        ).bind(rule_id).bind(version).bind(definition).bind(now).fetch_one(&mut *transaction).await.map_err(map_database_error)?;
        sqlx::query("DELETE FROM detection_rule_drafts WHERE detection_rule_id = $1")
            .bind(rule_id)
            .execute(&mut *transaction)
            .await
            .map_err(map_database_error)?;
        sqlx::query(
            "INSERT INTO admin_audit_events (actor_subject, action, resource_kind, resource_id, metadata) \
             VALUES ($1, 'detection_rule.published', 'detection_rule', $2, jsonb_build_object('version', $3))",
        ).bind(actor_subject).bind(rule_id).bind(version).execute(&mut *transaction).await.map_err(map_database_error)?;
        transaction.commit().await.map_err(map_database_error)?;
        let version = u16::try_from(version).map_err(|_| AdminOperationError::Unavailable)?;
        let definition = self
            .rule_by_slug(rule_slug)
            .await?
            .versions
            .into_iter()
            .find(|candidate| candidate.version == version)
            .map(|candidate| candidate.definition)
            .ok_or(AdminOperationError::Unavailable)?;
        Ok(AdminDetectionRuleVersion {
            version,
            definition,
            published_at: now,
            is_active: false,
        })
    }

    async fn mark_draft_tested(
        &self,
        rule_slug: &str,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<(), AdminOperationError> {
        let updated = sqlx::query(
            "UPDATE detection_rule_drafts AS drafts SET tested_at = $1 \
             FROM detection_rules AS rules \
             WHERE drafts.detection_rule_id = rules.id AND rules.slug = $2",
        )
        .bind(now)
        .bind(rule_slug)
        .execute(&self.pool)
        .await
        .map_err(map_database_error)?
        .rows_affected();
        if updated == 0 {
            return Err(AdminOperationError::NotFound);
        }
        let rule_id: Uuid = sqlx::query_scalar("SELECT id FROM detection_rules WHERE slug = $1")
            .bind(rule_slug)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_database_error)?
            .ok_or(AdminOperationError::NotFound)?;
        sqlx::query(
            "INSERT INTO admin_audit_events (actor_subject, action, resource_kind, resource_id) VALUES ($1, 'detection_rule.draft_tested', 'detection_rule', $2)",
        )
        .bind(actor_subject)
        .bind(rule_id)
        .execute(&self.pool)
        .await
        .map_err(map_database_error)?;
        Ok(())
    }

    async fn set_active_version(
        &self,
        rule_slug: &str,
        version: Option<u16>,
        actor_subject: &str,
        _now: OffsetDateTime,
    ) -> Result<(), AdminOperationError> {
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let rule_id: Uuid =
            sqlx::query_scalar("SELECT id FROM detection_rules WHERE slug = $1 FOR UPDATE")
                .bind(rule_slug)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(map_database_error)?
                .ok_or(AdminOperationError::NotFound)?;
        let version_id = if let Some(version) = version {
            sqlx::query_scalar::<_, Uuid>("SELECT id FROM detection_rule_versions WHERE detection_rule_id = $1 AND version = $2")
                .bind(rule_id).bind(i16::try_from(version).map_err(|_| AdminOperationError::Validation)?)
                .fetch_optional(&mut *transaction).await.map_err(map_database_error)?
                .ok_or(AdminOperationError::NotFound)?
        } else {
            Uuid::nil()
        };
        sqlx::query("UPDATE detection_rules SET active_version_id = $1 WHERE id = $2")
            .bind(version.map(|_| version_id))
            .bind(rule_id)
            .execute(&mut *transaction)
            .await
            .map_err(map_database_error)?;
        sqlx::query(
            "INSERT INTO admin_audit_events (actor_subject, action, resource_kind, resource_id, metadata) \
             VALUES ($1, $2, 'detection_rule', $3, jsonb_build_object('version', $4))",
        ).bind(actor_subject).bind(if version.is_some() { "detection_rule.activated" } else { "detection_rule.deactivated" }).bind(rule_id).bind(version.map(i32::from)).execute(&mut *transaction).await.map_err(map_database_error)?;
        transaction.commit().await.map_err(map_database_error)
    }
}

#[async_trait]
impl AdminReprocessingOperations for PostgresAdminWorkflowRepository {
    async fn start_reprocessing(
        &self,
        rule_slug: &str,
        rule_version: u16,
        idempotency_key: &str,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<AdminReprocessingRun, AdminOperationError> {
        let idempotency_key = idempotency_key.trim();
        if idempotency_key.is_empty()
            || idempotency_key.len() > 255
            || actor_subject.trim().is_empty()
        {
            return Err(AdminOperationError::Validation);
        }
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let version = sqlx::query(
            "SELECT versions.id FROM detection_rule_versions AS versions \
             JOIN detection_rules AS rules ON rules.id = versions.detection_rule_id \
             WHERE rules.slug = $1 AND versions.version = $2",
        )
        .bind(rule_slug)
        .bind(i16::try_from(rule_version).map_err(|_| AdminOperationError::Validation)?)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database_error)?
        .ok_or(AdminOperationError::NotFound)?;
        let version_id: Uuid = version.try_get("id").map_err(map_row_error)?;
        let run_id = Uuid::new_v4();
        let inserted: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO detection_reprocessing_runs \
             (id, detection_rule_version_id, requested_by, idempotency_key, correlation_id, requested_at) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (idempotency_key) DO NOTHING RETURNING id",
        )
        .bind(run_id)
        .bind(version_id)
        .bind(actor_subject)
        .bind(idempotency_key)
        .bind(Uuid::new_v4())
        .bind(now)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        let run_id = match inserted {
            Some(run_id) => {
                let total: i64 = sqlx::query_scalar(
                    "WITH inserted AS ( \
                       INSERT INTO detection_reprocessing_work_items \
                         (run_id, crawl_snapshot_id, domain_id, next_attempt_at, created_at, updated_at) \
                       SELECT $1, snapshots.id, snapshots.domain_id, $2, $2, $2 FROM crawl_snapshots AS snapshots \
                       ON CONFLICT (run_id, crawl_snapshot_id) DO NOTHING RETURNING 1 \
                     ) SELECT COUNT(*) FROM inserted",
                )
                .bind(run_id)
                .bind(now)
                .fetch_one(&mut *transaction)
                .await
                .map_err(map_database_error)?;
                sqlx::query(
                    "UPDATE detection_reprocessing_runs SET total_snapshot_count = $1, \
                     status = CASE WHEN $1 = 0 THEN 'completed' ELSE status END, \
                     finished_at = CASE WHEN $1 = 0 THEN $2 ELSE NULL END WHERE id = $3",
                )
                .bind(i32::try_from(total).map_err(|_| AdminOperationError::Unavailable)?)
                .bind(now)
                .bind(run_id)
                .execute(&mut *transaction)
                .await
                .map_err(map_database_error)?;
                sqlx::query(
                    "INSERT INTO admin_audit_events (actor_subject, action, resource_kind, resource_id, metadata) \
                     VALUES ($1, 'detection_reprocessing.requested', 'detection_reprocessing_run', $2, \
                       jsonb_build_object('rule_slug', $3, 'rule_version', $4, 'snapshot_count', $5))",
                )
                .bind(actor_subject)
                .bind(run_id)
                .bind(rule_slug)
                .bind(i32::from(rule_version))
                .bind(total)
                .execute(&mut *transaction)
                .await
                .map_err(map_database_error)?;
                run_id
            }
            None => {
                let existing = sqlx::query(
                    "SELECT id, detection_rule_version_id FROM detection_reprocessing_runs \
                     WHERE idempotency_key = $1 FOR UPDATE",
                )
                .bind(idempotency_key)
                .fetch_one(&mut *transaction)
                .await
                .map_err(map_database_error)?;
                if existing
                    .try_get::<Uuid, _>("detection_rule_version_id")
                    .map_err(map_row_error)?
                    != version_id
                {
                    return Err(AdminOperationError::Conflict);
                }
                existing.try_get("id").map_err(map_row_error)?
            }
        };
        let run = reprocessing_run_by_id(&mut transaction, run_id).await?;
        transaction.commit().await.map_err(map_database_error)?;
        Ok(run)
    }

    async fn reprocessing_runs(
        &self,
        rule_slug: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AdminReprocessingRun>, AdminOperationError> {
        let rows = sqlx::query(
            "SELECT runs.id, rules.slug AS rule_slug, versions.version, runs.status, runs.total_snapshot_count, \
                    runs.succeeded_snapshot_count, runs.failed_snapshot_count, runs.requested_at, runs.started_at, runs.finished_at \
             FROM detection_reprocessing_runs AS runs \
             JOIN detection_rule_versions AS versions ON versions.id = runs.detection_rule_version_id \
             JOIN detection_rules AS rules ON rules.id = versions.detection_rule_id \
             WHERE ($1::TEXT IS NULL OR rules.slug = $1) \
             ORDER BY runs.requested_at DESC LIMIT $2",
        )
        .bind(rule_slug)
        .bind(i64::try_from(limit).map_err(|_| AdminOperationError::Validation)?)
        .fetch_all(&self.pool)
        .await
        .map_err(map_database_error)?;
        rows.into_iter().map(reprocessing_run_from_row).collect()
    }
}

async fn reprocessing_run_by_id(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run_id: Uuid,
) -> Result<AdminReprocessingRun, AdminOperationError> {
    let row = sqlx::query(
        "SELECT runs.id, rules.slug AS rule_slug, versions.version, runs.status, runs.total_snapshot_count, \
                runs.succeeded_snapshot_count, runs.failed_snapshot_count, runs.requested_at, runs.started_at, runs.finished_at \
         FROM detection_reprocessing_runs AS runs \
         JOIN detection_rule_versions AS versions ON versions.id = runs.detection_rule_version_id \
         JOIN detection_rules AS rules ON rules.id = versions.detection_rule_id WHERE runs.id = $1",
    )
    .bind(run_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(map_database_error)?;
    reprocessing_run_from_row(row)
}

fn reprocessing_run_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<AdminReprocessingRun, AdminOperationError> {
    Ok(AdminReprocessingRun {
        id: row
            .try_get::<Uuid, _>("id")
            .map_err(map_row_error)?
            .to_string(),
        rule_slug: row.try_get("rule_slug").map_err(map_row_error)?,
        rule_version: u16::try_from(row.try_get::<i16, _>("version").map_err(map_row_error)?)
            .map_err(|_| AdminOperationError::Unavailable)?,
        status: row.try_get("status").map_err(map_row_error)?,
        total_snapshot_count: u32::try_from(
            row.try_get::<i32, _>("total_snapshot_count")
                .map_err(map_row_error)?,
        )
        .map_err(|_| AdminOperationError::Unavailable)?,
        succeeded_snapshot_count: u32::try_from(
            row.try_get::<i32, _>("succeeded_snapshot_count")
                .map_err(map_row_error)?,
        )
        .map_err(|_| AdminOperationError::Unavailable)?,
        failed_snapshot_count: u32::try_from(
            row.try_get::<i32, _>("failed_snapshot_count")
                .map_err(map_row_error)?,
        )
        .map_err(|_| AdminOperationError::Unavailable)?,
        requested_at: row.try_get("requested_at").map_err(map_row_error)?,
        started_at: row.try_get("started_at").map_err(map_row_error)?,
        finished_at: row.try_get("finished_at").map_err(map_row_error)?,
    })
}

fn row_to_attempt(row: sqlx::postgres::PgRow) -> Result<AdminCrawlAttempt, AdminOperationError> {
    let status: String = row.try_get("status").map_err(map_row_error)?;
    let failure_code: Option<String> = row.try_get("failure_code").map_err(map_row_error)?;
    let terminal_failure_at: Option<OffsetDateTime> =
        row.try_get("terminal_failure_at").map_err(map_row_error)?;
    let has_later_attempt: bool = row.try_get("has_later_attempt").map_err(map_row_error)?;
    let retry_state = retry_state(
        &status,
        failure_code.as_deref(),
        terminal_failure_at.is_some(),
        has_later_attempt,
    );
    Ok(AdminCrawlAttempt {
        job_id: row
            .try_get::<Uuid, _>("job_id")
            .map_err(map_row_error)?
            .to_string(),
        canonical_domain: row.try_get("canonical_domain").map_err(map_row_error)?,
        attempt_number: u16::try_from(
            row.try_get::<i16, _>("attempt_number")
                .map_err(map_row_error)?,
        )
        .map_err(|_| AdminOperationError::Unavailable)?,
        status,
        queued_at: row.try_get("queued_at").map_err(map_row_error)?,
        finished_at: row.try_get("finished_at").map_err(map_row_error)?,
        failure_code,
        failure_summary: row.try_get("failure_summary").map_err(map_row_error)?,
        retry_state,
        retry_eligible: matches!(retry_state, AdminCrawlRetryState::ManualRetryAvailable),
    })
}

fn retry_state(
    status: &str,
    failure_code: Option<&str>,
    terminal_failure: bool,
    has_later_attempt: bool,
) -> AdminCrawlRetryState {
    match status {
        "queued" | "running" => AdminCrawlRetryState::InProgress,
        "failed" if has_later_attempt || !terminal_failure => {
            AdminCrawlRetryState::AutomaticRetryScheduled
        }
        "failed"
            if matches!(
                failure_code,
                Some("unsafe_target" | "robots_denied" | "cancelled")
            ) =>
        {
            AdminCrawlRetryState::ManualRetryBlocked
        }
        "failed" => AdminCrawlRetryState::ManualRetryAvailable,
        "cancelled" => AdminCrawlRetryState::ManualRetryBlocked,
        _ => AdminCrawlRetryState::NotApplicable,
    }
}

fn map_database_error(_: sqlx::Error) -> AdminOperationError {
    AdminOperationError::Unavailable
}
fn map_row_error(_: sqlx::Error) -> AdminOperationError {
    AdminOperationError::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_retry_states_without_hiding_automatic_work() {
        assert_eq!(
            retry_state("queued", None, false, false),
            AdminCrawlRetryState::InProgress
        );
        assert_eq!(
            retry_state("failed", Some("network_timeout"), false, true),
            AdminCrawlRetryState::AutomaticRetryScheduled
        );
        assert_eq!(
            retry_state("failed", Some("robots_denied"), true, false),
            AdminCrawlRetryState::ManualRetryBlocked
        );
        assert_eq!(
            retry_state("failed", Some("network_timeout"), true, false),
            AdminCrawlRetryState::ManualRetryAvailable
        );
        assert_eq!(
            retry_state("succeeded", None, false, false),
            AdminCrawlRetryState::NotApplicable
        );
    }
}
