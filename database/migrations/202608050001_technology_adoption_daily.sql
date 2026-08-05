-- This is a rebuildable projection over immutable crawl snapshots. It is intentionally mutable:
-- the scheduler captures the current daily view and the maintenance CLI can repair/backfill a range.
CREATE TABLE technology_adoption_daily (
    observed_on DATE NOT NULL,
    technology_id UUID NOT NULL REFERENCES technologies (id) ON DELETE RESTRICT,
    domain_count BIGINT NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (observed_on, technology_id),
    CONSTRAINT technology_adoption_daily_count_nonnegative CHECK (domain_count >= 0)
);

CREATE INDEX technology_adoption_daily_technology_day_idx
    ON technology_adoption_daily (technology_id, observed_on);
