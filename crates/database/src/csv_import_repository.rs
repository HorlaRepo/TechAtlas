use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::types::PgInterval};
use techatlas_models::{
    AdminImportBatch, AdminImportOperations, AdminOperationError, CanonicalDomain, CrawlPolicy,
    CsvImportCommand, CsvImportRepository, CsvImportRepositoryError, CsvImportResult,
    CsvImportRowError, CsvImportRowInput, CsvImportRowResult, CsvImportRowStatus,
    DomainCreationDefaults, parse_row_domain,
};
use uuid::Uuid;

pub struct PostgresCsvImportRepository {
    pool: PgPool,
}

impl PostgresCsvImportRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AdminImportOperations for PostgresCsvImportRepository {
    async fn import_csv(
        &self,
        source_name: &str,
        actor_subject: &str,
        document: techatlas_models::CsvDomainImport,
    ) -> Result<CsvImportResult, AdminOperationError> {
        let command = CsvImportCommand::new(source_name, actor_subject, document)
            .map_err(|_| AdminOperationError::Validation)?;
        <Self as CsvImportRepository>::import_csv(self, command, DomainCreationDefaults::default())
            .await
            .map_err(|_| AdminOperationError::Unavailable)
    }

    async fn completed_import_batches(
        &self,
        limit: usize,
    ) -> Result<Vec<AdminImportBatch>, AdminOperationError> {
        let limit = i64::try_from(limit).map_err(|_| AdminOperationError::Validation)?;
        let rows = sqlx::query(
            "SELECT imports.id, sources.name AS source_name, imports.completed_at, \
                    COUNT(DISTINCT import_rows.domain_id) AS domain_count \
             FROM imports \
             JOIN domain_sources AS sources ON sources.id = imports.source_id \
             LEFT JOIN import_rows ON import_rows.import_id = imports.id \
                 AND import_rows.domain_id IS NOT NULL \
             WHERE imports.status = 'completed' \
             GROUP BY imports.id, sources.name, imports.completed_at \
             ORDER BY imports.completed_at DESC, imports.id DESC \
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| AdminOperationError::Unavailable)?;
        rows.into_iter()
            .map(|row| {
                Ok(AdminImportBatch {
                    import_id: row
                        .try_get::<Uuid, _>("id")
                        .map_err(|_| AdminOperationError::Unavailable)?
                        .to_string(),
                    source_name: row
                        .try_get("source_name")
                        .map_err(|_| AdminOperationError::Unavailable)?,
                    completed_at: row
                        .try_get("completed_at")
                        .map_err(|_| AdminOperationError::Unavailable)?,
                    domain_count: u64::try_from(
                        row.try_get::<i64, _>("domain_count")
                            .map_err(|_| AdminOperationError::Unavailable)?,
                    )
                    .map_err(|_| AdminOperationError::Unavailable)?,
                })
            })
            .collect()
    }
}

#[async_trait]
impl CsvImportRepository for PostgresCsvImportRepository {
    async fn import_csv(
        &self,
        command: CsvImportCommand,
        defaults: DomainCreationDefaults,
    ) -> Result<CsvImportResult, CsvImportRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let source_id = find_or_create_source(&mut transaction, command.source_name()).await?;
        let import_id = create_import(&mut transaction, source_id, command.initiated_by()).await?;
        let mut result = CsvImportResult {
            import_id,
            source_name: command.source_name().to_owned(),
            accepted_row_count: 0,
            duplicate_row_count: 0,
            rejected_row_count: 0,
            rows: Vec::with_capacity(command.document().rows().len()),
        };

        for input in command.document().rows() {
            let row_result = process_row(
                &mut transaction,
                import_id,
                source_id,
                input,
                defaults.crawl_policy(),
            )
            .await?;
            match row_result.status {
                CsvImportRowStatus::Accepted => {
                    result.accepted_row_count = result
                        .accepted_row_count
                        .checked_add(1)
                        .ok_or(CsvImportRepositoryError::Unavailable)?;
                }
                CsvImportRowStatus::Duplicate => {
                    result.duplicate_row_count = result
                        .duplicate_row_count
                        .checked_add(1)
                        .ok_or(CsvImportRepositoryError::Unavailable)?;
                }
                CsvImportRowStatus::Rejected => {
                    result.rejected_row_count = result
                        .rejected_row_count
                        .checked_add(1)
                        .ok_or(CsvImportRepositoryError::Unavailable)?;
                }
            }
            result.rows.push(row_result);
        }

        complete_import(&mut transaction, &result).await?;
        sqlx::query(
            "INSERT INTO admin_audit_events (actor_subject, action, resource_kind, resource_id, metadata) \
             VALUES ($1, 'import.created', 'import', $2, jsonb_build_object( \
                 'accepted_row_count', $3, 'duplicate_row_count', $4, 'rejected_row_count', $5))",
        )
        .bind(command.initiated_by())
        .bind(import_id)
        .bind(i32::try_from(result.accepted_row_count).map_err(|_| CsvImportRepositoryError::Unavailable)?)
        .bind(i32::try_from(result.duplicate_row_count).map_err(|_| CsvImportRepositoryError::Unavailable)?)
        .bind(i32::try_from(result.rejected_row_count).map_err(|_| CsvImportRepositoryError::Unavailable)?)
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;
        transaction.commit().await.map_err(map_database_error)?;

        Ok(result)
    }
}

async fn find_or_create_source(
    transaction: &mut Transaction<'_, Postgres>,
    source_name: &str,
) -> Result<Uuid, CsvImportRepositoryError> {
    let row = sqlx::query(
        "INSERT INTO domain_sources (kind, name) VALUES ('csv', $1) \
         ON CONFLICT (name) WHERE kind = 'csv' AND archived_at IS NULL \
         DO UPDATE SET name = EXCLUDED.name \
         RETURNING id",
    )
    .bind(source_name)
    .fetch_one(&mut **transaction)
    .await
    .map_err(map_database_error)?;

    row.try_get("id")
        .map_err(|_| CsvImportRepositoryError::Unavailable)
}

async fn create_import(
    transaction: &mut Transaction<'_, Postgres>,
    source_id: Uuid,
    initiated_by: &str,
) -> Result<Uuid, CsvImportRepositoryError> {
    let row = sqlx::query(
        "INSERT INTO imports (source_id, initiated_by, status, started_at) \
         VALUES ($1, $2, 'processing', now()) RETURNING id",
    )
    .bind(source_id)
    .bind(initiated_by)
    .fetch_one(&mut **transaction)
    .await
    .map_err(map_database_error)?;

    row.try_get("id")
        .map_err(|_| CsvImportRepositoryError::Unavailable)
}

async fn process_row(
    transaction: &mut Transaction<'_, Postgres>,
    import_id: Uuid,
    source_id: Uuid,
    input: &CsvImportRowInput,
    default_policy: CrawlPolicy,
) -> Result<CsvImportRowResult, CsvImportRepositoryError> {
    let row_number =
        i32::try_from(input.row_number()).map_err(|_| CsvImportRepositoryError::Unavailable)?;
    let parsed_domain = parse_row_domain(input.domain());
    let result = match parsed_domain {
        Err(error) => CsvImportRowResult {
            row_number: input.row_number(),
            input_domain: input.domain().to_owned(),
            canonical_domain: None,
            status: CsvImportRowStatus::Rejected,
            domain_id: None,
            error: Some(error),
        },
        Ok(canonical_domain) => {
            let inserted_domain =
                insert_domain_if_absent(transaction, &canonical_domain, default_policy).await?;
            let (domain_id, status) = match inserted_domain {
                Some(domain_id) => (domain_id, CsvImportRowStatus::Accepted),
                None => (
                    find_active_domain_id(transaction, &canonical_domain).await?,
                    CsvImportRowStatus::Duplicate,
                ),
            };
            upsert_source_membership(transaction, domain_id, source_id, import_id).await?;

            CsvImportRowResult {
                row_number: input.row_number(),
                input_domain: input.domain().to_owned(),
                canonical_domain: Some(canonical_domain.as_str().to_owned()),
                status,
                domain_id: Some(domain_id),
                error: None,
            }
        }
    };

    insert_row(transaction, import_id, row_number, &result).await?;

    Ok(result)
}

async fn insert_domain_if_absent(
    transaction: &mut Transaction<'_, Postgres>,
    canonical_domain: &CanonicalDomain,
    crawl_policy: CrawlPolicy,
) -> Result<Option<Uuid>, CsvImportRepositoryError> {
    let row = sqlx::query(
        "INSERT INTO domains (canonical_domain) VALUES ($1) \
         ON CONFLICT (canonical_domain) WHERE archived_at IS NULL \
         DO NOTHING RETURNING id",
    )
    .bind(canonical_domain.as_str())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(map_database_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let domain_id: Uuid = row
        .try_get("id")
        .map_err(|_| CsvImportRepositoryError::Unavailable)?;
    let interval = PgInterval::try_from(crawl_policy.desired_interval())
        .map_err(|_| CsvImportRepositoryError::Unavailable)?;

    sqlx::query(
        "INSERT INTO crawl_policies (domain_id, priority, desired_interval) VALUES ($1, $2, $3)",
    )
    .bind(domain_id)
    .bind(crawl_policy.priority().as_str())
    .bind(interval)
    .execute(&mut **transaction)
    .await
    .map_err(map_database_error)?;

    Ok(Some(domain_id))
}

async fn find_active_domain_id(
    transaction: &mut Transaction<'_, Postgres>,
    canonical_domain: &CanonicalDomain,
) -> Result<Uuid, CsvImportRepositoryError> {
    let row =
        sqlx::query("SELECT id FROM domains WHERE canonical_domain = $1 AND archived_at IS NULL")
            .bind(canonical_domain.as_str())
            .fetch_optional(&mut **transaction)
            .await
            .map_err(map_database_error)?
            .ok_or(CsvImportRepositoryError::Unavailable)?;

    row.try_get("id")
        .map_err(|_| CsvImportRepositoryError::Unavailable)
}

async fn upsert_source_membership(
    transaction: &mut Transaction<'_, Postgres>,
    domain_id: Uuid,
    source_id: Uuid,
    import_id: Uuid,
) -> Result<(), CsvImportRepositoryError> {
    sqlx::query(
        "INSERT INTO domain_source_domains (domain_id, source_id, first_import_id) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (domain_id, source_id) DO UPDATE SET last_seen_at = now()",
    )
    .bind(domain_id)
    .bind(source_id)
    .bind(import_id)
    .execute(&mut **transaction)
    .await
    .map_err(map_database_error)?;

    Ok(())
}

async fn insert_row(
    transaction: &mut Transaction<'_, Postgres>,
    import_id: Uuid,
    row_number: i32,
    result: &CsvImportRowResult,
) -> Result<(), CsvImportRepositoryError> {
    let error_code = result.error.map(CsvImportRowError::as_str);
    let error_summary = result.error.map(CsvImportRowError::summary);

    sqlx::query(
        "INSERT INTO import_rows \
         (import_id, row_number, input_values, canonical_domain, domain_id, status, error_code, error_summary) \
         VALUES ($1, $2, jsonb_build_object('domain', $3::TEXT), $4, $5, $6, $7, $8)",
    )
    .bind(import_id)
    .bind(row_number)
    .bind(&result.input_domain)
    .bind(&result.canonical_domain)
    .bind(result.domain_id)
    .bind(result.status.as_str())
    .bind(error_code)
    .bind(error_summary)
    .execute(&mut **transaction)
    .await
    .map_err(map_database_error)?;

    Ok(())
}

async fn complete_import(
    transaction: &mut Transaction<'_, Postgres>,
    result: &CsvImportResult,
) -> Result<(), CsvImportRepositoryError> {
    let accepted = i32::try_from(result.accepted_row_count)
        .map_err(|_| CsvImportRepositoryError::Unavailable)?;
    let duplicate = i32::try_from(result.duplicate_row_count)
        .map_err(|_| CsvImportRepositoryError::Unavailable)?;
    let rejected = i32::try_from(result.rejected_row_count)
        .map_err(|_| CsvImportRepositoryError::Unavailable)?;

    sqlx::query(
        "UPDATE imports \
         SET status = 'completed', accepted_row_count = $2, duplicate_row_count = $3, \
             rejected_row_count = $4, completed_at = now() \
         WHERE id = $1",
    )
    .bind(result.import_id)
    .bind(accepted)
    .bind(duplicate)
    .bind(rejected)
    .execute(&mut **transaction)
    .await
    .map_err(map_database_error)?;

    Ok(())
}

fn map_database_error(_: sqlx::Error) -> CsvImportRepositoryError {
    CsvImportRepositoryError::Unavailable
}
