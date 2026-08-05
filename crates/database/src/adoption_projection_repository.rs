use async_trait::async_trait;
use sqlx::PgPool;
use techatlas_models::{
    AdoptionCaptureResult, AdoptionHistoryRebuildResult, AdoptionProjectionOperations,
    SchedulerRepositoryError,
};
use time::{Date, OffsetDateTime, Time};

pub struct PostgresAdoptionProjectionRepository {
    pool: PgPool,
}

impl PostgresAdoptionProjectionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AdoptionProjectionOperations for PostgresAdoptionProjectionRepository {
    async fn capture_daily_adoption(
        &self,
        observed_on: Date,
        captured_at: OffsetDateTime,
    ) -> Result<AdoptionCaptureResult, SchedulerRepositoryError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| SchedulerRepositoryError::Unavailable)?;
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM technology_adoption_daily WHERE observed_on = $1)",
        )
        .bind(observed_on)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| SchedulerRepositoryError::Unavailable)?;
        if exists {
            transaction
                .commit()
                .await
                .map_err(|_| SchedulerRepositoryError::Unavailable)?;
            return Ok(AdoptionCaptureResult {
                inserted_rows: 0,
                already_captured: true,
            });
        }

        let result = sqlx::query(
            "INSERT INTO technology_adoption_daily (observed_on, technology_id, domain_count, captured_at) \
             SELECT $1, technologies.id, COUNT(domain_current_technologies.domain_id)::BIGINT, $2 \
             FROM technologies \
             LEFT JOIN domain_current_technologies \
                ON domain_current_technologies.technology_id = technologies.id \
             GROUP BY technologies.id",
        )
        .bind(observed_on)
        .bind(captured_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| SchedulerRepositoryError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| SchedulerRepositoryError::Unavailable)?;
        Ok(AdoptionCaptureResult {
            inserted_rows: result.rows_affected(),
            already_captured: false,
        })
    }

    async fn rebuild_adoption_history(
        &self,
        from: Date,
        to: Date,
        rebuilt_at: OffsetDateTime,
    ) -> Result<AdoptionHistoryRebuildResult, SchedulerRepositoryError> {
        let mut day = from;
        let mut rebuilt_days = 0_u64;
        let mut inserted_rows = 0_u64;
        loop {
            let cutoff = day
                .next_day()
                .ok_or(SchedulerRepositoryError::Unavailable)?
                .with_time(Time::MIDNIGHT)
                .assume_utc();
            let mut transaction = self
                .pool
                .begin()
                .await
                .map_err(|_| SchedulerRepositoryError::Unavailable)?;
            sqlx::query("DELETE FROM technology_adoption_daily WHERE observed_on = $1")
                .bind(day)
                .execute(&mut *transaction)
                .await
                .map_err(|_| SchedulerRepositoryError::Unavailable)?;
            let result = sqlx::query(
                "WITH latest_snapshots AS ( \
                    SELECT DISTINCT ON (crawl_snapshots.domain_id) crawl_snapshots.domain_id, crawl_snapshots.id \
                    FROM crawl_snapshots \
                    JOIN domains ON domains.id = crawl_snapshots.domain_id AND domains.archived_at IS NULL \
                    WHERE crawl_snapshots.captured_at < $2 \
                    ORDER BY crawl_snapshots.domain_id, crawl_snapshots.captured_at DESC \
                 ), observed_counts AS ( \
                    SELECT detections.technology_id, COUNT(DISTINCT latest_snapshots.domain_id)::BIGINT AS domain_count \
                    FROM latest_snapshots \
                    JOIN detections ON detections.crawl_snapshot_id = latest_snapshots.id \
                    GROUP BY detections.technology_id \
                 ) \
                 INSERT INTO technology_adoption_daily (observed_on, technology_id, domain_count, captured_at) \
                 SELECT $1, technologies.id, COALESCE(observed_counts.domain_count, 0), $3 \
                 FROM technologies \
                 LEFT JOIN observed_counts ON observed_counts.technology_id = technologies.id \
                 WHERE technologies.created_at < $2",
            )
            .bind(day)
            .bind(cutoff)
            .bind(rebuilt_at)
            .execute(&mut *transaction)
            .await
            .map_err(|_| SchedulerRepositoryError::Unavailable)?;
            transaction
                .commit()
                .await
                .map_err(|_| SchedulerRepositoryError::Unavailable)?;
            rebuilt_days = rebuilt_days.saturating_add(1);
            inserted_rows = inserted_rows.saturating_add(result.rows_affected());
            if day == to {
                break;
            }
            day = day
                .next_day()
                .ok_or(SchedulerRepositoryError::Unavailable)?;
        }
        Ok(AdoptionHistoryRebuildResult {
            rebuilt_days,
            inserted_rows,
        })
    }
}
