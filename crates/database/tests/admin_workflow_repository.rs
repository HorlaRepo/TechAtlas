use sqlx::{postgres::PgPoolOptions, query_as, query_scalar};
use techatlas_database::{
    PostgresAdminRepository, PostgresAdminWorkflowRepository, run_migrations,
};
use techatlas_models::{
    AdminCrawlPolicy, AdminOperationError, AdminPolicyOperations, AdminSchedulerOperations,
    CanonicalDomain, CrawlPriority, DomainCreationDefaults, DomainRepository, NewDomain,
};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ImageExt, runners::AsyncRunner},
};
use time::OffsetDateTime;

#[tokio::test]
async fn quick_crawl_makes_an_enabled_domain_eligible_and_audits_the_request()
-> Result<(), Box<dyn std::error::Error>> {
    let container = Postgres::default().with_tag("16-alpine").start().await?;
    let port = container.get_host_port_ipv4(5432).await?;
    let database_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;
    run_migrations(&pool).await?;

    let domain = CanonicalDomain::parse("quick-crawl.example")?;
    let domains = techatlas_database::PostgresDomainRepository::new(pool.clone());
    domains
        .create(NewDomain::new(
            domain.clone(),
            DomainCreationDefaults::default().crawl_policy(),
        ))
        .await?;
    let now = OffsetDateTime::now_utc();
    let now = now.replace_nanosecond(now.nanosecond() / 1_000 * 1_000)?;
    let workflow = PostgresAdminWorkflowRepository::new(pool.clone());

    workflow
        .request_domain_crawl(&domain, "operator-123", now)
        .await?;

    let next_crawl_at: OffsetDateTime = query_scalar(
        "SELECT next_crawl_at FROM crawl_policies WHERE domain_id = \
         (SELECT id FROM domains WHERE canonical_domain = $1)",
    )
    .bind(domain.as_str())
    .fetch_one(&pool)
    .await?;
    assert_eq!(next_crawl_at, now);
    let audit: (String, String, String) = query_as(
        "SELECT actor_subject, action, resource_kind FROM admin_audit_events \
         ORDER BY occurred_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        audit,
        (
            "operator-123".to_owned(),
            "domain.crawl_requested".to_owned(),
            "domain".to_owned()
        )
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM crawl_attempts")
            .fetch_one(&pool)
            .await?,
        0
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM crawl_job_outbox")
            .fetch_one(&pool)
            .await?,
        0
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn quick_crawl_rejects_a_disabled_domain_without_an_audit_event()
-> Result<(), Box<dyn std::error::Error>> {
    let container = Postgres::default().with_tag("16-alpine").start().await?;
    let port = container.get_host_port_ipv4(5432).await?;
    let database_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;
    run_migrations(&pool).await?;

    let domain = CanonicalDomain::parse("disabled-quick-crawl.example")?;
    let domains = techatlas_database::PostgresDomainRepository::new(pool.clone());
    domains
        .create(NewDomain::new(
            domain.clone(),
            DomainCreationDefaults::default().crawl_policy(),
        ))
        .await?;
    PostgresAdminRepository::new(pool.clone())
        .update_policy(
            &domain,
            AdminCrawlPolicy::new(false, CrawlPriority::Medium, 168)?,
            "operator-123",
        )
        .await?;

    let result = PostgresAdminWorkflowRepository::new(pool.clone())
        .request_domain_crawl(&domain, "operator-123", OffsetDateTime::now_utc())
        .await;

    assert_eq!(result, Err(AdminOperationError::Conflict));
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM admin_audit_events WHERE action = 'domain.crawl_requested'",
        )
        .fetch_one(&pool)
        .await?,
        0
    );

    pool.close().await;
    Ok(())
}
