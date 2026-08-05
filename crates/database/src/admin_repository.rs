use async_trait::async_trait;
use sqlx::{PgPool, Row, postgres::types::PgInterval};
use techatlas_models::{
    AdminAuditCursor, AdminAuditEvent, AdminAuditOperations, AdminCrawlPolicy, AdminOperationError,
    AdminPolicyOperations, CanonicalDomain,
};
use uuid::Uuid;

pub struct PostgresAdminRepository {
    pool: PgPool,
}
impl PostgresAdminRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AdminPolicyOperations for PostgresAdminRepository {
    async fn update_policy(
        &self,
        domain: &CanonicalDomain,
        policy: AdminCrawlPolicy,
        actor_subject: &str,
    ) -> Result<AdminCrawlPolicy, AdminOperationError> {
        let priority = policy.priority_enum()?;
        let interval = PgInterval::try_from(std::time::Duration::from_secs(
            u64::from(policy.desired_interval_hours) * 60 * 60,
        ))
        .map_err(|_| AdminOperationError::Validation)?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| AdminOperationError::Unavailable)?;
        let domain_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM domains WHERE canonical_domain = $1 AND archived_at IS NULL FOR UPDATE",
        )
        .bind(domain.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| AdminOperationError::Unavailable)?;
        let domain_id = domain_id.ok_or(AdminOperationError::NotFound)?;
        sqlx::query("UPDATE crawl_policies SET is_enabled = $1, priority = $2, desired_interval = $3, updated_at = now() WHERE domain_id = $4")
            .bind(policy.is_enabled).bind(priority.as_str()).bind(interval).bind(domain_id).execute(&mut *transaction).await.map_err(|_| AdminOperationError::Unavailable)?;
        sqlx::query("INSERT INTO admin_audit_events (actor_subject, action, domain_id, resource_kind, resource_id, metadata) VALUES ($1, 'domain.policy_updated', $2, 'domain', $2, jsonb_build_object('is_enabled', $3, 'priority', $4, 'desired_interval_hours', $5))")
            .bind(actor_subject).bind(domain_id).bind(policy.is_enabled).bind(priority.as_str()).bind(i32::from(policy.desired_interval_hours)).execute(&mut *transaction).await.map_err(|_| AdminOperationError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| AdminOperationError::Unavailable)?;
        Ok(policy)
    }
}

#[async_trait]
impl AdminAuditOperations for PostgresAdminRepository {
    async fn audit_events(
        &self,
        cursor: Option<AdminAuditCursor>,
        limit: usize,
    ) -> Result<(Vec<AdminAuditEvent>, Option<AdminAuditCursor>), AdminOperationError> {
        let cursor_time = cursor.as_ref().map(AdminAuditCursor::occurred_at);
        let cursor_id = cursor
            .as_ref()
            .map(|cursor| Uuid::parse_str(cursor.id()).map_err(|_| AdminOperationError::Validation))
            .transpose()?;
        let rows = sqlx::query("SELECT id, actor_subject, action, resource_kind, resource_id, occurred_at FROM admin_audit_events WHERE ($1::timestamptz IS NULL OR (occurred_at, id) < ($1, $2)) ORDER BY occurred_at DESC, id DESC LIMIT $3")
            .bind(cursor_time).bind(cursor_id).bind(i64::try_from(limit.saturating_add(1)).map_err(|_| AdminOperationError::Unavailable)?).fetch_all(&self.pool).await.map_err(|_| AdminOperationError::Unavailable)?;
        let mut events = rows
            .into_iter()
            .map(|row| {
                Ok(AdminAuditEvent {
                    id: row
                        .try_get::<Uuid, _>("id")
                        .map_err(|_| AdminOperationError::Unavailable)?
                        .to_string(),
                    actor_subject: row
                        .try_get("actor_subject")
                        .map_err(|_| AdminOperationError::Unavailable)?,
                    action: row
                        .try_get("action")
                        .map_err(|_| AdminOperationError::Unavailable)?,
                    resource_kind: row
                        .try_get("resource_kind")
                        .map_err(|_| AdminOperationError::Unavailable)?,
                    resource_id: row
                        .try_get::<Uuid, _>("resource_id")
                        .map_err(|_| AdminOperationError::Unavailable)?
                        .to_string(),
                    occurred_at: row
                        .try_get("occurred_at")
                        .map_err(|_| AdminOperationError::Unavailable)?,
                })
            })
            .collect::<Result<Vec<_>, AdminOperationError>>()?;
        let next = if events.len() > limit {
            events.pop();
            events
                .last()
                .map(|event| AdminAuditCursor::new(event.occurred_at, event.id.clone()))
                .transpose()?
        } else {
            None
        };
        Ok((events, next))
    }
}
