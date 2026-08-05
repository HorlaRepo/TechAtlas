CREATE TABLE worker_heartbeats (
    worker_name TEXT PRIMARY KEY,
    region TEXT NOT NULL,
    in_flight_work INTEGER NOT NULL DEFAULT 0,
    completed_total BIGINT NOT NULL DEFAULT 0,
    last_heartbeat_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT worker_heartbeats_name_present CHECK (char_length(btrim(worker_name)) > 0),
    CONSTRAINT worker_heartbeats_region_present CHECK (char_length(btrim(region)) > 0),
    CONSTRAINT worker_heartbeats_in_flight_nonnegative CHECK (in_flight_work >= 0),
    CONSTRAINT worker_heartbeats_completed_total_nonnegative CHECK (completed_total >= 0)
);

CREATE INDEX worker_heartbeats_last_heartbeat_at_idx
    ON worker_heartbeats (last_heartbeat_at DESC);
