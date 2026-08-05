# Crawl reliability and detection coverage implementation phases

## Status

Proposed implementation roadmap based on
[the improvement plan](../techatlas-improvement-plan.md) and the current
implementation as reviewed on 2026-08-05.

Each phase is deliberately sized for one focused implementation session. A
phase is complete only when its scoped acceptance criteria pass; do not begin
later phases partially. PostgreSQL remains authoritative, historical crawl
records remain immutable, and every normal crawl continues to be
scheduler-mediated.

## Current-state constraints

- The crawler currently treats every non-`404`/non-2xx robots response, invalid
  UTF-8 robots body, and parser error as `RobotsUnavailable`.
- The scheduler currently retries all failed crawl attempts. The crawler's
  retryability classifier is not used by that path, so permanent policy
  denials and unsafe targets need an explicit retry disposition.
- The robots parser fails the whole file on a malformed directive. It must
  become tolerant without treating unavailable policy as permission to crawl.
- SSRF validation rejects a host when any resolved address is non-public. It
  must not be weakened without evidence from the existing rejected domains.
- The production detector already uses weighted, additive evidence and stores
  immutable evidence and confidence. Detection work should add safe signals,
  rules, and confidence presentation rather than replace the scorer.
- `Set-Cookie` values are intentionally redacted. Cookie detection may retain
  only bounded cookie names/prefixes, never values.
- Historical replay does not have future-only artifacts such as cookies,
  favicon hashes, CNAME/MX/TXT records, or rendered output. New coverage needs
  controlled recrawls; it cannot be recovered by mutating historic snapshots.
- Browser rendering is explicitly out of scope for v1 in the PRD. It is a
  gated future track, not an implementation phase that may start automatically.

## Delivery rules

- Every API contract change updates the Rust API models/handlers, regenerates
  and validates OpenAPI, then regenerates `packages/api-client` before dashboard
  consumers change.
- Every schema change is a new forward-only migration with relevant SQLx
  integration coverage.
- Every crawler change has fixture-based tests; unit tests do not use live
  network or database calls.
- Every dashboard phase includes loading, empty, unavailable, error, keyboard,
  and responsive states.
- Do not log remote response bodies, cookie values, secrets, credentials, or
  unbounded remote text. Use correlation IDs and bounded structured fields.

## Phase plan

| ID | Session outcome | Backend and operations work | Frontend work | Completion check |
| --- | --- | --- | --- | --- |
| IMP-00 | Reproducible failure baseline | Add a protected, fixed-window failure-reason report with attempts, distinct domains, retry state, terminal reason, and safe diagnostic samples. Record the exact report/query parameters in a runbook. | Add a compact Crawl quality panel to Admin Scheduler or Monitoring. | The same 24-hour window can be reproduced before and after a release. |
| IMP-01 | Explicit failure disposition | Introduce typed crawl failure codes with retryable and terminal disposition. Make scheduler retry decisions use that disposition rather than retrying every failure. Add a policy-level terminal outcome/reason without mutating crawl attempts. Define whether the configured limit means three total deliveries or three retries. | Replace raw failure codes with stable status labels and correct retry affordances. | Policy denials and unsafe targets do not receive automatic retries; transient network failures do. |
| IMP-02 | Lenient robots parsing | Return robots policy plus parse diagnostics. Tolerate BOMs, best-effort Latin-1 fallback, comment edge cases, unknown directives, and malformed individual lines. Keep fail-closed behaviour when no safe policy can be derived. Add real anonymised failure fixtures. | None. | Valid directives from a partially malformed file still control access; skipped lines create an informational event/metric. |
| IMP-03 | Correct robots fetch semantics | Classify 401/403 as `robots_denied`; timeout, transient DNS, 400, 408, 429, and 5xx as `robots_transient`; and wholly unusable successful responses as `robots_permanent`. Use the existing scheduler/outbox retry path only. Emit a credential-free request descriptor for 400 diagnostics. | Show distinct outcome text and retry state in admin operations. | A 504 followed by 200 succeeds without a terminal outcome; a genuine 403 is full-disallow and is not retried. |
| IMP-04 | Robots-denial audit | Classify existing 401/403 responses using safe, ephemeral WAF/origin indicators. Confirm the actual HTTP User-Agent, which differs from the robots-policy matching token. Do not retain challenge bodies or add bot-evasion behaviour. | Add explanatory robots-denied tooltip/help text. | Genuine policy denials are documented as expected; any challenge-page finding becomes a separately approved investigation. |
| IMP-05 | Recover and re-measure robots candidates | Add an authenticated, audited bulk command that makes only former robots transient/permanent failures eligible for normal scheduler processing. It must not bypass robots validation. Re-run IMP-00 after deployment. | Confirmation dialog with eligible-domain count and result state. | Recovered domains are scheduled during the next scheduler cycle and the same report measures the outcome. |
| IMP-06 | SSRF evidence audit | Export all `unsafe_target` terminal records with domain, correlation ID, resolved-address summary, and rejection reason. Classify each candidate from a controlled operational environment. | Link/filter the candidates from Crawl quality; do not expose them publicly. | Each rejected domain has a recorded classification: genuinely unsafe, mixed answer, or other. |
| IMP-07 | Conditional SSRF correction | Only if IMP-06 proves false positives, filter mixed DNS answers to public addresses and reject only when none remain. Preserve validation in the resolver used by the HTTP transport so DNS rebinding remains blocked. | Update failure wording if behaviour changes. | Mixed-record fixtures pass when a public address exists; private-only and rebinding cases remain blocked. |
| IMP-08 | Safe signal contract | Publish an ADR covering new evidence sources, bounds, retention, redaction, historical availability, and rule semantics. Add typed evidence sources and any required schema constraints before source acquisition. | None. | The ADR identifies what is retained, what is discarded, and what a historic unavailable signal means. |
| IMP-09 | Favicon hashing | Fetch only bounded same-origin `/favicon.ico` when robots permits it. Retain a hash only, never icon bytes. Add collision and unavailable fixtures. | Render favicon-derived evidence as medium-confidence support. | Hash-only detections cannot receive high confidence, and no icon data is persisted. |
| IMP-10 | Cookie-name signals | Extract bounded `Set-Cookie` names/prefixes during acquisition and discard values immediately. Add cookie pattern rules and fixtures. | None. | Cookie values cannot enter snapshots, logs, evidence, or API responses. |
| IMP-11 | DNS signals | Capture bounded CNAME, MX, and recognised TXT/SPF provider markers. Do not persist arbitrary TXT data or verification tokens; emit only safe normalised provider markers. | Existing DNS evidence presentation handles safe new entries. | Fixtures prove provider matching and token redaction. |
| IMP-12 | JSON-LD markers | Extract only allowlisted, platform-identifying JSON-LD atoms. Do not use or store arbitrary JSON-LD content as detection evidence. | None. | Valid platform markers match deterministically without exposing page content. |
| IMP-13 | Inline static JavaScript markers | Extract bounded static markers such as `window.Shopify`, `dataLayer`, `ga(`, and `gtag(` without execution. Emit marker atoms, not raw script contents. | None. | Inline-script fixtures detect known markers and keep identifiers/content out of evidence. |
| IMP-14 | Manifest and build fingerprints | Add bounded, same-origin, robots-permitted manifest acquisition where justified; add chunk/source-map filename pattern support without downloading unbounded assets. | None. | The crawler never follows third-party manifest/icon URLs, and all new requests retain SSRF/robots safeguards. |
| IMP-15 | Rule-authoring throughput | Add authenticated, audited technology/rule creation support while preserving existing test, publish, activate, rollback, and replay controls. Complete the licence review before adapting external definitions. | Add an accessible New technology rule flow to the Rules page. | An admin can create, test, publish, activate, and roll back one rule through the generated contract. |
| IMP-16 | Rule corpus batches | Add one bounded technology/category cohort per session, each with provenance, positive, negative, and regression fixtures. Do not import a large third-party corpus in one change. | Catalogue display updates through the generated public API. | Each batch has a licence record and passes its fixture suite. |
| IMP-17 | Confidence tiers and calibration | Formalise High, Medium, and Low as a typed derivation of the existing numeric score. Calibrate thresholds against a labelled fixture set; do not replace the weighted evaluator. | Add a tier badge and explanation beside the existing percentage/evidence display. | The same score always produces the same tier, and users can inspect the contributing evidence. |
| IMP-18 | Controlled coverage recrawl | Add a bounded, audited recrawl workflow after significant source/rule batches. Measure crawl success, technologies per domain, confidence distribution, and false-positive rate against IMP-00. | Show recrawl progress and before/after quality metrics to administrators. | New immutable snapshots demonstrate the coverage change; historic records are unchanged. |
| IMP-19 | Rendering decision gate | Re-pull the operational and coverage measurements. Decide whether the remaining material gap is truly render-only. Do not write renderer code in this phase. | Optional explanatory coverage/capacity note. | A product decision explicitly accepts or rejects the future rendering track. |

## Rule corpus batching template

Repeat IMP-16 for each cohort. One cohort is small enough to review in one
session and contains:

1. A documented technology/category list and source licence/provenance.
2. Rules and confidence weights using only currently acquired safe signals.
3. Positive, negative, and regression fixtures.
4. A deterministic test run and rule-version publication.
5. A controlled recrawl decision, not a mutation or hidden historical replay.

## Gated future track: selective JavaScript rendering

This track may begin only after IMP-19 approves it and the PRD and ADR are
updated. It requires separate sessions:

| ID | Session outcome | Backend and operations work | Frontend work |
| --- | --- | --- | --- |
| RENDER-00 | Product and architecture decision | Update PRD and write ADR for the renderer boundary, ownership, retention, resource ceiling, security model, and rollback. | None. |
| RENDER-01 | Isolated renderer foundation | Add a separate Node/Playwright service, versioned message contract, bounded queue consumption, hard timeouts/concurrency, tracing, and isolated egress controls. | None. |
| RENDER-02 | Safe selective dispatch | Add a static-first evidence-sufficiency heuristic and idempotent renderer dispatch. Rendering remains sampled/selective. | Admin visibility for queued, capped, skipped, and failed render work. |
| RENDER-03 | Immutable rendered evidence | Capture bounded rendered DOM/global marker atoms, rerun deterministic rules, and preserve source provenance without modifying static snapshots. | Evidence source and rendering-state disclosure on domain profiles. |

The current production worker is capped at one CPU, 1.5 GB memory, and two
concurrent crawl jobs. Chromium therefore requires a separately reviewed
capacity and isolation decision; it must not be placed opportunistically in the
existing worker.

## Verification per delivered phase

Run the smallest relevant verification set, plus the required checks for any
affected boundary:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm api:openapi:check
pnpm api:client:check
pnpm --filter @techatlas/dashboard lint
pnpm --filter @techatlas/dashboard typecheck
pnpm --filter @techatlas/dashboard test
```

For migrations, contract changes, or full workflow changes, also run the
relevant Docker/integration checks and record the failure-report comparison,
configuration additions, operational impact, and exact next unlocked phase in
the phase handoff.
