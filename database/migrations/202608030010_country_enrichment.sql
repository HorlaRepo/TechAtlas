CREATE TABLE crawl_country_observations (
    crawl_snapshot_id UUID PRIMARY KEY REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    country_code CHAR(2) NOT NULL,
    source TEXT NOT NULL,
    source_version TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT crawl_country_observations_country_code_valid
        CHECK (country_code ~ '^[A-Z]{2}$'),
    CONSTRAINT crawl_country_observations_source_present
        CHECK (char_length(btrim(source)) > 0),
    CONSTRAINT crawl_country_observations_source_version_present
        CHECK (char_length(btrim(source_version)) > 0)
);

CREATE INDEX crawl_country_observations_country_idx
    ON crawl_country_observations (country_code, observed_at DESC);

CREATE FUNCTION prevent_crawl_country_observation_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'crawl country observations are immutable'
        USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER crawl_country_observations_prevent_mutation
    BEFORE UPDATE OR DELETE ON crawl_country_observations
    FOR EACH ROW
    EXECUTE FUNCTION prevent_crawl_country_observation_mutation();
