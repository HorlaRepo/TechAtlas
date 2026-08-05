CREATE TABLE crawl_dns_observations (
    crawl_snapshot_id UUID PRIMARY KEY REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    source TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    queried_name TEXT,
    addresses JSONB NOT NULL DEFAULT '[]'::JSONB,
    unavailable_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT crawl_dns_observations_source_present CHECK (char_length(btrim(source)) > 0),
    CONSTRAINT crawl_dns_observations_addresses_array CHECK (jsonb_typeof(addresses) = 'array'),
    CONSTRAINT crawl_dns_observations_reason_valid CHECK (
        unavailable_reason IS NULL
        OR unavailable_reason IN ('not_captured', 'not_applicable', 'timeout', 'connection_failed', 'invalid')
    ),
    CONSTRAINT crawl_dns_observations_availability_valid CHECK (
        (unavailable_reason IS NULL AND queried_name IS NOT NULL)
        OR (unavailable_reason IS NOT NULL AND queried_name IS NULL AND addresses = '[]'::JSONB)
    )
);

CREATE TABLE crawl_tls_observations (
    crawl_snapshot_id UUID PRIMARY KEY REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    source TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    unavailable_reason TEXT,
    validation_status TEXT,
    protocol TEXT,
    cipher_suite TEXT,
    certificate_subject TEXT,
    certificate_issuer TEXT,
    subject_alternative_names JSONB NOT NULL DEFAULT '[]'::JSONB,
    certificate_not_before TIMESTAMPTZ,
    certificate_not_after TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT crawl_tls_observations_source_present CHECK (char_length(btrim(source)) > 0),
    CONSTRAINT crawl_tls_observations_names_array CHECK (jsonb_typeof(subject_alternative_names) = 'array'),
    CONSTRAINT crawl_tls_observations_reason_valid CHECK (
        unavailable_reason IS NULL
        OR unavailable_reason IN ('not_captured', 'not_applicable', 'timeout', 'connection_failed', 'invalid')
    ),
    CONSTRAINT crawl_tls_observations_validation_valid CHECK (
        validation_status IS NULL OR validation_status IN ('verified', 'validation_failed')
    ),
    CONSTRAINT crawl_tls_observations_availability_valid CHECK (
        (unavailable_reason IS NULL AND validation_status IS NOT NULL)
        OR (
            unavailable_reason IS NOT NULL
            AND validation_status IS NULL
            AND protocol IS NULL
            AND cipher_suite IS NULL
            AND certificate_subject IS NULL
            AND certificate_issuer IS NULL
            AND subject_alternative_names = '[]'::JSONB
            AND certificate_not_before IS NULL
            AND certificate_not_after IS NULL
        )
    )
);

CREATE FUNCTION prevent_crawl_dns_tls_observation_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'crawl DNS and TLS observations are immutable'
        USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER crawl_dns_observations_prevent_mutation
    BEFORE UPDATE OR DELETE ON crawl_dns_observations
    FOR EACH ROW
    EXECUTE FUNCTION prevent_crawl_dns_tls_observation_mutation();

CREATE TRIGGER crawl_tls_observations_prevent_mutation
    BEFORE UPDATE OR DELETE ON crawl_tls_observations
    FOR EACH ROW
    EXECUTE FUNCTION prevent_crawl_dns_tls_observation_mutation();
