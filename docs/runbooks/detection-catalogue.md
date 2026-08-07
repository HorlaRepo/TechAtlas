# Detection catalogue

BE-12 adds deterministic, immutable technology detections to successful crawl snapshots. The worker parses only sanitized HTML and already-redacted response headers before evaluating the static published v1 rules. Missing and unavailable signals do not prove a technology is absent and never create negative records.

## Published v1 rules

| Technology | Category | Signals | Weights |
| --- | --- | --- | --- |
| Next.js | Framework | `x-powered-by` contains `Next.js`, `/_next/` script, `meta.generator: Next.js` | 90, 75, 80 |
| Stripe | Payment Provider | `js.stripe.com` script | 95 |
| Cloudflare | Hosting | `server: cloudflare`, `cf-ray` present | 90, 80 |
| PostHog | Analytics | `posthog.com` script | 90 |
| Shopify | Ecommerce | `cdn.shopify.com` script, `x-shopify-stage` present | 95, 90 |
| WordPress | CMS | `x-powered-by` contains `WordPress` | 90 |
| Drupal | CMS | `x-generator` or `meta.generator` contains `Drupal` | 90, 90 |
| Express | Framework | `x-powered-by: Express` | 90 |

Each matching signal contributes once. Scores sum to a maximum of 100, and each published rule threshold is at least 70. Evidence preserves its parser source, key, and normalized value. The database stores seeded technologies, rules, and immutable rule versions; detections reference the exact rule version and retain immutable evidence rows. The active Next.js rule is v2 so versioned `x-powered-by` values such as `Next.js/14` are accepted without editing the original v1 rule.

## Historical reprocessing

An `admin:operate` user may start an immutable replay for one published rule version. The run stores an idempotency key, correlation ID, initiating subject, and a database-backed item for every retained successful snapshot. Workers claim one item at a time, retry failures up to `WORKER_REPROCESSING_MAX_ATTEMPTS`, then mark exhausted work `dead_lettered`; run counters expose partial failure rather than hiding it.

Replays write separate observations, detections, and evidence. They never edit snapshots, ordinary detections/evidence, or the current stack projection. A rule can optionally declare `version_evidence` with `source`, `key`, and a Rust-regex `pattern` containing exactly one capture group. Only a matching, deterministic capture is stored; all other cases remain unknown. On run completion, comparable derived detections with two known, differing versions create immutable `version_changed` events for the public domain timeline.

If a retained raw artifact is missing, expired, corrupt, or unreadable, the work item retries and ultimately becomes terminally visible. Operators can inspect run totals and dead-letter counts in the detection-rules admin screen. Rolling back is done by activating a prior rule for future crawls; immutable replay history is retained for provenance.

## Session handoff

- Delivered outcome: versioned static catalogue, pure weighted evaluation, immutable detection/evidence persistence, and atomic worker integration.
- Migration: `202608030007_detection_catalogue.sql` adds the catalogue, rule-version, detection, and evidence tables with append-only constraints.
- Verification: positive, negative, and duplicate-signal regression fixtures; migration and queued-crawl persistence tests; `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
- Operational impact: successful worker crawls write normal detections while a separate worker loop processes historical replay items without altering current projections.
