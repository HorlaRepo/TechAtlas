CREATE TABLE public_refresh_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domain_id UUID NOT NULL REFERENCES domains (id) ON DELETE RESTRICT,
    requested_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT public_refresh_requests_timestamps_valid CHECK (created_at >= requested_at)
);

CREATE INDEX public_refresh_requests_domain_requested_idx
    ON public_refresh_requests (domain_id, requested_at DESC);
