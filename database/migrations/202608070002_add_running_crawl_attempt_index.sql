CREATE INDEX crawl_attempts_running_started_at_idx
    ON crawl_attempts (started_at ASC, id ASC)
    WHERE status = 'running';
