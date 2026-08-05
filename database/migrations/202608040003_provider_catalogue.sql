CREATE TABLE providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT providers_slug_valid
        CHECK (slug ~ '^[a-z0-9]+(?:-[a-z0-9]+)*$'),
    CONSTRAINT providers_name_present
        CHECK (char_length(btrim(display_name)) > 0)
);

CREATE TABLE technology_provider_mappings (
    technology_id UUID PRIMARY KEY REFERENCES technologies (id) ON DELETE RESTRICT,
    provider_id UUID NOT NULL REFERENCES providers (id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX technology_provider_mappings_provider_idx
    ON technology_provider_mappings (provider_id, technology_id);

CREATE FUNCTION prevent_provider_catalogue_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'published provider mappings are immutable'
        USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER providers_prevent_mutation
    BEFORE UPDATE OR DELETE ON providers
    FOR EACH ROW
    EXECUTE FUNCTION prevent_provider_catalogue_mutation();

CREATE TRIGGER technology_provider_mappings_prevent_mutation
    BEFORE UPDATE OR DELETE ON technology_provider_mappings
    FOR EACH ROW
    EXECUTE FUNCTION prevent_provider_catalogue_mutation();

INSERT INTO providers (slug, display_name)
VALUES
    ('vercel', 'Vercel'),
    ('stripe', 'Stripe'),
    ('cloudflare', 'Cloudflare'),
    ('posthog', 'PostHog'),
    ('shopify', 'Shopify');

INSERT INTO technology_provider_mappings (technology_id, provider_id)
SELECT technologies.id, providers.id
FROM (
    VALUES
        ('nextjs', 'vercel'),
        ('stripe', 'stripe'),
        ('cloudflare', 'cloudflare'),
        ('posthog', 'posthog'),
        ('shopify', 'shopify')
) AS mappings(technology_slug, provider_slug)
JOIN technologies ON technologies.slug = mappings.technology_slug
JOIN providers ON providers.slug = mappings.provider_slug;
