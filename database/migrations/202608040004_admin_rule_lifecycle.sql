CREATE TABLE detection_rule_drafts (
    detection_rule_id UUID PRIMARY KEY REFERENCES detection_rules (id) ON DELETE RESTRICT,
    definition JSONB NOT NULL,
    updated_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    tested_at TIMESTAMPTZ,
    CONSTRAINT detection_rule_drafts_definition_object CHECK (jsonb_typeof(definition) = 'object'),
    CONSTRAINT detection_rule_drafts_updated_by_present CHECK (char_length(btrim(updated_by)) > 0)
);

ALTER TABLE detection_rules
    ADD COLUMN active_version_id UUID REFERENCES detection_rule_versions (id) ON DELETE RESTRICT;

UPDATE detection_rules AS rules
SET active_version_id = versions.id
FROM detection_rule_versions AS versions
WHERE versions.detection_rule_id = rules.id
  AND versions.version = 1;

CREATE FUNCTION validate_detection_rule_active_version()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.active_version_id IS NOT NULL AND NOT EXISTS (
        SELECT 1
        FROM detection_rule_versions
        WHERE id = NEW.active_version_id
          AND detection_rule_id = NEW.id
    ) THEN
        RAISE EXCEPTION 'active version must belong to its detection rule'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER detection_rules_validate_active_version
    BEFORE INSERT OR UPDATE OF active_version_id ON detection_rules
    FOR EACH ROW
    EXECUTE FUNCTION validate_detection_rule_active_version();

CREATE INDEX detection_rules_active_version_idx
    ON detection_rules (active_version_id)
    WHERE active_version_id IS NOT NULL;
