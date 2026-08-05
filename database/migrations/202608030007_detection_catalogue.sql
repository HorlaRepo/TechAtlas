CREATE TABLE technology_categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT technology_categories_slug_valid
        CHECK (slug ~ '^[a-z0-9]+(?:-[a-z0-9]+)*$'),
    CONSTRAINT technology_categories_name_present
        CHECK (char_length(btrim(display_name)) > 0)
);

CREATE TABLE technologies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    category_id UUID NOT NULL REFERENCES technology_categories (id) ON DELETE RESTRICT,
    slug TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT technologies_slug_valid
        CHECK (slug ~ '^[a-z0-9]+(?:-[a-z0-9]+)*$'),
    CONSTRAINT technologies_name_present
        CHECK (char_length(btrim(display_name)) > 0)
);

CREATE TABLE detection_rules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    technology_id UUID NOT NULL UNIQUE REFERENCES technologies (id) ON DELETE RESTRICT,
    slug TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT detection_rules_slug_valid
        CHECK (slug ~ '^[a-z0-9]+(?:-[a-z0-9]+)*$')
);

CREATE TABLE detection_rule_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    detection_rule_id UUID NOT NULL REFERENCES detection_rules (id) ON DELETE RESTRICT,
    version SMALLINT NOT NULL,
    definition JSONB NOT NULL,
    published_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT detection_rule_versions_version_positive CHECK (version > 0),
    CONSTRAINT detection_rule_versions_definition_object CHECK (jsonb_typeof(definition) = 'object'),
    CONSTRAINT detection_rule_versions_rule_version_key UNIQUE (detection_rule_id, version)
);

CREATE TABLE detections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    crawl_snapshot_id UUID NOT NULL REFERENCES crawl_snapshots (id) ON DELETE RESTRICT,
    technology_id UUID NOT NULL REFERENCES technologies (id) ON DELETE RESTRICT,
    detection_rule_version_id UUID NOT NULL REFERENCES detection_rule_versions (id) ON DELETE RESTRICT,
    confidence SMALLINT NOT NULL,
    method TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT detections_confidence_valid CHECK (confidence BETWEEN 0 AND 100),
    CONSTRAINT detections_method_valid CHECK (method = 'deterministic_rule'),
    CONSTRAINT detections_observed_at_valid CHECK (observed_at >= created_at - INTERVAL '1 day'),
    CONSTRAINT detections_snapshot_technology_rule_version_key
        UNIQUE (crawl_snapshot_id, technology_id, detection_rule_version_id)
);

CREATE INDEX detections_snapshot_idx ON detections (crawl_snapshot_id);
CREATE INDEX detections_technology_observed_at_idx ON detections (technology_id, observed_at DESC);

CREATE TABLE detection_evidence (
    detection_id UUID NOT NULL REFERENCES detections (id) ON DELETE RESTRICT,
    source TEXT NOT NULL,
    evidence_key TEXT NOT NULL,
    evidence_value TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (detection_id, source, evidence_key, evidence_value),
    CONSTRAINT detection_evidence_source_valid CHECK (source IN ('html', 'header', 'script', 'dns', 'tls')),
    CONSTRAINT detection_evidence_key_valid
        CHECK (char_length(btrim(evidence_key)) BETWEEN 1 AND 128),
    CONSTRAINT detection_evidence_value_valid
        CHECK (char_length(btrim(evidence_value)) BETWEEN 1 AND 2048)
);

CREATE FUNCTION prevent_detection_catalogue_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'published detection rules and observations are immutable'
        USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER detection_rule_versions_prevent_mutation
    BEFORE UPDATE OR DELETE ON detection_rule_versions
    FOR EACH ROW
    EXECUTE FUNCTION prevent_detection_catalogue_mutation();

CREATE TRIGGER detections_prevent_mutation
    BEFORE UPDATE OR DELETE ON detections
    FOR EACH ROW
    EXECUTE FUNCTION prevent_detection_catalogue_mutation();

CREATE TRIGGER detection_evidence_prevent_mutation
    BEFORE UPDATE OR DELETE ON detection_evidence
    FOR EACH ROW
    EXECUTE FUNCTION prevent_detection_catalogue_mutation();

INSERT INTO technology_categories (slug, display_name)
VALUES
    ('framework', 'Framework'),
    ('payment-provider', 'Payment Provider'),
    ('hosting', 'Hosting'),
    ('analytics', 'Analytics'),
    ('ecommerce', 'Ecommerce');

INSERT INTO technologies (category_id, slug, display_name)
SELECT categories.id, values_to_insert.slug, values_to_insert.display_name
FROM (
    VALUES
        ('framework', 'nextjs', 'Next.js'),
        ('payment-provider', 'stripe', 'Stripe'),
        ('hosting', 'cloudflare', 'Cloudflare'),
        ('analytics', 'posthog', 'PostHog'),
        ('ecommerce', 'shopify', 'Shopify')
) AS values_to_insert(category_slug, slug, display_name)
JOIN technology_categories AS categories ON categories.slug = values_to_insert.category_slug;

INSERT INTO detection_rules (technology_id, slug)
SELECT technologies.id, values_to_insert.slug
FROM (
    VALUES
        ('nextjs', 'nextjs-v1'),
        ('stripe', 'stripe-v1'),
        ('cloudflare', 'cloudflare-v1'),
        ('posthog', 'posthog-v1'),
        ('shopify', 'shopify-v1')
) AS values_to_insert(technology_slug, slug)
JOIN technologies ON technologies.slug = values_to_insert.technology_slug;

INSERT INTO detection_rule_versions (detection_rule_id, version, definition)
SELECT rules.id, 1, values_to_insert.definition::JSONB
FROM (
    VALUES
        ('nextjs-v1', '{"threshold":70,"signals":[{"source":"header","key":"x-powered-by","match":"equals_ignore_case","value":"next.js","weight":90},{"source":"script","key":"src","match":"contains_ignore_case","value":"/_next/","weight":75},{"source":"html","key":"meta.generator","match":"equals_ignore_case","value":"next.js","weight":80}]}'),
        ('stripe-v1', '{"threshold":70,"signals":[{"source":"script","key":"src","match":"contains_ignore_case","value":"js.stripe.com","weight":95}]}'),
        ('cloudflare-v1', '{"threshold":70,"signals":[{"source":"header","key":"server","match":"equals_ignore_case","value":"cloudflare","weight":90},{"source":"header","key":"cf-ray","match":"present","weight":80}]}'),
        ('posthog-v1', '{"threshold":70,"signals":[{"source":"script","key":"src","match":"contains_ignore_case","value":"posthog.com","weight":90}]}'),
        ('shopify-v1', '{"threshold":70,"signals":[{"source":"script","key":"src","match":"contains_ignore_case","value":"cdn.shopify.com","weight":95},{"source":"header","key":"x-shopify-stage","match":"present","weight":90}]}')
) AS values_to_insert(rule_slug, definition)
JOIN detection_rules AS rules ON rules.slug = values_to_insert.rule_slug;
