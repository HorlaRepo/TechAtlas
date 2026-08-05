# Implementation gap report

**Audit date:** 2026-08-04  
**Scope:** Current repository implementation compared with the canonical [PRD](../TECHATLAS_PRD.md), the implementation-phase tracker, and user-visible dashboard behaviour.

This is an implementation backlog, not a statement that every item must be
included in the next release. Items explicitly excluded from v1 in the PRD
(for example browser rendering, screenshots, accounts, AI summaries, and
multi-node deployment) are intentionally not listed as gaps.

## Summary

**Implementation status (2026-08-05):** GAP-PH-00 through GAP-PH-07 are complete. This resolves
GAP-001 through GAP-013, including the DNS/TLS capture and immutable historical-reprocessing portions of GAP-008;
the audit evidence below remains as the 2026-08-04 baseline.

| Priority | Confirmed gaps | Why it matters |
| --- | ---: | --- |
| P0 | 0 | No known data-loss, authorization-bypass, or crawler-safety defect from this audit. |
| P2 | 0 | No remaining confirmed gaps from this audit. |

Priority definitions:

- **P1** — required v1 capability, materially misleading UI, or an important operational/data-completeness limitation.
- **P2** — valuable completion or usability work that does not block safe operation of the existing workflow.

## P1 — required capability and misleading UI

### GAP-001 — Public landing page is absent

- **PRD reference:** Section 4.1, route `/`.
- **Evidence:** The router redirects `/` directly to `/search`.
- **Missing behaviour:** Product landing/discovery with global search, headline metrics, discoveries/trends, and public navigation.
- **Recommended resolution:** Add a lightweight public landing feature composed from the existing search entry point and safe analytics aggregates. Keep it unauthenticated and API-backed.

### GAP-002 — Public domain catalogue route is absent

- **PRD reference:** Section 4.1, route `/domains`.
- **Evidence:** The router has `/domains/$domain`, but no `/domains` route. The only catalogue-like management route is protected `/admin/domains`.
- **Missing behaviour:** Public browse/filter catalogue that links to domain profiles.
- **Recommended resolution:** Add `/domains` as a public search/catalogue presentation over the existing generated public search client, with URL-backed filters and pagination.

### GAP-003 — Public About/methodology route is absent

- **PRD reference:** Section 4.1, route `/about`.
- **Evidence:** No `/about` route or page exists in the dashboard router.
- **Missing behaviour:** A user-facing explanation of collection methods, sources, policies, evidence, limitations, and refresh behaviour.
- **Recommended resolution:** Add a static, accessible public page that links to the appropriate published policy/methodology material without exposing operational secrets.

### GAP-004 — Analytics does not provide adoption-over-time charts

- **PRD reference:** Sections 4.1 and 4.5; FE-10 acceptance check.
- **Evidence:** The Analytics page requests overview, rankings, movers, and daily changes only. It does not request the existing `/api/v1/public/analytics/adoption` endpoint. The current adoption endpoint returns present counts by technology, not a historical series. The only chart-like visual is the daily `Stack changes` horizontal bar list.
- **Missing behaviour:** Adoption-over-time visualization, richer trend views, and chart-based exploration of the intelligence data.
- **Recommended resolution:** First define a bounded historical-adoption aggregate in the public contract and database read model; then add responsive, text-backed charts for adoption and stack changes. Use explicit empty states when insufficient history exists.
- **Current-data note:** The live changes endpoint currently returns one day of activity, so a multi-day trend cannot be meaningful yet even after a chart is added.

### GAP-005 — Analytics coverage is incomplete against the PRD

- **PRD reference:** Section 4.5.
- **Present:** Top technologies/categories/providers/countries, growing and declining technologies, and aggregate daily changes.
- **Missing:** Large migration views, newest/frequently crawled domains, and a public-safe presentation of detection success rate, worker throughput, and queue health.
- **Recommended resolution:** Decide which operational aggregates are safe for public exposure. Keep privileged operational detail in the protected Operations Center; add only safe aggregate cards to public analytics. Add API support before UI work where no aggregate exists.

### GAP-006 — Admin Analytics navigation is a visible disabled placeholder

- **PRD reference:** Section 4.6 requires an administrator experience with operational visibility; the dashboard itself exposes an `Analytics` menu entry.
- **Evidence:** `Analytics` in the admin sidebar has no route and renders as `aria-disabled` with the title `Coming soon`. Public Analytics is available separately at `/analytics`.
- **Impact:** Administrators see a menu item that looks available but cannot use it, and the public-vs-admin analytics boundary is unclear.
- **Recommended resolution:** Either remove the entry until an authenticated analytics surface exists, or add a clearly named route. The route should either expose admin-only operational analytics or deliberately link to the public intelligence analytics page with an explanatory label.

### GAP-007 — Quick Crawl is not connected to a crawl command

- **PRD reference:** Section 4.6 requires administrators to trigger a permitted crawl/refresh; all admin actions must be authorized and auditable.
- **Evidence:** The global `Quick Crawl` action only displays `Quick crawl request staged in the local dashboard preview.` It does not collect a target, call an API, create a refresh request, or create an audit event.
- **Impact:** The primary action in the protected shell appears functional but performs no operation.
- **Recommended resolution:** Replace it with a focused, permission-gated dialog that validates a target domain and invokes a scheduler-mediated API command, or remove it in favour of the existing domain-level refresh flow. The chosen command must be rate-limited and audited.

### GAP-008 — DNS/TLS acquisition and historical rule reprocessing

- **PRD reference:** FR-CRW-003 and FR-DET-003/FR-DET-004.
- **Resolved by:** GAP-PH-05 captures immutable DNS/TLS observations and GAP-PH-06 adds auditable, idempotent historical reprocessing with bounded worker retries, terminal visibility, immutable derived detections/evidence, deterministic optional version captures, and public `version_changed` timeline events.

## P2 — completion and usability work

### GAP-009 — Required admin Monitoring and Settings surfaces are unavailable

- **PRD reference:** Section 4.6 lists Monitoring and Settings in the protected application.
- **Evidence:** `Settings` is a disabled sidebar placeholder. There is no dashboard Monitoring route; observability currently lives in infrastructure/runbooks rather than a protected dashboard surface.
- **Recommended resolution:** Define the intended split between dashboard status summaries and Grafana/Prometheus. Add an authenticated Monitoring page or a safe deep-link panel, then implement only the settings that have real backend configuration support.

### GAP-010 — Admin sidebar “Technologies” is disabled despite a public library existing

- **Evidence:** The sidebar `Technologies` item is disabled, while the public technology library exists at `/technologies`.
- **Impact:** This duplicates an accessible capability as an apparently unavailable admin feature.
- **Recommended resolution:** Link it to the public library, rename it to clarify it is a catalogue, or replace it with a real protected catalogue-management page if that is required.

### GAP-011 — Notifications and Dashboard Help are placeholder toasts

- **Evidence:** Notifications always reports `No new operational alerts in the local preview`; Help reports it will be available with the operations handbook.
- **Recommended resolution:** Hide these affordances until they are useful, or connect Notifications to safe alert/incident summaries and Help to the relevant runbook pages.

### GAP-012 — Application breadcrumb is static

- **Evidence:** The protected shell always displays `TechAtlas > Overview`, regardless of the current route.
- **Impact:** Navigation context is inaccurate on Domains, Imports, Scheduler, Queue, Workers, Audit, and Rules.
- **Recommended resolution:** Derive the breadcrumb title from the active TanStack Router route and ensure it remains usable on narrow layouts.

### GAP-013 — Frontend hardening phase remains unfinished

- **Tracker reference:** `docs/implementation-phases.md`, FE-12.
- **Missing work:** A route-level accessibility sweep, visual-regression coverage, and a documented performance pass against the project’s build budgets.
- **Recommended resolution:** Complete FE-12 after the P1 route/analytics work stabilizes. Record the tested breakpoints, keyboard/focus checks, automated accessibility checks, visual baselines, and bundle-budget result.

## Deliberately excluded from this report

The following are already marked as future opportunities outside v1 in the PRD and should be planned as product expansion rather than gap remediation:

- Browser rendering and screenshots.
- Public API keys, public API productisation, GraphQL, user accounts, and premium datasets.
- Notifications as a broader product feature, natural-language discovery, structured AI summaries, and trend forecasting.
- Multi-node deployment.

## Implementation phases

The phases below are deliberately ordered to avoid building visualizations
before their data model exists, or exposing admin controls before they perform
a safe, auditable operation. Each phase is a separately releasable vertical
slice; a later phase must not be partially implemented early.

| Phase | Name | Gap IDs | Depends on |
| --- | --- | --- | --- |
| GAP-PH-00 | Product-boundary decisions | GAP-004, GAP-005, GAP-006, GAP-009 | — |
| GAP-PH-01 | Honest and functional admin shell | GAP-006, GAP-007, GAP-010, GAP-011, GAP-012 | GAP-PH-00 decisions affecting Analytics |
| GAP-PH-02 | Complete public information architecture | GAP-001, GAP-002, GAP-003 | Existing public search/read APIs |
| GAP-PH-03 | Historical analytics data and public charts | GAP-004, GAP-005 | GAP-PH-00, GAP-PH-02 |
| GAP-PH-04 | Protected monitoring, settings, and admin analytics | GAP-006, GAP-009 | GAP-PH-00, existing admin telemetry |
| GAP-PH-05 | DNS and TLS crawl capture | GAP-008 (capture portion) | Existing crawler/snapshot workflow |
| GAP-PH-06 | Immutable reprocessing and version-change history | GAP-008 (history portion) | GAP-PH-05 is independent but may run before or after it |
| GAP-PH-07 | Frontend release hardening | GAP-013 | GAP-PH-01 through GAP-PH-04 |

### GAP-PH-00 — Product-boundary decisions

**Status:** Complete (2026-08-05).

**Purpose:** Resolve the cross-cutting choices before adding contracts,
database projections, or privileged views.

**Scope**

- Publish an ADR defining which analytics are public safe aggregates and which
  are protected operational analytics. This resolves the public Analytics vs
  admin Analytics navigation ambiguity.
- Publish an ADR or extend the chosen ADR with the historical-adoption
  projection: measurement unit, daily snapshot/cutoff semantics, retention,
  rebuild process, zero/unknown handling, and chart time windows.
- Define the protected Monitoring and Settings boundary. Runtime environment
  settings must remain environment-managed unless a separate audited
  configuration workflow is approved.
- Specify the Quick Crawl command: required permission, valid target input,
  rate limit, scheduler mediation, confirmation, and audit event.

**Completion check**

- ADRs are accepted, linked from the ADR index, and identify the exact API,
  migration, and UI changes owned by the following phases.
- No production code is changed merely to bypass an unresolved product or
  authorization decision.

### GAP-PH-01 — Honest and functional admin shell

**Status:** Complete (2026-08-05).

**Purpose:** Every visible protected-shell action is either usable or absent;
no control may be presented as a working operation when it is only a toast.

**Scope**

- Replace Quick Crawl with a target-domain dialog that calls a
  scheduler-mediated, permission-gated, rate-limited, audited command. Reuse
  an existing command only if it has all of those properties; otherwise add a
  protected API endpoint, update OpenAPI, and regenerate the client.
- Remove the disabled Analytics item until GAP-PH-04 provides its destination,
  or make it a clearly labelled link to the public analytics page if the ADR
  selects that outcome.
- Link the Technologies item to the public technology library or remove it
  from the admin-only navigation.
- Remove placeholder Notifications and Help controls until real data and
  documentation destinations exist.
- Derive the protected breadcrumb from the active router route.

**Completion check**

- Keyboard and pointer tests prove that each remaining shell control has a
  real destination or command.
- An authorized Quick Crawl creates the expected scheduler request and audit
  record; unauthenticated and unauthorized attempts fail safely.
- The dashboard typecheck, lint, focused component tests, OpenAPI freshness,
  and generated-client freshness checks pass.

### GAP-PH-02 — Complete public information architecture

**Status:** Complete (2026-08-05).

**Purpose:** Deliver every required public top-level route without duplicating
the existing public search models or bypassing the generated client.

**Scope**

- Add `/` as an API-backed landing/discovery page with global search, headline
  metrics, selected discoveries/trends, and public navigation.
- Add `/domains` as a browseable, filterable catalogue using the existing
  public search contract, URL state, pagination, and links to profiles.
- Add `/about` with methodology, source, safety, evidence, data-availability,
  and refresh-limit explanations. Do not disclose credentials, raw storage
  locations, or sensitive evidence.
- Make public navigation expose the new routes and verify compact/mobile
  layouts.

**Completion check**

- `/`, `/domains`, and `/about` resolve directly, render meaningful loading,
  empty, and error states, and meet keyboard-accessibility expectations.
- `/domains` reproduces filter/search state from its URL and does not add a
  handwritten duplicate of generated API types.

### GAP-PH-03 — Historical analytics data and public charts

**Status:** Complete (2026-08-05).

**Purpose:** Turn analytics from rankings plus a single change-bar list into a
history-aware intelligence surface.

**Scope**

- Add the approved immutable or rebuildable historical-adoption aggregate,
  including a forward-only migration where schema is required, a bounded
  query, indexes, and a rebuild path.
- Version the public analytics contract, generate the API client, and expose
  adoption history with explicit unavailable/insufficient-history semantics.
- Add responsive, accessible public charts for adoption and stack changes.
  Every chart needs a text summary, semantic label, keyboard-safe controls,
  and an empty state; it must not rely on colour alone.
- Add the approved public-safe analytics from GAP-005: large migrations,
  newest/frequently crawled domains, and only the operational aggregates
  approved in GAP-PH-00.

**Completion check**

- Tests cover empty, single-point, and multi-point series plus the selected
  time window.
- The frontend consumes the generated analytics client, including the
  adoption-history endpoint; no API call or DTO is handwritten.
- Migration, contract, client generation, database read, and responsive UI
  tests demonstrate the full path.

### GAP-PH-04 — Protected monitoring, settings, and admin analytics

**Status:** Complete (2026-08-05).

**Purpose:** Give administrators an explicit, authenticated operational view
without leaking privileged telemetry into public analytics.

**Scope**

- Implement the Analytics destination selected in GAP-PH-00: either a
  protected operational-analytics route or a deliberately labelled link to
  public intelligence analytics. It must no longer be disabled.
- Add a protected Monitoring page or a safe dashboard summary/deep-link to
  the existing observability system. Display dependency readiness, queue
  health, worker throughput, and alertable failures at an appropriate level.
- Implement only Settings that have an approved backend model and audit trail.
  Environment-managed service configuration is documented rather than edited
  in the browser.

**Completion check**

- Admin routes enforce Auth0 permissions and show a usable unavailable state
  without revealing internal connection details or credentials.
- Monitoring values come from live supported telemetry/API data, not local
  preview constants.
- Any settings mutation is validated, authorized, audited, and covered by an
  OpenAPI/client contract test; otherwise the page remains read-only.

### GAP-PH-05 — DNS and TLS crawl capture

**Status:** Complete (2026-08-05).

**Purpose:** Complete the missing acquisition facts in crawl snapshots while
preserving crawler safety and immutable evidence.

**Scope**

- Add bounded, timeout-configured DNS and TLS/certificate collection after
  the crawler's existing SSRF and redirect safety checks.
- Persist sanitized observations with source, acquisition time, availability
  reason, and snapshot provenance; update public profile presentation only
  for safe fields.
- Extend artifact storage/persistence documentation and add observability for
  collection failures without turning a missing optional signal into a failed
  successful crawl.

**Completion check**

- Unit fixtures cover successful collection, unavailable data, malformed
  values, timeouts, and private-address protections without live network or
  database calls.
- Integration coverage proves snapshots remain immutable and missing DNS/TLS
  is represented as unavailable rather than absent.

### GAP-PH-06 — Immutable reprocessing and version-change history

**Purpose:** Let rule improvements produce new, attributable derived results
without modifying any historic snapshot, detection, or evidence record.

**Scope**

- Introduce an explicit reprocessing run model with rule-version provenance,
  bounded workload controls, idempotency, terminal/dead-letter visibility,
  and audit coverage for admin initiation.
- Add reliable technology-version observations only where the detector has
  deterministic version evidence; otherwise preserve `unknown` rather than
  inferring a version.
- Derive version-change events and expose them as distinct immutable records.

**Completion check**

- A reprocessing run creates new derived records while before/after historical
  records remain byte-for-byte unchanged.
- Tests cover replay/idempotency, bounded retries, partial failure visibility,
  provenance, and positive/negative version-change fixtures.

### GAP-PH-07 — Frontend release hardening

**Status:** Complete (2026-08-05). See [the phase handoff](./phase-handoffs/gap-ph-07-frontend-release-hardening.md).

**Purpose:** Complete FE-12 after the affected routes and visualizations stop
changing.

**Scope**

- Perform route-level accessibility checks for the public routes, admin shell,
  charts, dialogs, filters, loading/error/empty states, focus order, and
  reduced motion.
- Add visual-regression coverage for desktop, tablet, and mobile critical
  paths, including dense data and chart empty states.
- Run and document a performance pass against the repository performance
  budget; address material regressions before release.

**Completion check**

- Accessibility, visual, typecheck, lint, test, and production-build results
  are recorded in the phase handoff.
- The performance budget is either met or has an explicit, approved exception
  with a follow-up owner.

## Audit limits

This report records confirmed gaps from the PRD, the phase tracker, and reachable dashboard code. It is not a substitute for a full release-readiness review of production credentials, deployment infrastructure, load behaviour, or an end-to-end accessibility audit. Those checks should be performed before a public beta.
