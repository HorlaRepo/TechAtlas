CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE domains (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    canonical_domain TEXT NOT NULL,
    archived_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT domains_canonical_domain_normalized
        CHECK (
            canonical_domain = lower(canonical_domain)
            AND canonical_domain = btrim(canonical_domain)
            AND char_length(canonical_domain) > 0
        )
);

CREATE UNIQUE INDEX domains_active_canonical_domain_key
    ON domains (canonical_domain)
    WHERE archived_at IS NULL;

CREATE TABLE domain_sources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    source_url TEXT,
    archived_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT domain_sources_kind_valid
        CHECK (kind IN ('manual', 'csv', 'public_list')),
    CONSTRAINT domain_sources_name_present
        CHECK (char_length(btrim(name)) > 0)
);

CREATE TABLE imports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id UUID NOT NULL REFERENCES domain_sources (id) ON DELETE RESTRICT,
    initiated_by TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    accepted_row_count INTEGER NOT NULL DEFAULT 0,
    duplicate_row_count INTEGER NOT NULL DEFAULT 0,
    rejected_row_count INTEGER NOT NULL DEFAULT 0,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT imports_initiated_by_present
        CHECK (char_length(btrim(initiated_by)) > 0),
    CONSTRAINT imports_status_valid
        CHECK (status IN ('pending', 'processing', 'completed', 'failed')),
    CONSTRAINT imports_row_counts_nonnegative
        CHECK (
            accepted_row_count >= 0
            AND duplicate_row_count >= 0
            AND rejected_row_count >= 0
        ),
    CONSTRAINT imports_timestamps_valid
        CHECK (
            started_at IS NULL
            OR started_at >= created_at
        ),
    CONSTRAINT imports_completion_valid
        CHECK (
            completed_at IS NULL
            OR (started_at IS NOT NULL AND completed_at >= started_at)
        )
);

CREATE INDEX imports_source_created_at_idx ON imports (source_id, created_at DESC);

CREATE TABLE import_rows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    import_id UUID NOT NULL REFERENCES imports (id) ON DELETE RESTRICT,
    row_number INTEGER NOT NULL,
    input_values JSONB NOT NULL DEFAULT '{}'::JSONB,
    canonical_domain TEXT,
    domain_id UUID REFERENCES domains (id) ON DELETE RESTRICT,
    status TEXT NOT NULL DEFAULT 'pending',
    error_code TEXT,
    error_summary TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT import_rows_row_number_positive CHECK (row_number > 0),
    CONSTRAINT import_rows_canonical_domain_normalized
        CHECK (
            canonical_domain IS NULL
            OR (
                canonical_domain = lower(canonical_domain)
                AND canonical_domain = btrim(canonical_domain)
                AND char_length(canonical_domain) > 0
            )
        ),
    CONSTRAINT import_rows_status_valid
        CHECK (status IN ('pending', 'accepted', 'duplicate', 'rejected')),
    CONSTRAINT import_rows_outcome_valid
        CHECK (
            (status = 'pending' AND domain_id IS NULL AND error_code IS NULL)
            OR (status IN ('accepted', 'duplicate') AND domain_id IS NOT NULL AND error_code IS NULL)
            OR (status = 'rejected' AND domain_id IS NULL AND error_code IS NOT NULL)
        ),
    CONSTRAINT import_rows_error_summary_bounded
        CHECK (error_summary IS NULL OR char_length(error_summary) <= 1024),
    CONSTRAINT import_rows_import_row_number_key UNIQUE (import_id, row_number)
);

CREATE INDEX import_rows_domain_id_idx ON import_rows (domain_id) WHERE domain_id IS NOT NULL;

CREATE TABLE domain_source_domains (
    domain_id UUID NOT NULL REFERENCES domains (id) ON DELETE RESTRICT,
    source_id UUID NOT NULL REFERENCES domain_sources (id) ON DELETE RESTRICT,
    first_import_id UUID REFERENCES imports (id) ON DELETE RESTRICT,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (domain_id, source_id),
    CONSTRAINT domain_source_domains_seen_at_valid CHECK (last_seen_at >= first_seen_at)
);

CREATE INDEX domain_source_domains_source_id_idx ON domain_source_domains (source_id, domain_id);

CREATE TABLE crawl_policies (
    domain_id UUID PRIMARY KEY REFERENCES domains (id) ON DELETE RESTRICT,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    priority TEXT NOT NULL DEFAULT 'medium',
    desired_interval INTERVAL NOT NULL,
    last_crawl_at TIMESTAMPTZ,
    next_crawl_at TIMESTAMPTZ,
    consecutive_failure_count INTEGER NOT NULL DEFAULT 0,
    last_failure_at TIMESTAMPTZ,
    terminal_failure_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT crawl_policies_priority_valid CHECK (priority IN ('high', 'medium', 'low')),
    CONSTRAINT crawl_policies_interval_positive CHECK (desired_interval > INTERVAL '0'),
    CONSTRAINT crawl_policies_failure_count_nonnegative CHECK (consecutive_failure_count >= 0),
    CONSTRAINT crawl_policies_last_crawl_valid
        CHECK (last_crawl_at IS NULL OR last_crawl_at >= created_at),
    CONSTRAINT crawl_policies_next_crawl_valid
        CHECK (next_crawl_at IS NULL OR next_crawl_at >= created_at),
    CONSTRAINT crawl_policies_last_failure_valid
        CHECK (last_failure_at IS NULL OR last_failure_at >= created_at),
    CONSTRAINT crawl_policies_terminal_failure_valid
        CHECK (terminal_failure_at IS NULL OR terminal_failure_at >= created_at)
);

CREATE INDEX crawl_policies_eligible_next_crawl_idx
    ON crawl_policies (next_crawl_at)
    WHERE is_enabled;

CREATE TABLE crawl_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domain_id UUID NOT NULL REFERENCES domains (id) ON DELETE RESTRICT,
    job_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL,
    attempt_number SMALLINT NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'queued',
    queued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    failure_code TEXT,
    failure_summary TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT crawl_attempts_idempotency_key_present
        CHECK (char_length(btrim(idempotency_key)) > 0),
    CONSTRAINT crawl_attempts_number_positive CHECK (attempt_number > 0),
    CONSTRAINT crawl_attempts_status_valid
        CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    CONSTRAINT crawl_attempts_queued_at_valid CHECK (queued_at >= created_at),
    CONSTRAINT crawl_attempts_timestamps_valid
        CHECK (
            started_at IS NULL
            OR started_at >= queued_at
        ),
    CONSTRAINT crawl_attempts_finished_at_valid
        CHECK (
            finished_at IS NULL
            OR (started_at IS NOT NULL AND finished_at >= started_at)
            OR (status = 'cancelled' AND finished_at >= queued_at)
        ),
    CONSTRAINT crawl_attempts_lifecycle_valid
        CHECK (
            (status = 'queued' AND started_at IS NULL AND finished_at IS NULL AND failure_code IS NULL)
            OR (status = 'running' AND started_at IS NOT NULL AND finished_at IS NULL AND failure_code IS NULL)
            OR (status = 'succeeded' AND started_at IS NOT NULL AND finished_at IS NOT NULL AND failure_code IS NULL)
            OR (status = 'failed' AND started_at IS NOT NULL AND finished_at IS NOT NULL AND failure_code IS NOT NULL)
            OR (status = 'cancelled' AND finished_at IS NOT NULL)
        ),
    CONSTRAINT crawl_attempts_failure_summary_bounded
        CHECK (failure_summary IS NULL OR char_length(failure_summary) <= 1024),
    CONSTRAINT crawl_attempts_idempotency_key_unique UNIQUE (idempotency_key)
);

CREATE INDEX crawl_attempts_domain_queued_at_idx ON crawl_attempts (domain_id, queued_at DESC);
CREATE INDEX crawl_attempts_correlation_id_idx ON crawl_attempts (correlation_id);

CREATE FUNCTION prevent_terminal_crawl_attempt_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status IN ('succeeded', 'failed', 'cancelled') THEN
        RAISE EXCEPTION 'terminal crawl attempts are immutable'
            USING ERRCODE = '55000';
    END IF;

    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER crawl_attempts_prevent_terminal_mutation
    BEFORE UPDATE OR DELETE ON crawl_attempts
    FOR EACH ROW
    EXECUTE FUNCTION prevent_terminal_crawl_attempt_mutation();
