use sqlx::{postgres::PgPoolOptions, query_as, query_scalar};
use techatlas_database::{PostgresAdminRepository, PostgresDomainRepository, run_migrations};
use techatlas_models::{
    AdminCrawlPolicy, AdminPolicyOperations, CanonicalDomain, CrawlPriority,
    DomainCreationDefaults, DomainRepository, NewDomain,
};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ImageExt, runners::AsyncRunner},
};

#[tokio::test]
async fn policy_update_is_atomic_and_writes_an_audit_event()
-> Result<(), Box<dyn std::error::Error>> {
    let container = Postgres::default().with_tag("16-alpine").start().await?;
    let port = container.get_host_port_ipv4(5432).await?;
    let database_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;
    run_migrations(&pool).await?;

    let domain = CanonicalDomain::parse("policy.example")?;
    let domain_repository = PostgresDomainRepository::new(pool.clone());
    domain_repository
        .create(NewDomain::new(
            domain.clone(),
            DomainCreationDefaults::default().crawl_policy(),
        ))
        .await?;

    let repository = PostgresAdminRepository::new(pool.clone());
    let updated = repository
        .update_policy(
            &domain,
            AdminCrawlPolicy::new(true, CrawlPriority::High, 24)?,
            "operator-123",
        )
        .await?;
    assert_eq!(updated.priority, "high");
    assert_eq!(updated.desired_interval_hours, 24);

    let policy: (bool, String, i64) = query_as(
        "SELECT is_enabled, priority, EXTRACT(EPOCH FROM desired_interval)::BIGINT \
         FROM crawl_policies WHERE domain_id = (SELECT id FROM domains WHERE canonical_domain = $1)",
    )
    .bind(domain.as_str())
    .fetch_one(&pool)
    .await?;
    assert_eq!(policy, (true, "high".to_owned(), 86_400));

    let action: String = query_scalar(
        "SELECT action FROM admin_audit_events \
         WHERE actor_subject = 'operator-123' AND resource_kind = 'domain'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(action, "domain.policy_updated");

    pool.close().await;
    Ok(())
}
