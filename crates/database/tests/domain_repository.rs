use sqlx::{postgres::PgPoolOptions, query, query_scalar};
use techatlas_database::{PostgresDomainRepository, run_migrations};
use techatlas_models::{
    CanonicalDomain, DomainCreationDefaults, DomainRepository, DomainRepositoryError, NewDomain,
};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ImageExt, runners::AsyncRunner},
};

#[tokio::test]
async fn repository_creates_active_domains_with_default_policy_and_rolls_back_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let container = Postgres::default().with_tag("16-alpine").start().await?;
    let port = container.get_host_port_ipv4(5432).await?;
    let database_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;
    run_migrations(&pool).await?;

    let repository = PostgresDomainRepository::new(pool.clone());
    let domain = repository.create(new_domain("example.com")).await?;
    assert_eq!(domain.canonical_domain().as_str(), "example.com");
    let found = repository
        .find_active_by_canonical(&CanonicalDomain::parse("HTTPS://EXAMPLE.COM/path")?)
        .await?
        .expect("created domain should be found by its normalized identity");
    assert_eq!(found.id(), domain.id());

    let policy: (String, i64) = sqlx::query_as(
        "SELECT priority, EXTRACT(EPOCH FROM desired_interval)::BIGINT \
         FROM crawl_policies WHERE domain_id = $1",
    )
    .bind(domain.id().as_uuid())
    .fetch_one(&pool)
    .await?;
    assert_eq!(policy, ("medium".to_owned(), 604_800));
    assert_eq!(
        repository.create(new_domain("example.com")).await,
        Err(DomainRepositoryError::Duplicate)
    );

    let manually_created = repository
        .create_with_audit(new_domain("manual.example"), "bootstrap-admin")
        .await?;
    let audit: (String, String, String) = sqlx::query_as(
        "SELECT actor_subject, action, domain_id::TEXT FROM admin_audit_events WHERE domain_id = $1",
    )
    .bind(manually_created.id().as_uuid())
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        audit,
        (
            "bootstrap-admin".to_owned(),
            "domain.created".to_owned(),
            manually_created.id().as_uuid().to_string(),
        )
    );

    let listed = repository.list_active(None, 10).await?;
    assert_eq!(
        listed
            .iter()
            .map(|domain| domain.canonical_domain().as_str())
            .collect::<Vec<_>>(),
        vec!["example.com", "manual.example"]
    );

    query(
        "CREATE FUNCTION reject_crawl_policy_for_test() RETURNS TRIGGER LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'test policy failure'; END; $$",
    )
    .execute(&pool)
    .await?;
    query(
        "CREATE TRIGGER reject_crawl_policy_for_test \
         BEFORE INSERT ON crawl_policies \
         FOR EACH ROW EXECUTE FUNCTION reject_crawl_policy_for_test()",
    )
    .execute(&pool)
    .await?;

    assert_eq!(
        repository.create(new_domain("rollback.example")).await,
        Err(DomainRepositoryError::Unavailable)
    );
    let persisted: bool = query_scalar(
        "SELECT EXISTS (SELECT 1 FROM domains WHERE canonical_domain = 'rollback.example')",
    )
    .fetch_one(&pool)
    .await?;
    assert!(!persisted);

    pool.close().await;
    Ok(())
}

fn new_domain(input: &str) -> NewDomain {
    NewDomain::new(
        CanonicalDomain::parse(input).expect("test domain should normalize"),
        DomainCreationDefaults::default().crawl_policy(),
    )
}
