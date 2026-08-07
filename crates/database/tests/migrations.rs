use sqlx::{Row, postgres::PgPoolOptions, query, query_scalar};
use std::time::Duration;
use techatlas_database::{
    MIGRATOR, PostgresSearchIndexRepository, expired_raw_artifact_locations, run_migrations,
};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ImageExt, runners::AsyncRunner},
};
use time::OffsetDateTime;

#[tokio::test]
async fn migrations_apply_to_a_blank_postgres_and_enforce_corpus_invariants()
-> Result<(), Box<dyn std::error::Error>> {
    let container = Postgres::default().with_tag("16-alpine").start().await?;
    let port = container.get_host_port_ipv4(5432).await?;
    let database_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;

    run_migrations(&pool).await?;

    let applied_migrations: i64 = query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await?;
    assert_eq!(applied_migrations as usize, MIGRATOR.migrations.len());

    let domain_id: String = query_scalar(
        "INSERT INTO domains (canonical_domain) VALUES ('example.com') RETURNING id::TEXT",
    )
    .fetch_one(&pool)
    .await?;

    assert!(
        query("INSERT INTO domains (canonical_domain) VALUES ('example.com')")
            .execute(&pool)
            .await
            .is_err()
    );

    let audit_event_id: String = query_scalar(
        "INSERT INTO admin_audit_events (actor_subject, action, domain_id, resource_kind, resource_id) \
         VALUES ('migration-test', 'domain.created', $1::UUID, 'domain', $1::UUID) RETURNING id::TEXT",
    )
    .bind(&domain_id)
    .fetch_one(&pool)
    .await?;
    assert!(
        query("UPDATE admin_audit_events SET action = 'changed' WHERE id = $1::UUID")
            .bind(&audit_event_id)
            .execute(&pool)
            .await
            .is_err()
    );

    query("UPDATE domains SET archived_at = now() WHERE id = $1::UUID")
        .bind(&domain_id)
        .execute(&pool)
        .await?;
    query("INSERT INTO domains (canonical_domain) VALUES ('example.com')")
        .execute(&pool)
        .await?;

    let source_id: String = query_scalar(
        "INSERT INTO domain_sources (kind, name) VALUES ('csv', 'Migration fixture') RETURNING id::TEXT",
    )
    .fetch_one(&pool)
    .await?;
    assert!(
        query("INSERT INTO domain_sources (kind, name) VALUES ('csv', 'Migration fixture')")
            .execute(&pool)
            .await
            .is_err()
    );
    let import_id: String = query_scalar(
        "INSERT INTO imports (source_id, initiated_by) VALUES ($1::UUID, 'migration-test') RETURNING id::TEXT",
    )
    .bind(&source_id)
    .fetch_one(&pool)
    .await?;

    query(
        "INSERT INTO import_rows (import_id, row_number, input_values, canonical_domain, domain_id, status) \
         VALUES ($1::UUID, 1, '{\"domain\": \"example.com\"}'::JSONB, 'example.com', $2::UUID, 'accepted')",
    )
    .bind(&import_id)
    .bind(&domain_id)
    .execute(&pool)
    .await?;
    assert!(
        query(
            "INSERT INTO import_rows (import_id, row_number, input_values, status, error_code) \
         VALUES ($1::UUID, 1, '{}'::JSONB, 'rejected', 'duplicate_row_number')",
        )
        .bind(&import_id)
        .execute(&pool)
        .await
        .is_err()
    );

    let snapshot_attempt_id: String = query_scalar(
        "INSERT INTO crawl_attempts (domain_id, job_id, correlation_id, idempotency_key) \
         VALUES ($1::UUID, gen_random_uuid(), gen_random_uuid(), 'snapshot-migration-attempt') \
         RETURNING id::TEXT",
    )
    .bind(&domain_id)
    .fetch_one(&pool)
    .await?;
    query("UPDATE crawl_attempts SET status = 'running', started_at = now() WHERE id = $1::UUID")
        .bind(&snapshot_attempt_id)
        .execute(&pool)
        .await?;
    query(
        "UPDATE crawl_attempts SET status = 'succeeded', finished_at = now() WHERE id = $1::UUID",
    )
    .bind(&snapshot_attempt_id)
    .execute(&pool)
    .await?;
    let snapshot_id: String = query_scalar(
        "INSERT INTO crawl_snapshots (crawl_attempt_id, domain_id, requested_url, final_url, \
             response_status, response_body_bytes, captured_at) \
         VALUES ($1::UUID, $2::UUID, 'https://example.com/', 'https://example.com/', 200, 0, now()) \
         RETURNING id::TEXT",
    )
    .bind(&snapshot_attempt_id)
    .bind(&domain_id)
    .fetch_one(&pool)
    .await?;
    assert!(
        query(
            "UPDATE crawl_snapshots SET final_url = 'https://changed.example/' WHERE id = $1::UUID"
        )
        .bind(&snapshot_id)
        .execute(&pool)
        .await
        .is_err()
    );
    query(
        "INSERT INTO crawl_country_observations \
         (crawl_snapshot_id, country_code, source, source_version, observed_at) \
         VALUES ($1::UUID, 'US', 'test-geolite', '2026-08-03', now())",
    )
    .bind(&snapshot_id)
    .execute(&pool)
    .await?;
    assert!(
        query("UPDATE crawl_country_observations SET country_code = 'GB' WHERE crawl_snapshot_id = $1::UUID")
            .bind(&snapshot_id)
            .execute(&pool)
            .await
            .is_err()
    );
    let artifact_id: String = query_scalar(
        "INSERT INTO raw_artifacts (crawl_snapshot_id, kind, storage_location, checksum_sha256, \
             compression, uncompressed_size_bytes, compressed_size_bytes, retention_expires_at, captured_at) \
         VALUES ($1::UUID, 'response_body', 'sha256/aa/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.zst', \
             'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'zstd', 1, 10, \
             now() + INTERVAL '30 days', now()) \
         RETURNING id::TEXT",
    )
    .bind(&snapshot_id)
    .fetch_one(&pool)
    .await?;
    assert!(
        query("UPDATE raw_artifacts SET retention_state = 'expired' WHERE id = $1::UUID")
            .bind(&artifact_id)
            .execute(&pool)
            .await
            .is_err()
    );

    query(
        "INSERT INTO domain_source_domains (domain_id, source_id, first_import_id) \
         VALUES ($1::UUID, $2::UUID, $3::UUID)",
    )
    .bind(&domain_id)
    .bind(&source_id)
    .bind(&import_id)
    .execute(&pool)
    .await?;

    query("INSERT INTO crawl_policies (domain_id, desired_interval) VALUES ($1::UUID, INTERVAL '7 days')")
        .bind(&domain_id)
        .execute(&pool)
        .await?;
    assert!(query(
        "INSERT INTO crawl_policies (domain_id, desired_interval) VALUES ($1::UUID, INTERVAL '7 days')",
    )
    .bind(&domain_id)
    .execute(&pool)
    .await
    .is_err());
    query("UPDATE crawl_policies SET last_successful_crawl_at = now() WHERE domain_id = $1::UUID")
        .bind(&domain_id)
        .execute(&pool)
        .await?;
    let last_successful_crawl_at: Option<OffsetDateTime> = query_scalar(
        "SELECT last_successful_crawl_at FROM crawl_policies WHERE domain_id = $1::UUID",
    )
    .bind(&domain_id)
    .fetch_one(&pool)
    .await?;
    assert!(last_successful_crawl_at.is_some());
    let country_recrawl_audit_id: String = query_scalar(
        "INSERT INTO admin_audit_events (actor_subject, action, resource_kind, resource_id) \
         VALUES ('migration-test', 'country_enrichment.recrawl_requested', 'country_enrichment', $1::UUID) \
         RETURNING id::TEXT",
    )
    .bind(uuid::Uuid::nil())
    .fetch_one(&pool)
    .await?;
    assert!(!country_recrawl_audit_id.is_empty());

    let attempt_id: String = query_scalar(
        "INSERT INTO crawl_attempts (domain_id, job_id, correlation_id, idempotency_key) \
         VALUES ($1::UUID, gen_random_uuid(), gen_random_uuid(), 'migration-test-attempt') \
         RETURNING id::TEXT",
    )
    .bind(&domain_id)
    .fetch_one(&pool)
    .await?;

    query("UPDATE crawl_attempts SET status = 'running', started_at = now() WHERE id = $1::UUID")
        .bind(&attempt_id)
        .execute(&pool)
        .await?;
    query(
        "UPDATE crawl_attempts \
         SET status = 'failed', finished_at = now(), failure_code = 'network_timeout' \
         WHERE id = $1::UUID",
    )
    .bind(&attempt_id)
    .execute(&pool)
    .await?;
    assert!(
        query("UPDATE crawl_attempts SET failure_summary = 'changed' WHERE id = $1::UUID")
            .bind(&attempt_id)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        query(
            "INSERT INTO crawl_attempts (domain_id, job_id, correlation_id, idempotency_key) \
         VALUES ($1::UUID, gen_random_uuid(), gen_random_uuid(), 'migration-test-attempt')",
        )
        .bind(&domain_id)
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        expired_raw_artifact_locations(&pool, OffsetDateTime::now_utc())
            .await?
            .is_empty()
    );

    let seeded_technology_count: i64 = query_scalar("SELECT COUNT(*) FROM technologies")
        .fetch_one(&pool)
        .await?;
    assert_eq!(seeded_technology_count, 8);
    let active_rule_count: i64 =
        query_scalar("SELECT COUNT(*) FROM detection_rules WHERE active_version_id IS NOT NULL")
            .fetch_one(&pool)
            .await?;
    assert_eq!(active_rule_count, 8);
    let wordpress_definition: serde_json::Value = query_scalar(
        "SELECT versions.definition \
         FROM detection_rules AS rules \
         JOIN detection_rule_versions AS versions ON versions.id = rules.active_version_id \
         WHERE rules.slug = 'wordpress-v1'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(wordpress_definition["signals"][0]["source"], "header");
    assert_eq!(wordpress_definition["signals"][0]["key"], "x-powered-by");
    let nextjs_active_version: i16 = query_scalar(
        "SELECT versions.version \
         FROM detection_rules AS rules \
         JOIN detection_rule_versions AS versions ON versions.id = rules.active_version_id \
         WHERE rules.slug = 'nextjs-v1'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(nextjs_active_version, 2);
    let rule_version_id: String = query_scalar(
        "SELECT detection_rule_versions.id::TEXT \
         FROM detection_rule_versions \
         JOIN detection_rules ON detection_rules.id = detection_rule_versions.detection_rule_id \
         WHERE detection_rules.slug = 'nextjs-v1' AND detection_rule_versions.version = 1",
    )
    .fetch_one(&pool)
    .await?;
    let technology_id: String =
        query_scalar("SELECT id::TEXT FROM technologies WHERE slug = 'nextjs'")
            .fetch_one(&pool)
            .await?;
    let detection_id: String = query_scalar(
        "INSERT INTO detections (crawl_snapshot_id, technology_id, detection_rule_version_id, confidence, method, observed_at) \
         VALUES ($1::UUID, $2::UUID, $3::UUID, 90, 'deterministic_rule', now()) RETURNING id::TEXT",
    )
    .bind(&snapshot_id)
    .bind(&technology_id)
    .bind(&rule_version_id)
    .fetch_one(&pool)
    .await?;
    query(
        "INSERT INTO detection_evidence (detection_id, source, evidence_key, evidence_value) \
         VALUES ($1::UUID, 'header', 'x-powered-by', 'Next.js')",
    )
    .bind(&detection_id)
    .execute(&pool)
    .await?;
    assert!(
        query(
            "INSERT INTO detection_evidence (detection_id, source, evidence_key, evidence_value) \
             VALUES ($1::UUID, 'header', 'x-powered-by', 'Next.js')",
        )
        .bind(&detection_id)
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        query("UPDATE detections SET confidence = 80 WHERE id = $1::UUID")
            .bind(&detection_id)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        query("UPDATE detection_rule_versions SET version = 2 WHERE id = $1::UUID")
            .bind(&rule_version_id)
            .execute(&pool)
            .await
            .is_err()
    );
    query(
        "INSERT INTO detection_rule_observations (crawl_snapshot_id, detection_rule_version_id, status, observed_at) \
         VALUES ($1::UUID, $2::UUID, 'detected', now())",
    )
    .bind(&snapshot_id)
    .bind(&rule_version_id)
    .execute(&pool)
    .await?;
    assert!(
        query("UPDATE detection_rule_observations SET status = 'unknown' WHERE crawl_snapshot_id = $1::UUID")
            .bind(&snapshot_id)
            .execute(&pool)
            .await
            .is_err()
    );

    let table_count: i64 = query_scalar(
        "SELECT COUNT(*) \
         FROM information_schema.tables \
         WHERE table_schema = 'public' \
           AND table_name IN ('domains', 'domain_sources', 'imports', 'import_rows', \
                              'domain_source_domains', 'crawl_policies', 'crawl_attempts', 'crawl_job_outbox', 'crawl_snapshots', 'raw_artifacts', 'admin_audit_events', \
                              'technology_categories', 'technologies', 'detection_rules', 'detection_rule_versions', 'detections', 'detection_evidence', \
                              'detection_rule_observations', 'domain_current_technologies', 'technology_changes', \
                              'search_index_outbox', 'search_index_state', 'crawl_country_observations', \
                              'detection_reprocessing_runs', 'detection_reprocessing_work_items', 'reprocessed_detection_observations', \
                              'reprocessed_detections', 'reprocessed_detection_evidence', 'technology_version_changes', 'admin_audit_events')",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(table_count, 29);

    let search_domain_id: uuid::Uuid = query_scalar(
        "INSERT INTO domains (canonical_domain) VALUES ('search-outbox.example') RETURNING id",
    )
    .fetch_one(&pool)
    .await?;
    query("INSERT INTO search_index_outbox (domain_id) VALUES ($1)")
        .bind(search_domain_id)
        .execute(&pool)
        .await?;
    let index_outbox = PostgresSearchIndexRepository::new(pool.clone());
    let now = OffsetDateTime::now_utc();
    let jobs = index_outbox
        .claim_ready(now, Duration::from_secs(10), 1)
        .await?;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].domain_id, search_domain_id);
    index_outbox
        .fail(jobs[0], now, Duration::from_secs(1), 1, "fixture failure")
        .await?;
    assert_eq!(index_outbox.lag(now).await?.dead_count, 1);

    let import_row =
        query("SELECT canonical_domain, status FROM import_rows WHERE import_id = $1::UUID")
            .bind(&import_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        import_row.try_get::<String, _>("canonical_domain")?,
        "example.com"
    );
    assert_eq!(import_row.try_get::<String, _>("status")?, "accepted");

    pool.close().await;
    Ok(())
}
