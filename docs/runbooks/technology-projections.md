# Technology projections

BE-13 derives mutable current technology state and immutable technology-change history from successful snapshots. The authoritative records remain crawl snapshots, detections, evidence, and per-rule observations.

For every published rule, the worker persists one observation: `detected`, `confirmed_absent`, or `unknown`. An absence is confirmed only when all artifact sources required by that rule were available and no signal matched. `unknown` never removes an existing current technology.

The projection emits additions and removals. When exactly one current technology is confirmed absent and exactly one new technology is detected in the same category, it emits one `migrated` event instead. Ambiguous category changes remain independent additions/removals. Immutable historical reprocessing emits separate `version_changed` records only when a deterministic rule capture establishes two adjacent known, different technology versions. These derived records never rewrite the ordinary current-state projection.

## Session handoff

- Delivered outcome: immutable rule coverage observations, coverage-gated current state, and immutable technology-change events.
- Migration: `202608030008_technology_projections.sql` adds observations, current state, and history projections.
- Verification: pure projection transitions plus migration and queued-crawl integration assertions; `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
- Operational impact: each successful crawl updates the PostgreSQL projection in the same transaction. No new configuration is required.
- Next unlocked phase: BE-14, Meilisearch indexing, rebuild command, and index-lag telemetry.
