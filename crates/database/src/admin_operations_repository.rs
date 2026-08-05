use async_trait::async_trait;
use sqlx::{PgPool, Row};
use techatlas_models::{
    AdminActivityKind, AdminOperationError, AdminOperationsActivity, AdminOperationsOverview,
    AdminOperationsRead, AdminThroughputPoint, AdminWorker, WorkerHeartbeat,
    WorkerHeartbeatOperations,
};
use time::{Duration, OffsetDateTime};

pub struct PostgresAdminOperationsRepository {
    pool: PgPool,
}

impl PostgresAdminOperationsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AdminOperationsRead for PostgresAdminOperationsRepository {
    async fn overview(
        &self,
        now: OffsetDateTime,
    ) -> Result<AdminOperationsOverview, AdminOperationError> {
        let since = now - Duration::hours(24);
        let counts = sqlx::query(
            "SELECT
                (SELECT COUNT(*) FROM domains WHERE archived_at IS NULL) AS domain_count,
                (SELECT COUNT(*) FROM technologies) AS technology_count,
                (SELECT COUNT(*) FROM domain_current_technologies) AS current_detection_count,
                (SELECT COUNT(*) FROM crawl_attempts WHERE status = 'succeeded') AS successful_crawl_count,
                (SELECT COUNT(*) FROM crawl_policies WHERE is_enabled AND next_crawl_at > $1) AS scheduled_crawl_count",
        )
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| AdminOperationError::Unavailable)?;

        let throughput_rows = sqlx::query(
            "SELECT date_trunc('hour', finished_at) AS observed_at, COUNT(*) AS completed_count
             FROM crawl_attempts
             WHERE status = 'succeeded' AND finished_at >= $1
             GROUP BY 1
             ORDER BY 1 ASC",
        )
        .bind(since)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| AdminOperationError::Unavailable)?;
        let throughput = throughput_rows
            .into_iter()
            .map(|row| {
                Ok(AdminThroughputPoint {
                    observed_at: row
                        .try_get("observed_at")
                        .map_err(|_| AdminOperationError::Unavailable)?,
                    completed_count: count(&row, "completed_count")?,
                })
            })
            .collect::<Result<Vec<_>, AdminOperationError>>()?;

        let worker_rows = sqlx::query(
            "SELECT worker_name, region, in_flight_work, completed_total, last_heartbeat_at
             FROM worker_heartbeats
             ORDER BY last_heartbeat_at DESC, worker_name ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|_| AdminOperationError::Unavailable)?;
        let workers = worker_rows
            .into_iter()
            .map(|row| {
                Ok(AdminWorker {
                    name: row
                        .try_get("worker_name")
                        .map_err(|_| AdminOperationError::Unavailable)?,
                    region: row
                        .try_get("region")
                        .map_err(|_| AdminOperationError::Unavailable)?,
                    in_flight_work: u32::try_from(
                        row.try_get::<i32, _>("in_flight_work")
                            .map_err(|_| AdminOperationError::Unavailable)?,
                    )
                    .map_err(|_| AdminOperationError::Unavailable)?,
                    completed_total: count(&row, "completed_total")?,
                    last_heartbeat_at: row
                        .try_get("last_heartbeat_at")
                        .map_err(|_| AdminOperationError::Unavailable)?,
                })
            })
            .collect::<Result<Vec<_>, AdminOperationError>>()?;

        let activity_rows = sqlx::query(
            "SELECT id, action, resource_kind, occurred_at
             FROM (
                SELECT id::text AS id, action, resource_kind, occurred_at
                FROM admin_audit_events
                UNION ALL
                SELECT id::text AS id, 'crawl.completed' AS action, 'crawl' AS resource_kind, finished_at AS occurred_at
                FROM crawl_attempts
                WHERE status = 'succeeded' AND finished_at IS NOT NULL
             ) AS events
             ORDER BY occurred_at DESC, id DESC
             LIMIT 8",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|_| AdminOperationError::Unavailable)?;
        let activity = activity_rows
            .into_iter()
            .map(|row| {
                let action: String = row
                    .try_get("action")
                    .map_err(|_| AdminOperationError::Unavailable)?;
                let resource_kind: String = row
                    .try_get("resource_kind")
                    .map_err(|_| AdminOperationError::Unavailable)?;
                Ok(AdminOperationsActivity {
                    id: row
                        .try_get("id")
                        .map_err(|_| AdminOperationError::Unavailable)?,
                    title: if action == "crawl.completed" {
                        "crawl completed".to_owned()
                    } else {
                        action.replace('.', " ")
                    },
                    description: if action == "crawl.completed" {
                        "successful crawl recorded".to_owned()
                    } else {
                        format!("{resource_kind} operation recorded")
                    },
                    occurred_at: row
                        .try_get("occurred_at")
                        .map_err(|_| AdminOperationError::Unavailable)?,
                    kind: if action == "crawl.completed" {
                        AdminActivityKind::Success
                    } else if action.contains("import") {
                        AdminActivityKind::Discovery
                    } else {
                        AdminActivityKind::System
                    },
                })
            })
            .collect::<Result<Vec<_>, AdminOperationError>>()?;

        Ok(AdminOperationsOverview {
            domain_count: count(&counts, "domain_count")?,
            technology_count: count(&counts, "technology_count")?,
            current_detection_count: count(&counts, "current_detection_count")?,
            successful_crawl_count: count(&counts, "successful_crawl_count")?,
            scheduled_crawl_count: count(&counts, "scheduled_crawl_count")?,
            throughput,
            workers,
            activity,
        })
    }
}

#[async_trait]
impl WorkerHeartbeatOperations for PostgresAdminOperationsRepository {
    async fn heartbeat(&self, heartbeat: WorkerHeartbeat) -> Result<(), AdminOperationError> {
        sqlx::query(
            "INSERT INTO worker_heartbeats (worker_name, region, in_flight_work, last_heartbeat_at)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (worker_name) DO UPDATE SET
                region = EXCLUDED.region,
                in_flight_work = EXCLUDED.in_flight_work,
                last_heartbeat_at = EXCLUDED.last_heartbeat_at",
        )
        .bind(heartbeat.name)
        .bind(heartbeat.region)
        .bind(i32::try_from(heartbeat.in_flight_work).map_err(|_| AdminOperationError::Validation)?)
        .bind(heartbeat.observed_at)
        .execute(&self.pool)
        .await
        .map_err(|_| AdminOperationError::Unavailable)?;
        Ok(())
    }

    async fn record_completion(
        &self,
        heartbeat: WorkerHeartbeat,
    ) -> Result<(), AdminOperationError> {
        sqlx::query(
            "INSERT INTO worker_heartbeats (worker_name, region, in_flight_work, completed_total, last_heartbeat_at)
             VALUES ($1, $2, $3, 1, $4)
             ON CONFLICT (worker_name) DO UPDATE SET
                region = EXCLUDED.region,
                in_flight_work = EXCLUDED.in_flight_work,
                completed_total = worker_heartbeats.completed_total + 1,
                last_heartbeat_at = EXCLUDED.last_heartbeat_at",
        )
        .bind(heartbeat.name)
        .bind(heartbeat.region)
        .bind(i32::try_from(heartbeat.in_flight_work).map_err(|_| AdminOperationError::Validation)?)
        .bind(heartbeat.observed_at)
        .execute(&self.pool)
        .await
        .map_err(|_| AdminOperationError::Unavailable)?;
        Ok(())
    }
}

fn count(row: &sqlx::postgres::PgRow, name: &str) -> Result<u64, AdminOperationError> {
    u64::try_from(
        row.try_get::<i64, _>(name)
            .map_err(|_| AdminOperationError::Unavailable)?,
    )
    .map_err(|_| AdminOperationError::Unavailable)
}
