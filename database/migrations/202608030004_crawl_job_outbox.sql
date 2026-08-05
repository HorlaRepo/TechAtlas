CREATE TABLE crawl_job_outbox (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    crawl_attempt_id UUID NOT NULL UNIQUE REFERENCES crawl_attempts (id) ON DELETE RESTRICT,
    domain_id UUID NOT NULL REFERENCES domains (id) ON DELETE RESTRICT,
    job_id UUID NOT NULL UNIQUE,
    correlation_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    available_at TIMESTAMPTZ NOT NULL,
    next_publish_at TIMESTAMPTZ NOT NULL,
    publish_attempt_count SMALLINT NOT NULL DEFAULT 0,
    published_at TIMESTAMPTZ,
    terminal_failure_at TIMESTAMPTZ,
    last_error_summary TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT crawl_job_outbox_idempotency_key_present
        CHECK (char_length(btrim(idempotency_key)) > 0),
    CONSTRAINT crawl_job_outbox_publish_attempt_count_nonnegative
        CHECK (publish_attempt_count >= 0),
    CONSTRAINT crawl_job_outbox_timestamps_valid
        CHECK (
            available_at >= created_at
            AND next_publish_at >= created_at
            AND (published_at IS NULL OR published_at >= created_at)
            AND (terminal_failure_at IS NULL OR terminal_failure_at >= created_at)
        ),
    CONSTRAINT crawl_job_outbox_terminal_state_valid
        CHECK (published_at IS NULL OR terminal_failure_at IS NULL),
    CONSTRAINT crawl_job_outbox_error_summary_bounded
        CHECK (last_error_summary IS NULL OR char_length(last_error_summary) <= 1024)
);

CREATE INDEX crawl_job_outbox_dispatch_idx
    ON crawl_job_outbox (next_publish_at, available_at)
    WHERE published_at IS NULL AND terminal_failure_at IS NULL;
