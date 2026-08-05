use sqlx::{Row, postgres::PgPoolOptions, query, query_scalar};
use techatlas_database::{PostgresCsvImportRepository, PostgresDomainRepository, run_migrations};
use techatlas_models::{
    CanonicalDomain, CsvDomainImport, CsvImportRepository, CsvImportRowStatus, CsvImportService,
    DomainCreationDefaults, DomainRepository, NewDomain,
};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ImageExt, runners::AsyncRunner},
};

#[tokio::test]
async fn mixed_csv_import_persists_traceable_rows_and_reuses_its_source()
-> Result<(), Box<dyn std::error::Error>> {
    let container = Postgres::default().with_tag("16-alpine").start().await?;
    let port = container.get_host_port_ipv4(5432).await?;
    let database_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;
    run_migrations(&pool).await?;

    let domain_repository = PostgresDomainRepository::new(pool.clone());
    let existing = domain_repository
        .create(new_domain("existing.example"))
        .await?;
    let service =
        CsvImportService::with_default_policy(PostgresCsvImportRepository::new(pool.clone()));
    let document = CsvDomainImport::parse(include_bytes!(
        "../../../tests/fixtures/imports/mixed-domains.csv"
    ))?;

    let result = service
        .import("partner-list", "ops@example.test", document)
        .await?;

    assert_eq!(result.accepted_row_count, 1);
    assert_eq!(result.duplicate_row_count, 2);
    assert_eq!(result.rejected_row_count, 1);
    assert_eq!(
        result.rows.iter().map(|row| row.status).collect::<Vec<_>>(),
        vec![
            CsvImportRowStatus::Duplicate,
            CsvImportRowStatus::Accepted,
            CsvImportRowStatus::Rejected,
            CsvImportRowStatus::Duplicate,
        ]
    );
    assert_eq!(result.rows[0].domain_id, Some(existing.id().as_uuid()));
    assert_eq!(
        result.rows[1].canonical_domain.as_deref(),
        Some("fresh.example")
    );
    assert_eq!(
        result.rows[2].error.map(|error| error.as_str()),
        Some("invalid_domain")
    );
    assert_eq!(
        result.rows[3].canonical_domain.as_deref(),
        Some("fresh.example")
    );

    let import: (String, i32, i32, i32) = sqlx::query_as(
        "SELECT status, accepted_row_count, duplicate_row_count, rejected_row_count \
         FROM imports WHERE id = $1",
    )
    .bind(result.import_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(import, ("completed".to_owned(), 1, 2, 1));

    let rows = query(
        "SELECT row_number, status, canonical_domain, error_code \
         FROM import_rows WHERE import_id = $1 ORDER BY row_number",
    )
    .bind(result.import_id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].try_get::<i32, _>("row_number")?, 1);
    assert_eq!(rows[0].try_get::<String, _>("status")?, "duplicate");
    assert_eq!(
        rows[1].try_get::<String, _>("canonical_domain")?,
        "fresh.example"
    );
    assert_eq!(
        rows[2].try_get::<String, _>("error_code")?,
        "invalid_domain"
    );

    let source_memberships: i64 = query_scalar(
        "SELECT COUNT(*) FROM domain_source_domains WHERE source_id = \
         (SELECT source_id FROM imports WHERE id = $1)",
    )
    .bind(result.import_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(source_memberships, 2);

    let audit: (String, String, i32, i32, i32) = sqlx::query_as(
        "SELECT actor_subject, action, (metadata->>'accepted_row_count')::INTEGER, \
         (metadata->>'duplicate_row_count')::INTEGER, (metadata->>'rejected_row_count')::INTEGER \
         FROM admin_audit_events WHERE resource_kind = 'import' AND resource_id = $1",
    )
    .bind(result.import_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        audit,
        (
            "ops@example.test".to_owned(),
            "import.created".to_owned(),
            1,
            2,
            1,
        )
    );

    let repeated = CsvDomainImport::parse(b"domain\nanother.example\n")?;
    service
        .import("partner-list", "ops@example.test", repeated)
        .await?;
    let source_count: i64 = query_scalar(
        "SELECT COUNT(*) FROM domain_sources WHERE kind = 'csv' AND name = 'partner-list' AND archived_at IS NULL",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(source_count, 1);

    let policy: (String, i64) = sqlx::query_as(
        "SELECT priority, EXTRACT(EPOCH FROM desired_interval)::BIGINT \
         FROM crawl_policies WHERE domain_id = $1",
    )
    .bind(result.rows[1].domain_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(policy, ("medium".to_owned(), 604_800));

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn import_rolls_back_all_records_when_default_policy_creation_fails()
-> Result<(), Box<dyn std::error::Error>> {
    let container = Postgres::default().with_tag("16-alpine").start().await?;
    let port = container.get_host_port_ipv4(5432).await?;
    let database_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;
    run_migrations(&pool).await?;
    query(
        "CREATE FUNCTION reject_csv_policy_for_test() RETURNS TRIGGER LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'test policy failure'; END; $$",
    )
    .execute(&pool)
    .await?;
    query(
        "CREATE TRIGGER reject_csv_policy_for_test \
         BEFORE INSERT ON crawl_policies \
         FOR EACH ROW EXECUTE FUNCTION reject_csv_policy_for_test()",
    )
    .execute(&pool)
    .await?;

    let repository = PostgresCsvImportRepository::new(pool.clone());
    let document = CsvDomainImport::parse(b"domain\nrollback.example\n")?;
    assert!(
        repository
            .import_csv(
                techatlas_models::CsvImportCommand::new(
                    "failing-list",
                    "ops@example.test",
                    document
                )?,
                DomainCreationDefaults::default(),
            )
            .await
            .is_err()
    );

    let import_count: i64 = query_scalar("SELECT COUNT(*) FROM imports")
        .fetch_one(&pool)
        .await?;
    let source_count: i64 = query_scalar("SELECT COUNT(*) FROM domain_sources")
        .fetch_one(&pool)
        .await?;
    let domain_count: i64 = query_scalar("SELECT COUNT(*) FROM domains")
        .fetch_one(&pool)
        .await?;
    assert_eq!((import_count, source_count, domain_count), (0, 0, 0));

    pool.close().await;
    Ok(())
}

fn new_domain(input: &str) -> NewDomain {
    NewDomain::new(
        CanonicalDomain::parse(input).expect("test domain should normalize"),
        DomainCreationDefaults::default().crawl_policy(),
    )
}
