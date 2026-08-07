INSERT INTO technology_categories (slug, display_name)
VALUES ('cms', 'Content Management System')
ON CONFLICT (slug) DO NOTHING;

INSERT INTO technologies (category_id, slug, display_name)
SELECT categories.id, values_to_insert.slug, values_to_insert.display_name
FROM (
    VALUES
        ('cms', 'wordpress', 'WordPress'),
        ('cms', 'drupal', 'Drupal'),
        ('framework', 'express', 'Express')
) AS values_to_insert(category_slug, slug, display_name)
JOIN technology_categories AS categories ON categories.slug = values_to_insert.category_slug
ON CONFLICT (slug) DO NOTHING;

INSERT INTO detection_rules (technology_id, slug)
SELECT technologies.id, values_to_insert.slug
FROM (
    VALUES
        ('wordpress', 'wordpress-v1'),
        ('drupal', 'drupal-v1'),
        ('express', 'express-v1')
) AS values_to_insert(technology_slug, slug)
JOIN technologies ON technologies.slug = values_to_insert.technology_slug
ON CONFLICT (slug) DO NOTHING;

INSERT INTO detection_rule_versions (detection_rule_id, version, definition)
SELECT rules.id, 1, values_to_insert.definition::JSONB
FROM (
    VALUES
        ('wordpress-v1', '{"threshold":90,"signals":[{"source":"header","key":"x-powered-by","match":"contains_ignore_case","value":"wordpress","weight":90}]}'),
        ('drupal-v1', '{"threshold":90,"signals":[{"source":"header","key":"x-generator","match":"contains_ignore_case","value":"drupal","weight":90},{"source":"html","key":"meta.generator","match":"contains_ignore_case","value":"drupal","weight":90}]}'),
        ('express-v1', '{"threshold":90,"signals":[{"source":"header","key":"x-powered-by","match":"equals_ignore_case","value":"express","weight":90}]}')
) AS values_to_insert(rule_slug, definition)
JOIN detection_rules AS rules ON rules.slug = values_to_insert.rule_slug
ON CONFLICT (detection_rule_id, version) DO NOTHING;

INSERT INTO detection_rule_versions (detection_rule_id, version, definition)
SELECT rules.id, 2, '{"threshold":70,"signals":[{"source":"header","key":"x-powered-by","match":"contains_ignore_case","value":"next.js","weight":90},{"source":"script","key":"src","match":"contains_ignore_case","value":"/_next/","weight":75},{"source":"html","key":"meta.generator","match":"equals_ignore_case","value":"next.js","weight":80}]}'::JSONB
FROM detection_rules AS rules
WHERE rules.slug = 'nextjs-v1'
ON CONFLICT (detection_rule_id, version) DO NOTHING;

UPDATE detection_rules AS rules
SET active_version_id = versions.id
FROM detection_rule_versions AS versions
WHERE versions.detection_rule_id = rules.id
  AND versions.version = 1
  AND rules.slug IN ('wordpress-v1', 'drupal-v1', 'express-v1')
  AND rules.active_version_id IS NULL;

UPDATE detection_rules AS rules
SET active_version_id = versions.id
FROM detection_rule_versions AS versions
WHERE versions.detection_rule_id = rules.id
  AND versions.version = 2
  AND rules.slug = 'nextjs-v1';
