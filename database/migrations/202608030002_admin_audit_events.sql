CREATE TABLE admin_audit_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_subject TEXT NOT NULL CHECK (btrim(actor_subject) <> ''),
    action TEXT NOT NULL,
    domain_id UUID NOT NULL REFERENCES domains(id) ON DELETE RESTRICT,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX admin_audit_events_domain_occurred_at_idx
    ON admin_audit_events (domain_id, occurred_at DESC);

CREATE INDEX admin_audit_events_actor_occurred_at_idx
    ON admin_audit_events (actor_subject, occurred_at DESC);
