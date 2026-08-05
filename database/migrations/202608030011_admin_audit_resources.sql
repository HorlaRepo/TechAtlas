ALTER TABLE admin_audit_events
    ALTER COLUMN domain_id DROP NOT NULL,
    ADD COLUMN resource_kind TEXT,
    ADD COLUMN resource_id UUID,
    ADD COLUMN metadata JSONB NOT NULL DEFAULT '{}'::JSONB;

UPDATE admin_audit_events
SET resource_kind = 'domain', resource_id = domain_id
WHERE resource_kind IS NULL;

ALTER TABLE admin_audit_events
    ALTER COLUMN resource_kind SET NOT NULL,
    ALTER COLUMN resource_id SET NOT NULL,
    ADD CONSTRAINT admin_audit_events_resource_kind_valid
        CHECK (resource_kind IN ('domain', 'import')),
    ADD CONSTRAINT admin_audit_events_metadata_object
        CHECK (jsonb_typeof(metadata) = 'object');

CREATE INDEX admin_audit_events_occurred_at_idx
    ON admin_audit_events (occurred_at DESC, id DESC);

CREATE FUNCTION prevent_admin_audit_event_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'admin audit events are immutable';
END;
$$;

CREATE TRIGGER admin_audit_events_immutable
    BEFORE UPDATE OR DELETE ON admin_audit_events
    FOR EACH ROW
    EXECUTE FUNCTION prevent_admin_audit_event_mutation();
