CREATE TABLE raw_artifacts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    crawl_snapshot_id UUID NOT NULL UNIQUE REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    kind TEXT NOT NULL,
    storage_location TEXT NOT NULL,
    checksum_sha256 CHAR(64) NOT NULL,
    compression TEXT NOT NULL,
    uncompressed_size_bytes BIGINT NOT NULL,
    compressed_size_bytes BIGINT NOT NULL,
    retention_state TEXT NOT NULL DEFAULT 'active',
    retention_expires_at TIMESTAMPTZ NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT raw_artifacts_kind_valid CHECK (kind = 'response_body'),
    CONSTRAINT raw_artifacts_location_present CHECK (char_length(btrim(storage_location)) > 0),
    CONSTRAINT raw_artifacts_checksum_valid CHECK (checksum_sha256 ~ '^[0-9a-f]{64}$'),
    CONSTRAINT raw_artifacts_compression_valid CHECK (compression = 'zstd'),
    CONSTRAINT raw_artifacts_sizes_valid CHECK (
        uncompressed_size_bytes >= 0
        AND compressed_size_bytes >= 0
        AND (uncompressed_size_bytes = 0 OR compressed_size_bytes > 0)
    ),
    CONSTRAINT raw_artifacts_retention_state_valid CHECK (retention_state = 'active'),
    CONSTRAINT raw_artifacts_retention_expiry_valid CHECK (retention_expires_at >= captured_at)
);

CREATE INDEX raw_artifacts_retention_expiry_idx
    ON raw_artifacts (retention_expires_at)
    WHERE retention_state = 'active';

CREATE FUNCTION prevent_raw_artifact_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'raw artifacts are immutable'
        USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER raw_artifacts_prevent_mutation
    BEFORE UPDATE OR DELETE ON raw_artifacts
    FOR EACH ROW
    EXECUTE FUNCTION prevent_raw_artifact_mutation();
