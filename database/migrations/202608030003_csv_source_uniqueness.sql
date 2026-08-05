CREATE UNIQUE INDEX domain_sources_active_csv_name_key
    ON domain_sources (name)
    WHERE kind = 'csv' AND archived_at IS NULL;
