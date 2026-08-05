ALTER TABLE crawl_policies
    ADD COLUMN last_successful_crawl_at TIMESTAMPTZ,
    ADD CONSTRAINT crawl_policies_last_successful_crawl_valid
        CHECK (last_successful_crawl_at IS NULL OR last_successful_crawl_at >= created_at);

UPDATE crawl_policies AS policies
SET last_successful_crawl_at = snapshots.captured_at
FROM (
    SELECT DISTINCT ON (domain_id) domain_id, captured_at
    FROM crawl_snapshots
    ORDER BY domain_id, captured_at DESC
) AS snapshots
WHERE snapshots.domain_id = policies.domain_id;

INSERT INTO search_index_outbox (domain_id, status, attempt_count, next_attempt_at, created_at, updated_at)
SELECT policies.domain_id, 'pending', 0, now(), now(), now()
FROM crawl_policies AS policies
JOIN domains ON domains.id = policies.domain_id
WHERE domains.archived_at IS NULL
ON CONFLICT (domain_id) DO UPDATE SET
    status = 'pending',
    attempt_count = 0,
    next_attempt_at = EXCLUDED.next_attempt_at,
    claimed_at = NULL,
    last_error = NULL,
    updated_at = EXCLUDED.updated_at;

ALTER TABLE admin_audit_events
    DROP CONSTRAINT admin_audit_events_resource_kind_valid,
    ADD CONSTRAINT admin_audit_events_resource_kind_valid
        CHECK (resource_kind IN ('domain', 'import', 'crawl_attempt', 'detection_rule', 'country_enrichment'));
