use async_trait::async_trait;
use sqlx::{PgPool, Row};
use techatlas_models::{
    CanonicalDomain, PublicRefreshError, PublicRefreshOperations, PublicRefreshRequest,
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

pub struct PostgresPublicRefreshRepository {
    pool: PgPool,
}

impl PostgresPublicRefreshRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PublicRefreshOperations for PostgresPublicRefreshRepository {
    async fn request_refresh(
        &self,
        domain: &CanonicalDomain,
        requested_at: OffsetDateTime,
        cooldown: Duration,
    ) -> Result<PublicRefreshRequest, PublicRefreshError> {
        let next_allowed_at = requested_at
            .checked_add(cooldown)
            .ok_or(PublicRefreshError::Unavailable)?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| PublicRefreshError::Unavailable)?;

        // Locking the policy serializes requests for one domain and keeps the cooldown check
        // race-free without relying on a process-local rate limiter.
        let policy = sqlx::query(
            "SELECT domains.id, crawl_policies.is_enabled FROM domains \
             JOIN crawl_policies ON crawl_policies.domain_id = domains.id \
             WHERE domains.canonical_domain = $1 AND domains.archived_at IS NULL \
             FOR UPDATE OF crawl_policies",
        )
        .bind(domain.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PublicRefreshError::Unavailable)?;
        let Some(policy) = policy else {
            transaction
                .rollback()
                .await
                .map_err(|_| PublicRefreshError::Unavailable)?;
            return Err(PublicRefreshError::NotFound);
        };
        let domain_id: Uuid = policy
            .try_get("id")
            .map_err(|_| PublicRefreshError::Unavailable)?;
        let is_enabled: bool = policy
            .try_get("is_enabled")
            .map_err(|_| PublicRefreshError::Unavailable)?;
        if !is_enabled {
            transaction
                .rollback()
                .await
                .map_err(|_| PublicRefreshError::Unavailable)?;
            return Err(PublicRefreshError::Disabled);
        }

        let last_requested_at: Option<OffsetDateTime> = sqlx::query_scalar(
            "SELECT requested_at FROM public_refresh_requests \
             WHERE domain_id = $1 ORDER BY requested_at DESC LIMIT 1",
        )
        .bind(domain_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PublicRefreshError::Unavailable)?;
        if let Some(last_requested_at) = last_requested_at {
            let next = last_requested_at
                .checked_add(cooldown)
                .ok_or(PublicRefreshError::Unavailable)?;
            if requested_at < next {
                let retry_after_seconds =
                    u64::try_from((next - requested_at).whole_seconds().max(1))
                        .map_err(|_| PublicRefreshError::Unavailable)?;
                transaction
                    .rollback()
                    .await
                    .map_err(|_| PublicRefreshError::Unavailable)?;
                return Err(PublicRefreshError::RateLimited {
                    retry_after_seconds,
                });
            }
        }

        sqlx::query(
            "INSERT INTO public_refresh_requests (domain_id, requested_at, created_at) \
             VALUES ($1, $2, $2)",
        )
        .bind(domain_id)
        .bind(requested_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| PublicRefreshError::Unavailable)?;
        sqlx::query(
            "UPDATE crawl_policies SET next_crawl_at = GREATEST($1, created_at), terminal_failure_at = NULL, updated_at = $1 \
             WHERE domain_id = $2",
        )
        .bind(requested_at)
        .bind(domain_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| PublicRefreshError::Unavailable)?;

        transaction
            .commit()
            .await
            .map_err(|_| PublicRefreshError::Unavailable)?;
        Ok(PublicRefreshRequest {
            accepted_at: requested_at,
            next_allowed_at,
        })
    }
}
