use sqlx::{PgPool, Row};
use std::time::Duration;
use techatlas_models::{SearchDomainProjection, SearchTechnologyProjection};
use time::OffsetDateTime;
use uuid::Uuid;

pub struct PostgresSearchIndexRepository {
    pool: PgPool,
}

impl PostgresSearchIndexRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn claim_ready(
        &self,
        now: OffsetDateTime,
        lease: Duration,
        limit: usize,
    ) -> Result<Vec<SearchIndexJob>, SearchIndexRepositoryError> {
        let lease =
            time::Duration::try_from(lease).map_err(|_| SearchIndexRepositoryError::Unavailable)?;
        let limit = i64::try_from(limit).map_err(|_| SearchIndexRepositoryError::Unavailable)?;
        let rows = sqlx::query(
            "WITH claimed AS ( \
                 SELECT id FROM search_index_outbox \
                 WHERE (status IN ('pending', 'retry') AND next_attempt_at <= $1) \
                    OR (status = 'processing' AND claimed_at <= $1 - $2) \
                 ORDER BY next_attempt_at, created_at \
                 FOR UPDATE SKIP LOCKED LIMIT $3 \
             ) \
             UPDATE search_index_outbox AS outbox \
             SET status = 'processing', claimed_at = $1, attempt_count = attempt_count + 1, updated_at = $1 \
             FROM claimed WHERE outbox.id = claimed.id \
             RETURNING outbox.id, outbox.domain_id, outbox.attempt_count",
        )
        .bind(now)
        .bind(lease)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| SearchIndexRepositoryError::Unavailable)?;
        rows.into_iter()
            .map(|row| {
                Ok(SearchIndexJob {
                    id: row
                        .try_get("id")
                        .map_err(|_| SearchIndexRepositoryError::Unavailable)?,
                    domain_id: row
                        .try_get("domain_id")
                        .map_err(|_| SearchIndexRepositoryError::Unavailable)?,
                    attempt_count: u8::try_from(
                        row.try_get::<i16, _>("attempt_count")
                            .map_err(|_| SearchIndexRepositoryError::Unavailable)?,
                    )
                    .map_err(|_| SearchIndexRepositoryError::Unavailable)?,
                })
            })
            .collect()
    }

    pub async fn projection(
        &self,
        domain_id: Uuid,
    ) -> Result<Option<SearchDomainProjection>, SearchIndexRepositoryError> {
        let domain = sqlx::query(
            "SELECT domains.canonical_domain, crawl_policies.last_successful_crawl_at, country.country_code, \
                 COALESCE(MAX(domain_current_technologies.updated_at), domains.updated_at) AS updated_at \
             FROM domains \
             JOIN crawl_policies ON crawl_policies.domain_id = domains.id \
             LEFT JOIN domain_current_technologies ON domain_current_technologies.domain_id = domains.id \
             LEFT JOIN LATERAL (SELECT crawl_country_observations.country_code FROM crawl_country_observations \
                 JOIN crawl_snapshots ON crawl_snapshots.id = crawl_country_observations.crawl_snapshot_id \
                 WHERE crawl_snapshots.domain_id = domains.id ORDER BY crawl_snapshots.captured_at DESC LIMIT 1) country ON TRUE \
             WHERE domains.id = $1 AND domains.archived_at IS NULL \
             GROUP BY domains.id, crawl_policies.last_successful_crawl_at, country.country_code",
        )
        .bind(domain_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| SearchIndexRepositoryError::Unavailable)?;
        let Some(domain) = domain else {
            return Ok(None);
        };
        let technologies = sqlx::query(
            "SELECT technologies.slug, technology_categories.slug AS category_slug, detections.confidence, \
                 domain_current_technologies.last_observed_at \
             FROM domain_current_technologies \
             JOIN technologies ON technologies.id = domain_current_technologies.technology_id \
             JOIN technology_categories ON technology_categories.id = technologies.category_id \
             JOIN detections ON detections.id = domain_current_technologies.current_detection_id \
             WHERE domain_current_technologies.domain_id = $1 \
             ORDER BY technologies.slug",
        )
        .bind(domain_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| SearchIndexRepositoryError::Unavailable)?
        .into_iter()
        .map(|row| Ok(SearchTechnologyProjection {
            slug: row.try_get("slug").map_err(|_| SearchIndexRepositoryError::Unavailable)?,
            category_slug: row.try_get("category_slug").map_err(|_| SearchIndexRepositoryError::Unavailable)?,
            confidence: u8::try_from(row.try_get::<i16, _>("confidence").map_err(|_| SearchIndexRepositoryError::Unavailable)?)
                .map_err(|_| SearchIndexRepositoryError::Unavailable)?,
            last_observed_at: row.try_get("last_observed_at").map_err(|_| SearchIndexRepositoryError::Unavailable)?,
        })).collect::<Result<Vec<_>, _>>()?;
        Ok(Some(SearchDomainProjection {
            domain_id,
            canonical_domain: domain
                .try_get("canonical_domain")
                .map_err(|_| SearchIndexRepositoryError::Unavailable)?,
            technologies,
            last_crawled_at: domain
                .try_get("last_successful_crawl_at")
                .map_err(|_| SearchIndexRepositoryError::Unavailable)?,
            country_code: domain
                .try_get("country_code")
                .map_err(|_| SearchIndexRepositoryError::Unavailable)?,
            updated_at: domain
                .try_get("updated_at")
                .map_err(|_| SearchIndexRepositoryError::Unavailable)?,
        }))
    }

    pub async fn all_projections(
        &self,
    ) -> Result<Vec<SearchDomainProjection>, SearchIndexRepositoryError> {
        let domain_ids = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM domains WHERE archived_at IS NULL ORDER BY canonical_domain",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|_| SearchIndexRepositoryError::Unavailable)?;
        let mut projections = Vec::with_capacity(domain_ids.len());
        for domain_id in domain_ids {
            if let Some(projection) = self.projection(domain_id).await? {
                projections.push(projection);
            }
        }
        Ok(projections)
    }

    pub async fn complete(
        &self,
        job: SearchIndexJob,
        now: OffsetDateTime,
    ) -> Result<(), SearchIndexRepositoryError> {
        let result =
            sqlx::query("DELETE FROM search_index_outbox WHERE id = $1 AND status = 'processing'")
                .bind(job.id)
                .execute(&self.pool)
                .await
                .map_err(|_| SearchIndexRepositoryError::Unavailable)?;
        if result.rows_affected() != 1 {
            return Err(SearchIndexRepositoryError::Unavailable);
        }
        sqlx::query(
            "UPDATE search_index_state SET last_success_at = $1, updated_at = $1 WHERE singleton",
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|_| SearchIndexRepositoryError::Unavailable)?;
        Ok(())
    }

    pub async fn fail(
        &self,
        job: SearchIndexJob,
        now: OffsetDateTime,
        retry_after: Duration,
        max_attempts: u8,
        summary: &str,
    ) -> Result<(), SearchIndexRepositoryError> {
        let retry_after = time::Duration::try_from(retry_after)
            .map_err(|_| SearchIndexRepositoryError::Unavailable)?;
        let status = if job.attempt_count >= max_attempts {
            "dead"
        } else {
            "retry"
        };
        let summary = bounded_summary(summary);
        let result = sqlx::query(
            "UPDATE search_index_outbox SET status = $1, next_attempt_at = $2 + $3, claimed_at = NULL, \
                 last_error = $4, updated_at = $2 WHERE id = $5 AND status = 'processing'",
        )
        .bind(status).bind(now).bind(retry_after).bind(&summary).bind(job.id)
        .execute(&self.pool).await.map_err(|_| SearchIndexRepositoryError::Unavailable)?;
        if result.rows_affected() != 1 {
            return Err(SearchIndexRepositoryError::Unavailable);
        }
        sqlx::query("UPDATE search_index_state SET last_failure_at = $1, last_failure_summary = $2, updated_at = $1 WHERE singleton")
            .bind(now).bind(summary).execute(&self.pool).await.map_err(|_| SearchIndexRepositoryError::Unavailable)?;
        Ok(())
    }

    pub async fn lag(
        &self,
        now: OffsetDateTime,
    ) -> Result<SearchIndexLag, SearchIndexRepositoryError> {
        let row = sqlx::query(
            "SELECT MIN(created_at) AS oldest_pending_at, COUNT(*) FILTER (WHERE status = 'dead') AS dead_count \
             FROM search_index_outbox",
        ).fetch_one(&self.pool).await.map_err(|_| SearchIndexRepositoryError::Unavailable)?;
        let oldest_pending_at: Option<OffsetDateTime> = row
            .try_get("oldest_pending_at")
            .map_err(|_| SearchIndexRepositoryError::Unavailable)?;
        Ok(SearchIndexLag {
            pending_age: oldest_pending_at.map(|created_at| now - created_at),
            dead_count: row
                .try_get::<i64, _>("dead_count")
                .map_err(|_| SearchIndexRepositoryError::Unavailable)?,
        })
    }
}

fn bounded_summary(value: &str) -> String {
    value.trim().chars().take(1024).collect()
}

#[derive(Clone, Copy, Debug)]
pub struct SearchIndexJob {
    pub id: Uuid,
    pub domain_id: Uuid,
    pub attempt_count: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct SearchIndexLag {
    pub pending_age: Option<time::Duration>,
    pub dead_count: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum SearchIndexRepositoryError {
    #[error("search indexing persistence is unavailable")]
    Unavailable,
}
