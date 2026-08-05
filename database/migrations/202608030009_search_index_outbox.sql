CREATE TABLE search_index_outbox (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domain_id UUID NOT NULL UNIQUE REFERENCES domains (id) ON DELETE RESTRICT,
    status TEXT NOT NULL DEFAULT 'pending',
    attempt_count SMALLINT NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    claimed_at TIMESTAMPTZ,
    last_error TEXT,
    last_indexed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT search_index_outbox_status_valid CHECK (status IN ('pending', 'processing', 'retry', 'dead')),
    CONSTRAINT search_index_outbox_attempt_count_nonnegative CHECK (attempt_count >= 0),
    CONSTRAINT search_index_outbox_error_bounded CHECK (last_error IS NULL OR char_length(last_error) <= 1024)
);

CREATE INDEX search_index_outbox_ready_idx
    ON search_index_outbox (next_attempt_at, created_at)
    WHERE status IN ('pending', 'retry', 'processing');

CREATE TABLE search_index_state (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    last_success_at TIMESTAMPTZ,
    last_failure_at TIMESTAMPTZ,
    last_failure_summary TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT search_index_state_failure_bounded
        CHECK (last_failure_summary IS NULL OR char_length(last_failure_summary) <= 1024)
);

INSERT INTO search_index_state (singleton) VALUES (TRUE);
