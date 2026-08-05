CREATE TABLE detection_reprocessing_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    detection_rule_version_id UUID NOT NULL REFERENCES detection_rule_versions (id) ON DELETE RESTRICT,
    requested_by TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    correlation_id UUID NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    total_snapshot_count INTEGER NOT NULL DEFAULT 0,
    succeeded_snapshot_count INTEGER NOT NULL DEFAULT 0,
    failed_snapshot_count INTEGER NOT NULL DEFAULT 0,
    requested_at TIMESTAMPTZ NOT NULL,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    CONSTRAINT detection_reprocessing_runs_requester_present CHECK (char_length(btrim(requested_by)) > 0),
    CONSTRAINT detection_reprocessing_runs_key_present CHECK (char_length(btrim(idempotency_key)) BETWEEN 1 AND 255),
    CONSTRAINT detection_reprocessing_runs_status_valid CHECK (status IN ('queued', 'running', 'completed', 'partial_failed')),
    CONSTRAINT detection_reprocessing_runs_counts_valid CHECK (
        total_snapshot_count >= 0 AND succeeded_snapshot_count >= 0 AND failed_snapshot_count >= 0
        AND succeeded_snapshot_count + failed_snapshot_count <= total_snapshot_count
    ),
    CONSTRAINT detection_reprocessing_runs_terminal_valid CHECK (
        (status IN ('queued', 'running') AND finished_at IS NULL)
        OR (status IN ('completed', 'partial_failed') AND finished_at IS NOT NULL)
    )
);

CREATE TABLE detection_reprocessing_work_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES detection_reprocessing_runs (id) ON DELETE RESTRICT,
    crawl_snapshot_id UUID NOT NULL REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    domain_id UUID NOT NULL REFERENCES domains (id) ON DELETE RESTRICT,
    status TEXT NOT NULL DEFAULT 'queued',
    attempt_count SMALLINT NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL,
    claimed_at TIMESTAMPTZ,
    failure_summary TEXT,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, crawl_snapshot_id),
    CONSTRAINT detection_reprocessing_work_items_status_valid CHECK (status IN ('queued', 'running', 'succeeded', 'dead_lettered')),
    CONSTRAINT detection_reprocessing_work_items_attempt_valid CHECK (attempt_count >= 0),
    CONSTRAINT detection_reprocessing_work_items_failure_bounded CHECK (failure_summary IS NULL OR char_length(failure_summary) <= 1024),
    CONSTRAINT detection_reprocessing_work_items_terminal_valid CHECK (
        (status IN ('queued', 'running') AND completed_at IS NULL)
        OR (status = 'succeeded' AND completed_at IS NOT NULL AND failure_summary IS NULL)
        OR (status = 'dead_lettered' AND completed_at IS NOT NULL AND failure_summary IS NOT NULL)
    )
);

CREATE INDEX detection_reprocessing_work_items_pending_idx
    ON detection_reprocessing_work_items (status, next_attempt_at)
    WHERE status = 'queued';
CREATE INDEX detection_reprocessing_work_items_run_idx
    ON detection_reprocessing_work_items (run_id, status);

CREATE TABLE reprocessed_detection_observations (
    run_id UUID NOT NULL REFERENCES detection_reprocessing_runs (id) ON DELETE RESTRICT,
    crawl_snapshot_id UUID NOT NULL REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    detection_rule_version_id UUID NOT NULL REFERENCES detection_rule_versions (id) ON DELETE RESTRICT,
    status TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, crawl_snapshot_id, detection_rule_version_id),
    CONSTRAINT reprocessed_detection_observations_status_valid CHECK (status IN ('detected', 'confirmed_absent', 'unknown'))
);

CREATE TABLE reprocessed_detections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES detection_reprocessing_runs (id) ON DELETE RESTRICT,
    crawl_snapshot_id UUID NOT NULL REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    technology_id UUID NOT NULL REFERENCES technologies (id) ON DELETE RESTRICT,
    detection_rule_version_id UUID NOT NULL REFERENCES detection_rule_versions (id) ON DELETE RESTRICT,
    confidence SMALLINT NOT NULL,
    method TEXT NOT NULL,
    technology_version TEXT,
    observed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, crawl_snapshot_id, technology_id, detection_rule_version_id),
    CONSTRAINT reprocessed_detections_confidence_valid CHECK (confidence BETWEEN 0 AND 100),
    CONSTRAINT reprocessed_detections_method_valid CHECK (method = 'deterministic_rule'),
    CONSTRAINT reprocessed_detections_version_valid CHECK (technology_version IS NULL OR char_length(technology_version) BETWEEN 1 AND 128)
);

CREATE TABLE reprocessed_detection_evidence (
    reprocessed_detection_id UUID NOT NULL REFERENCES reprocessed_detections (id) ON DELETE RESTRICT,
    source TEXT NOT NULL,
    evidence_key TEXT NOT NULL,
    evidence_value TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (reprocessed_detection_id, source, evidence_key, evidence_value),
    CONSTRAINT reprocessed_detection_evidence_source_valid CHECK (source IN ('html', 'header', 'script', 'dns', 'tls')),
    CONSTRAINT reprocessed_detection_evidence_key_valid CHECK (char_length(btrim(evidence_key)) BETWEEN 1 AND 128),
    CONSTRAINT reprocessed_detection_evidence_value_valid CHECK (char_length(btrim(evidence_value)) BETWEEN 1 AND 2048)
);

CREATE TABLE technology_version_changes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reprocessing_run_id UUID NOT NULL REFERENCES detection_reprocessing_runs (id) ON DELETE RESTRICT,
    domain_id UUID NOT NULL REFERENCES domains (id) ON DELETE RESTRICT,
    technology_id UUID NOT NULL REFERENCES technologies (id) ON DELETE RESTRICT,
    prior_snapshot_id UUID NOT NULL REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    current_snapshot_id UUID NOT NULL REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    prior_reprocessed_detection_id UUID NOT NULL REFERENCES reprocessed_detections (id) ON DELETE RESTRICT,
    current_reprocessed_detection_id UUID NOT NULL REFERENCES reprocessed_detections (id) ON DELETE RESTRICT,
    from_version TEXT NOT NULL,
    to_version TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (reprocessing_run_id, current_reprocessed_detection_id),
    CONSTRAINT technology_version_changes_versions_differ CHECK (from_version <> to_version)
);

CREATE INDEX technology_version_changes_domain_observed_idx
    ON technology_version_changes (domain_id, observed_at DESC);

CREATE FUNCTION prevent_reprocessed_detection_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'reprocessed detection records are immutable' USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER reprocessed_detection_observations_prevent_mutation
    BEFORE UPDATE OR DELETE ON reprocessed_detection_observations
    FOR EACH ROW EXECUTE FUNCTION prevent_reprocessed_detection_mutation();
CREATE TRIGGER reprocessed_detections_prevent_mutation
    BEFORE UPDATE OR DELETE ON reprocessed_detections
    FOR EACH ROW EXECUTE FUNCTION prevent_reprocessed_detection_mutation();
CREATE TRIGGER reprocessed_detection_evidence_prevent_mutation
    BEFORE UPDATE OR DELETE ON reprocessed_detection_evidence
    FOR EACH ROW EXECUTE FUNCTION prevent_reprocessed_detection_mutation();
CREATE TRIGGER technology_version_changes_prevent_mutation
    BEFORE UPDATE OR DELETE ON technology_version_changes
    FOR EACH ROW EXECUTE FUNCTION prevent_reprocessed_detection_mutation();
