use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::time::Duration;
use techatlas_models::{
    CorrelationId, CrawlAttemptOutcome, CrawlFailure, CrawlJobId, CrawlJobV1,
    CrawlScheduleRepository, DomainId, IdempotencyKey, PendingCrawlJob, SchedulerRepositoryError,
    SchedulerSettings,
};
use time::OffsetDateTime;
use uuid::Uuid;

pub struct PostgresCrawlScheduleRepository {
    pool: PgPool,
}

impl PostgresCrawlScheduleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CrawlScheduleRepository for PostgresCrawlScheduleRepository {
    async fn recover_stale_attempts(
        &self,
        now: OffsetDateTime,
        stale_after: Duration,
        limit: usize,
        settings: SchedulerSettings,
    ) -> Result<usize, SchedulerRepositoryError> {
        if limit == 0 {
            return Ok(0);
        }
        let stale_after = time::Duration::try_from(stale_after)
            .map_err(|_| SchedulerRepositoryError::Unavailable)?;
        let stale_before = now
            .checked_sub(stale_after)
            .ok_or(SchedulerRepositoryError::Unavailable)?;
        let limit = i64::try_from(limit).map_err(|_| SchedulerRepositoryError::Unavailable)?;
        let stale_job_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT job_id \
             FROM crawl_attempts \
             WHERE status = 'running' AND started_at <= $1 \
             ORDER BY started_at ASC, id ASC \
             LIMIT $2",
        )
        .bind(stale_before)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_database_error)?;

        let failure = CrawlFailure::new(
            "worker_lease_expired",
            "Worker did not record an outcome before its crawl lease expired",
        )
        .map_err(|_| SchedulerRepositoryError::Unavailable)?;
        for job_id in &stale_job_ids {
            self.record_attempt_outcome(
                &CrawlJobId::from_uuid(*job_id),
                CrawlAttemptOutcome::Failed(failure.clone()),
                now,
                settings,
            )
            .await?;
        }

        Ok(stale_job_ids.len())
    }

    async fn reserve_due(
        &self,
        now: OffsetDateTime,
        limit: usize,
    ) -> Result<Vec<CrawlJobV1>, SchedulerRepositoryError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).map_err(|_| SchedulerRepositoryError::Unavailable)?;
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let rows = sqlx::query(
            "SELECT crawl_policies.domain_id \
             FROM crawl_policies \
             JOIN domains ON domains.id = crawl_policies.domain_id \
             WHERE domains.archived_at IS NULL \
               AND crawl_policies.is_enabled \
               AND (crawl_policies.next_crawl_at IS NULL OR crawl_policies.next_crawl_at <= $1) \
               AND NOT EXISTS ( \
                   SELECT 1 FROM crawl_attempts \
                   WHERE crawl_attempts.domain_id = crawl_policies.domain_id \
                     AND crawl_attempts.status IN ('queued', 'running') \
               ) \
             ORDER BY \
                CASE crawl_policies.priority \
                    WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END, \
                crawl_policies.next_crawl_at ASC NULLS FIRST, \
                crawl_policies.domain_id ASC \
             FOR UPDATE OF crawl_policies SKIP LOCKED \
             LIMIT $2",
        )
        .bind(now)
        .bind(limit)
        .fetch_all(&mut *transaction)
        .await
        .map_err(map_database_error)?;

        let mut jobs = Vec::with_capacity(rows.len());
        for row in rows {
            let domain_id: Uuid = row
                .try_get("domain_id")
                .map_err(|_| SchedulerRepositoryError::Unavailable)?;
            let job = new_job(domain_id, Uuid::new_v4())?;
            insert_attempt_and_outbox(&mut transaction, &job, 1, now, now).await?;
            sqlx::query(
                "UPDATE crawl_policies \
                 SET next_crawl_at = $1 + desired_interval, \
                     terminal_failure_at = NULL, \
                     updated_at = $1 \
                 WHERE domain_id = $2",
            )
            .bind(now)
            .bind(domain_id)
            .execute(&mut *transaction)
            .await
            .map_err(map_database_error)?;
            jobs.push(job);
        }

        transaction.commit().await.map_err(map_database_error)?;
        Ok(jobs)
    }

    async fn pending_publications(
        &self,
        now: OffsetDateTime,
        limit: usize,
    ) -> Result<Vec<PendingCrawlJob>, SchedulerRepositoryError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = i64::try_from(limit).map_err(|_| SchedulerRepositoryError::Unavailable)?;
        let rows = sqlx::query(
            "SELECT domain_id, job_id, correlation_id, idempotency_key, available_at, publish_attempt_count \
             FROM crawl_job_outbox \
             WHERE published_at IS NULL \
               AND terminal_failure_at IS NULL \
               AND available_at <= $1 \
               AND next_publish_at <= $1 \
             ORDER BY available_at ASC, created_at ASC \
             LIMIT $2",
        )
        .bind(now)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_database_error)?;

        rows.into_iter().map(row_to_pending_job).collect()
    }

    async fn mark_published(
        &self,
        job_id: &CrawlJobId,
        now: OffsetDateTime,
    ) -> Result<(), SchedulerRepositoryError> {
        let updated = sqlx::query(
            "UPDATE crawl_job_outbox \
             SET published_at = $1, updated_at = $1 \
             WHERE job_id = $2 AND published_at IS NULL AND terminal_failure_at IS NULL",
        )
        .bind(now)
        .bind(job_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(map_database_error)?
        .rows_affected();

        if updated == 0 {
            return Err(SchedulerRepositoryError::NotFound);
        }
        Ok(())
    }

    async fn record_publish_failure(
        &self,
        job_id: &CrawlJobId,
        now: OffsetDateTime,
        settings: SchedulerSettings,
    ) -> Result<(), SchedulerRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let row = sqlx::query(
            "SELECT publish_attempt_count, published_at, terminal_failure_at \
             FROM crawl_job_outbox WHERE job_id = $1 FOR UPDATE",
        )
        .bind(job_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database_error)?
        .ok_or(SchedulerRepositoryError::NotFound)?;
        let published_at: Option<OffsetDateTime> = row
            .try_get("published_at")
            .map_err(|_| SchedulerRepositoryError::Unavailable)?;
        let terminal_failure_at: Option<OffsetDateTime> = row
            .try_get("terminal_failure_at")
            .map_err(|_| SchedulerRepositoryError::Unavailable)?;
        if published_at.is_some() || terminal_failure_at.is_some() {
            transaction.commit().await.map_err(map_database_error)?;
            return Ok(());
        }

        let current: i16 = row
            .try_get("publish_attempt_count")
            .map_err(|_| SchedulerRepositoryError::Unavailable)?;
        let next = current
            .checked_add(1)
            .ok_or(SchedulerRepositoryError::Unavailable)?;
        let next_u8 = u8::try_from(next).map_err(|_| SchedulerRepositoryError::Unavailable)?;
        if next_u8 >= settings.max_outbox_publish_attempts() {
            sqlx::query(
                "UPDATE crawl_job_outbox \
                 SET publish_attempt_count = $1, terminal_failure_at = $2, \
                     last_error_summary = 'Redis publication failed', updated_at = $2 \
                 WHERE job_id = $3",
            )
            .bind(next)
            .bind(now)
            .bind(job_id.as_uuid())
            .execute(&mut *transaction)
            .await
            .map_err(map_database_error)?;
        } else {
            let delay = settings
                .outbox_retry_delay(next_u8)
                .ok_or(SchedulerRepositoryError::Unavailable)?;
            let next_publish_at = add_std_duration(now, delay)?;
            sqlx::query(
                "UPDATE crawl_job_outbox \
                 SET publish_attempt_count = $1, next_publish_at = $2, \
                     last_error_summary = 'Redis publication failed', updated_at = $3 \
                 WHERE job_id = $4",
            )
            .bind(next)
            .bind(next_publish_at)
            .bind(now)
            .bind(job_id.as_uuid())
            .execute(&mut *transaction)
            .await
            .map_err(map_database_error)?;
        }

        transaction.commit().await.map_err(map_database_error)
    }

    async fn record_attempt_outcome(
        &self,
        job_id: &CrawlJobId,
        outcome: CrawlAttemptOutcome,
        now: OffsetDateTime,
        settings: SchedulerSettings,
    ) -> Result<(), SchedulerRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let row = sqlx::query(
            "SELECT crawl_attempts.id, crawl_attempts.domain_id, crawl_attempts.correlation_id, \
                    crawl_attempts.attempt_number, crawl_attempts.status \
             FROM crawl_attempts \
             JOIN crawl_policies ON crawl_policies.domain_id = crawl_attempts.domain_id \
             WHERE crawl_attempts.job_id = $1 \
             FOR UPDATE OF crawl_attempts, crawl_policies",
        )
        .bind(job_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database_error)?
        .ok_or(SchedulerRepositoryError::NotFound)?;
        let status: String = row
            .try_get("status")
            .map_err(|_| SchedulerRepositoryError::Unavailable)?;
        if matches!(status.as_str(), "succeeded" | "failed" | "cancelled") {
            transaction.commit().await.map_err(map_database_error)?;
            return Ok(());
        }

        let attempt_id: Uuid = row
            .try_get("id")
            .map_err(|_| SchedulerRepositoryError::Unavailable)?;
        let domain_id: Uuid = row
            .try_get("domain_id")
            .map_err(|_| SchedulerRepositoryError::Unavailable)?;
        let correlation_id: Uuid = row
            .try_get("correlation_id")
            .map_err(|_| SchedulerRepositoryError::Unavailable)?;
        let attempt_number: i16 = row
            .try_get("attempt_number")
            .map_err(|_| SchedulerRepositoryError::Unavailable)?;

        match outcome {
            CrawlAttemptOutcome::Succeeded => {
                sqlx::query(
                    "UPDATE crawl_attempts \
                     SET status = 'succeeded', started_at = COALESCE(started_at, $1), \
                         finished_at = $1 \
                     WHERE id = $2",
                )
                .bind(now)
                .bind(attempt_id)
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
                .bind(now)
                .bind(domain_id)
                .execute(&mut *transaction)
                .await
                .map_err(map_database_error)?;
            }
            CrawlAttemptOutcome::Failed(failure) => {
                sqlx::query(
                    "UPDATE crawl_attempts \
                     SET status = 'failed', started_at = COALESCE(started_at, $1), \
                         finished_at = $1, failure_code = $2, failure_summary = $3 \
                     WHERE id = $4",
                )
                .bind(now)
                .bind(failure.code())
                .bind(failure.summary())
                .bind(attempt_id)
                .execute(&mut *transaction)
                .await
                .map_err(map_database_error)?;

                let attempt_number = u8::try_from(attempt_number)
                    .map_err(|_| SchedulerRepositoryError::Unavailable)?;
                if attempt_number <= settings.max_crawl_retries() {
                    let delay = settings
                        .crawl_retry_delay(attempt_number)
                        .ok_or(SchedulerRepositoryError::Unavailable)?;
                    let retry_at = add_std_duration(now, delay)?;
                    let retry =
                        new_job_with_correlation(domain_id, correlation_id, Uuid::new_v4())?;
                    insert_attempt_and_outbox(
                        &mut transaction,
                        &retry,
                        i16::from(attempt_number) + 1,
                        retry_at,
                        now,
                    )
                    .await?;
                    sqlx::query(
                        "UPDATE crawl_policies \
                         SET last_crawl_at = $1, next_crawl_at = $2, \
                             consecutive_failure_count = consecutive_failure_count + 1, \
                             last_failure_at = $1, updated_at = $1 \
                         WHERE domain_id = $3",
                    )
                    .bind(now)
                    .bind(retry_at)
                    .bind(domain_id)
                    .execute(&mut *transaction)
                    .await
                    .map_err(map_database_error)?;
                } else {
                    sqlx::query(
                        "UPDATE crawl_policies \
                         SET last_crawl_at = $1, next_crawl_at = $1 + desired_interval, \
                             consecutive_failure_count = consecutive_failure_count + 1, \
                             last_failure_at = $1, terminal_failure_at = $1, updated_at = $1 \
                         WHERE domain_id = $2",
                    )
                    .bind(now)
                    .bind(domain_id)
                    .execute(&mut *transaction)
                    .await
                    .map_err(map_database_error)?;
                }
            }
        }

        transaction.commit().await.map_err(map_database_error)
    }
}

fn new_job(domain_id: Uuid, job_id: Uuid) -> Result<CrawlJobV1, SchedulerRepositoryError> {
    new_job_with_correlation(domain_id, Uuid::new_v4(), job_id)
}

fn new_job_with_correlation(
    domain_id: Uuid,
    correlation_id: Uuid,
    job_id: Uuid,
) -> Result<CrawlJobV1, SchedulerRepositoryError> {
    let idempotency_key = IdempotencyKey::parse(&format!("crawl:{job_id}"))
        .map_err(|_| SchedulerRepositoryError::Unavailable)?;
    Ok(CrawlJobV1::new(
        CrawlJobId::from_uuid(job_id),
        DomainId::from_uuid(domain_id),
        CorrelationId::from_uuid(correlation_id),
        idempotency_key,
    ))
}

async fn insert_attempt_and_outbox(
    transaction: &mut Transaction<'_, Postgres>,
    job: &CrawlJobV1,
    attempt_number: i16,
    available_at: OffsetDateTime,
    created_at: OffsetDateTime,
) -> Result<(), SchedulerRepositoryError> {
    let attempt_id: Uuid = sqlx::query_scalar(
        "INSERT INTO crawl_attempts (domain_id, job_id, correlation_id, idempotency_key, \
             attempt_number, queued_at, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(job.domain_id().as_uuid())
    .bind(job.job_id().as_uuid())
    .bind(job.correlation_id().as_uuid())
    .bind(job.idempotency_key().as_str())
    .bind(attempt_number)
    .bind(available_at)
    .bind(created_at)
    .fetch_one(&mut **transaction)
    .await
    .map_err(map_database_error)?;

    sqlx::query(
        "INSERT INTO crawl_job_outbox (crawl_attempt_id, domain_id, job_id, correlation_id, \
             idempotency_key, available_at, next_publish_at, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $6, $7, $7)",
    )
    .bind(attempt_id)
    .bind(job.domain_id().as_uuid())
    .bind(job.job_id().as_uuid())
    .bind(job.correlation_id().as_uuid())
    .bind(job.idempotency_key().as_str())
    .bind(available_at)
    .bind(created_at)
    .execute(&mut **transaction)
    .await
    .map_err(map_database_error)?;

    Ok(())
}

fn row_to_pending_job(
    row: sqlx::postgres::PgRow,
) -> Result<PendingCrawlJob, SchedulerRepositoryError> {
    let domain_id: Uuid = row
        .try_get("domain_id")
        .map_err(|_| SchedulerRepositoryError::Unavailable)?;
    let job_id: Uuid = row
        .try_get("job_id")
        .map_err(|_| SchedulerRepositoryError::Unavailable)?;
    let correlation_id: Uuid = row
        .try_get("correlation_id")
        .map_err(|_| SchedulerRepositoryError::Unavailable)?;
    let idempotency_key: String = row
        .try_get("idempotency_key")
        .map_err(|_| SchedulerRepositoryError::Unavailable)?;
    let available_at: OffsetDateTime = row
        .try_get("available_at")
        .map_err(|_| SchedulerRepositoryError::Unavailable)?;
    let publish_attempt_count: i16 = row
        .try_get("publish_attempt_count")
        .map_err(|_| SchedulerRepositoryError::Unavailable)?;
    let publish_attempt_count =
        u8::try_from(publish_attempt_count).map_err(|_| SchedulerRepositoryError::Unavailable)?;
    let job = CrawlJobV1::new(
        CrawlJobId::from_uuid(job_id),
        DomainId::from_uuid(domain_id),
        CorrelationId::from_uuid(correlation_id),
        IdempotencyKey::parse(&idempotency_key)
            .map_err(|_| SchedulerRepositoryError::Unavailable)?,
    );

    Ok(PendingCrawlJob::new(
        job,
        available_at,
        publish_attempt_count,
    ))
}

fn add_std_duration(
    now: OffsetDateTime,
    duration: Duration,
) -> Result<OffsetDateTime, SchedulerRepositoryError> {
    let duration =
        time::Duration::try_from(duration).map_err(|_| SchedulerRepositoryError::Unavailable)?;
    now.checked_add(duration)
        .ok_or(SchedulerRepositoryError::Unavailable)
}

fn map_database_error(_: sqlx::Error) -> SchedulerRepositoryError {
    SchedulerRepositoryError::Unavailable
}
