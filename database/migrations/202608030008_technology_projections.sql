CREATE TABLE detection_rule_observations (
    crawl_snapshot_id UUID NOT NULL REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    detection_rule_version_id UUID NOT NULL REFERENCES detection_rule_versions (id) ON DELETE RESTRICT,
    status TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (crawl_snapshot_id, detection_rule_version_id),
    CONSTRAINT detection_rule_observations_status_valid
        CHECK (status IN ('detected', 'confirmed_absent', 'unknown')),
    CONSTRAINT detection_rule_observations_observed_at_valid
        CHECK (observed_at >= created_at - INTERVAL '1 day')
);

CREATE TABLE domain_current_technologies (
    domain_id UUID NOT NULL REFERENCES domains (id) ON DELETE RESTRICT,
    technology_id UUID NOT NULL REFERENCES technologies (id) ON DELETE RESTRICT,
    current_detection_id UUID NOT NULL REFERENCES detections (id) ON DELETE RESTRICT,
    first_snapshot_id UUID NOT NULL REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    last_snapshot_id UUID NOT NULL REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    first_observed_at TIMESTAMPTZ NOT NULL,
    last_observed_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (domain_id, technology_id),
    CONSTRAINT domain_current_technologies_observed_at_valid
        CHECK (last_observed_at >= first_observed_at)
);

CREATE INDEX domain_current_technologies_technology_idx
    ON domain_current_technologies (technology_id, last_observed_at DESC);

CREATE TABLE technology_changes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domain_id UUID NOT NULL REFERENCES domains (id) ON DELETE RESTRICT,
    change_kind TEXT NOT NULL,
    category_id UUID NOT NULL REFERENCES technology_categories (id) ON DELETE RESTRICT,
    prior_snapshot_id UUID REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    current_snapshot_id UUID NOT NULL REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    prior_detection_id UUID REFERENCES detections (id) ON DELETE RESTRICT,
    current_detection_id UUID REFERENCES detections (id) ON DELETE RESTRICT,
    from_technology_id UUID REFERENCES technologies (id) ON DELETE RESTRICT,
    to_technology_id UUID REFERENCES technologies (id) ON DELETE RESTRICT,
    observed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT technology_changes_kind_valid CHECK (change_kind IN ('added', 'removed', 'migrated')),
    CONSTRAINT technology_changes_observed_at_valid CHECK (observed_at >= created_at - INTERVAL '1 day'),
    CONSTRAINT technology_changes_shape_valid CHECK (
        (change_kind = 'added' AND prior_snapshot_id IS NULL AND prior_detection_id IS NULL
            AND from_technology_id IS NULL AND current_detection_id IS NOT NULL AND to_technology_id IS NOT NULL)
        OR (change_kind = 'removed' AND prior_snapshot_id IS NOT NULL AND prior_detection_id IS NOT NULL
            AND from_technology_id IS NOT NULL AND current_detection_id IS NULL AND to_technology_id IS NULL)
        OR (change_kind = 'migrated' AND prior_snapshot_id IS NOT NULL AND prior_detection_id IS NOT NULL
            AND from_technology_id IS NOT NULL AND current_detection_id IS NOT NULL AND to_technology_id IS NOT NULL)
    )
);

CREATE INDEX technology_changes_domain_observed_idx
    ON technology_changes (domain_id, observed_at DESC);

CREATE TRIGGER detection_rule_observations_prevent_mutation
    BEFORE UPDATE OR DELETE ON detection_rule_observations
    FOR EACH ROW
    EXECUTE FUNCTION prevent_detection_catalogue_mutation();

CREATE TRIGGER technology_changes_prevent_mutation
    BEFORE UPDATE OR DELETE ON technology_changes
    FOR EACH ROW
    EXECUTE FUNCTION prevent_detection_catalogue_mutation();
