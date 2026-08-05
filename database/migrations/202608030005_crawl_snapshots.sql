CREATE TABLE crawl_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    crawl_attempt_id UUID NOT NULL UNIQUE REFERENCES crawl_attempts (id) ON DELETE RESTRICT,
    domain_id UUID NOT NULL REFERENCES domains (id) ON DELETE RESTRICT,
    requested_url TEXT NOT NULL,
    final_url TEXT NOT NULL,
    redirect_chain JSONB NOT NULL DEFAULT '[]'::JSONB,
    response_status SMALLINT NOT NULL,
    response_headers JSONB NOT NULL DEFAULT '{}'::JSONB,
    response_body_bytes BIGINT NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT crawl_snapshots_requested_url_present CHECK (char_length(btrim(requested_url)) > 0),
    CONSTRAINT crawl_snapshots_final_url_present CHECK (char_length(btrim(final_url)) > 0),
    CONSTRAINT crawl_snapshots_http_status_valid CHECK (response_status BETWEEN 100 AND 599),
    CONSTRAINT crawl_snapshots_body_bytes_nonnegative CHECK (response_body_bytes >= 0),
    CONSTRAINT crawl_snapshots_capture_time_valid CHECK (captured_at >= created_at - INTERVAL '1 day')
);

CREATE INDEX crawl_snapshots_domain_captured_at_idx
    ON crawl_snapshots (domain_id, captured_at DESC);

CREATE FUNCTION require_successful_crawl_attempt_for_snapshot()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM crawl_attempts
        WHERE id = NEW.crawl_attempt_id AND status = 'succeeded'
    ) THEN
        RAISE EXCEPTION 'crawl snapshots require a succeeded crawl attempt'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER crawl_snapshots_require_succeeded_attempt
    BEFORE INSERT ON crawl_snapshots
    FOR EACH ROW
    EXECUTE FUNCTION require_successful_crawl_attempt_for_snapshot();

CREATE FUNCTION prevent_crawl_snapshot_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'crawl snapshots are immutable'
        USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER crawl_snapshots_prevent_mutation
    BEFORE UPDATE OR DELETE ON crawl_snapshots
    FOR EACH ROW
    EXECUTE FUNCTION prevent_crawl_snapshot_mutation();
