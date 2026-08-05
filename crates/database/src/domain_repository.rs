use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::types::PgInterval};
use techatlas_models::{
    CanonicalDomain, Domain, DomainId, DomainRepository, DomainRepositoryError, NewDomain,
};
use uuid::Uuid;

const ACTIVE_DOMAIN_UNIQUENESS_CONSTRAINT: &str = "domains_active_canonical_domain_key";

pub struct PostgresDomainRepository {
    pool: PgPool,
}

impl PostgresDomainRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DomainRepository for PostgresDomainRepository {
    async fn create(&self, domain: NewDomain) -> Result<Domain, DomainRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let domain = insert_domain(&mut transaction, domain).await?;

        transaction.commit().await.map_err(map_database_error)?;

        Ok(domain)
    }

    async fn create_with_audit(
        &self,
        domain: NewDomain,
        actor_subject: &str,
    ) -> Result<Domain, DomainRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(map_database_error)?;
        let domain = insert_domain(&mut transaction, domain).await?;

        sqlx::query(
            "INSERT INTO admin_audit_events (actor_subject, action, domain_id, resource_kind, resource_id) VALUES ($1, $2, $3, 'domain', $3)",
        )
        .bind(actor_subject)
        .bind("domain.created")
        .bind(domain.id().as_uuid())
        .execute(&mut *transaction)
        .await
        .map_err(map_database_error)?;

        transaction.commit().await.map_err(map_database_error)?;

        Ok(domain)
    }

    async fn find_active_by_canonical(
        &self,
        canonical_domain: &CanonicalDomain,
    ) -> Result<Option<Domain>, DomainRepositoryError> {
        let row = sqlx::query(
            "SELECT id, canonical_domain FROM domains WHERE canonical_domain = $1 AND archived_at IS NULL",
        )
        .bind(canonical_domain.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_database_error)?;

        row.map(row_to_domain).transpose()
    }

    async fn list_active(
        &self,
        after: Option<&CanonicalDomain>,
        limit: usize,
    ) -> Result<Vec<Domain>, DomainRepositoryError> {
        let limit = i64::try_from(limit).map_err(|_| DomainRepositoryError::Unavailable)?;
        let after = after.map(CanonicalDomain::as_str);
        let rows = sqlx::query(
            "SELECT id, canonical_domain FROM domains \
             WHERE archived_at IS NULL AND ($1::TEXT IS NULL OR canonical_domain > $1) \
             ORDER BY canonical_domain ASC LIMIT $2",
        )
        .bind(after)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_database_error)?;

        rows.into_iter().map(row_to_domain).collect()
    }
}

async fn insert_domain(
    transaction: &mut Transaction<'_, Postgres>,
    domain: NewDomain,
) -> Result<Domain, DomainRepositoryError> {
    let interval = PgInterval::try_from(domain.crawl_policy().desired_interval())
        .map_err(|_| DomainRepositoryError::Unavailable)?;
    let row = sqlx::query(
        "INSERT INTO domains (canonical_domain) VALUES ($1) RETURNING id, canonical_domain",
    )
    .bind(domain.canonical_domain().as_str())
    .fetch_one(&mut **transaction)
    .await
    .map_err(map_database_error)?;
    let id: Uuid = row
        .try_get("id")
        .map_err(|_| DomainRepositoryError::Unavailable)?;
    let canonical_domain: String = row
        .try_get("canonical_domain")
        .map_err(|_| DomainRepositoryError::Unavailable)?;

    sqlx::query(
        "INSERT INTO crawl_policies (domain_id, priority, desired_interval) VALUES ($1, $2, $3)",
    )
    .bind(id)
    .bind(domain.crawl_policy().priority().as_str())
    .bind(interval)
    .execute(&mut **transaction)
    .await
    .map_err(map_database_error)?;

    let canonical_domain = CanonicalDomain::parse(&canonical_domain)
        .map_err(|_| DomainRepositoryError::Unavailable)?;

    Ok(Domain::new(DomainId::from_uuid(id), canonical_domain))
}

fn row_to_domain(row: sqlx::postgres::PgRow) -> Result<Domain, DomainRepositoryError> {
    let id: Uuid = row
        .try_get("id")
        .map_err(|_| DomainRepositoryError::Unavailable)?;
    let canonical_domain: String = row
        .try_get("canonical_domain")
        .map_err(|_| DomainRepositoryError::Unavailable)?;
    let canonical_domain = CanonicalDomain::parse(&canonical_domain)
        .map_err(|_| DomainRepositoryError::Unavailable)?;

    Ok(Domain::new(DomainId::from_uuid(id), canonical_domain))
}

fn map_database_error(error: sqlx::Error) -> DomainRepositoryError {
    if let sqlx::Error::Database(database_error) = &error
        && database_error.code().as_deref() == Some("23505")
        && database_error.constraint() == Some(ACTIVE_DOMAIN_UNIQUENESS_CONSTRAINT)
    {
        return DomainRepositoryError::Duplicate;
    }

    DomainRepositoryError::Unavailable
}
