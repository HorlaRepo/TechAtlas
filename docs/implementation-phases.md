# TechAtlas session-sized implementation phases

This roadmap decomposes the canonical [PRD](../TECHATLAS_PRD.md) into independent sessions. A session is complete only when its acceptance check passes, its scope remains limited to the listed outcome, and no later phase is partially implemented.

## Working rules

- Work phases in order within a track; backend phases unlock the frontend phases that consume them.
- A session should produce one demonstrable vertical slice or one bounded foundation change. Split it further if it cannot be completed, reviewed, and verified in one focused coding session.
- Every backend phase includes unit/integration coverage appropriate to its boundary. Every frontend phase includes responsive and accessibility review for the affected views.
- PostgreSQL is authoritative; Redis, Meilisearch, and the UI remain replaceable projections or consumers.
- API-facing phases update OpenAPI and regenerate `packages/api-client` in the same session.

## Foundation and local environment

| ID | Session outcome | Completion check |
| --- | --- | --- |
| FND-01 | **Done:** pnpm/Turborepo workspace, strict dashboard tooling, shared UI package, and quality scripts. | `pnpm lint`, `pnpm typecheck`, `pnpm test`, and `pnpm build` pass. |
| FND-02 | **Done:** local Docker Compose stack for dashboard, PostgreSQL, Redis, and Meilisearch. | `pnpm docker:up`; all backing-service health checks report healthy and the dashboard opens at `/admin/overview`. |
| FND-03 | **Done:** Added pull-request CI for frontend and Rust lint, typecheck, tests, builds, contracts, and generated-client freshness. | A pull request workflow runs all four commands with no local-only dependency. |
| FND-04 | **Done:** Added configuration validation guidance and a documented per-service configuration matrix. | Application startup reports actionable errors for missing/invalid configuration without exposing secrets. |

## Backend phases

| ID | Session outcome | Depends on | Completion check |
| --- | --- | --- | --- |
| BE-01 | **Done:** Cargo workspace, `apps/api`, core crates, typed configuration, and API liveness/readiness endpoints. | FND-02 | API starts in Compose and reports dependency-aware readiness. |
| BE-02 | **Done:** Added forward-only SQLx migrations for domains, sources, imports, crawl policy, crawl attempts, and subsequent immutable workflow records. | BE-01 | A blank PostgreSQL volume migrates successfully and migration validation passes. |
| BE-03 | **Done:** Implemented canonical domain validation/normalization and domain repository/service. | BE-02 | Unit tests cover valid, invalid, duplicate, and normalized domain input. |
| BE-04 | **Done:** Implemented authenticated admin manual domain creation and list/read endpoints with OpenAPI. | BE-03 | API contract and integration tests prove authenticated admin writes and public-safe reads. |
| BE-05 | **Done:** Implemented CSV import parsing, row validation, duplicate behavior, and persisted import results. | BE-03 | A mixed valid/invalid/duplicate fixture returns traceable per-row outcomes. |
| BE-06 | **Done:** Defined versioned crawl-job messages and Redis Streams producer/consumer adapters. | BE-01 | Tests publish, consume, acknowledge, dead-letter, and safely replay idempotent messages. |
| BE-07 | **Done:** Implemented scheduler eligibility, priority/frequency handling, bounded retry policy, and idempotency keys. | BE-02, BE-06 | Eligible domains produce one correct job; ineligible domains produce none. |
| BE-08 | **Done:** Built crawler URL normalization, robots enforcement, SSRF/private-address blocking, redirect checks, timeouts, and body limits. | BE-01 | Fixture-based tests reject unsafe targets and accept an allowed public target. |
| BE-09 | **Done:** Implemented worker acquisition and immutable crawl-attempt/snapshot metadata persistence. | BE-06, BE-08 | A queued fixture crawl produces one terminal attempt and one successful snapshot record. |
| BE-10 | **Done:** Added compressed raw-artifact storage and the local development implementation. | BE-09 | Stored artifact checksum, location, retrieval, and sensitive-field redaction are verified. |
| BE-11 | **Done:** Implemented pure HTML/header/script/DNS/TLS artifact parsing with fixtures. | BE-10 | Parsers yield normalized evidence and gracefully classify unavailable signals. |
| BE-12 | **Done:** Implemented versioned deterministic detection for the initial small rule catalogue. | BE-11 | Positive, negative, and regression fixtures persist confidence, method, evidence, and rule version. |
| BE-13 | **Done:** Built current-state and technology-change projections from immutable snapshots. | BE-12 | Consecutive snapshots yield correct additions, removals, and current stack. |
| BE-14 | **Done:** Added Meilisearch documents, indexing job, rebuild command, and index-lag telemetry. | BE-13 | A domain projection is searchable after indexing and is rebuildable from PostgreSQL. |
| BE-15 | **Done:** Implemented versioned public domain, technology, search, comparison, and analytics read endpoints. | BE-13, BE-14 | OpenAPI/client, pagination, filter validation, and country enrichment are implemented. |
| BE-16 | **Done:** Auth0 administrator authentication, permission authorization, immutable audit events, and protected operations endpoints. | BE-04, BE-05, BE-07 | Unauthenticated requests fail; permitted admin mutations create audit events. |
| BE-17 | **Done:** Added structured telemetry, metrics, tracing, Prometheus/Grafana/Tempo profiles, alerts, and operational runbooks. | BE-09 onward | Prometheus can scrape required signals and a simulated failure is diagnosable. |
| BE-18 | **Done:** Hardened single-host deployment with release migrations, S3-compatible backup/restore rehearsal, resource limits, retention, and Caddy/API service wiring. | BE-15, BE-17 | A clean host deployment and documented restore rehearsal complete successfully. |

## Frontend phases

| ID | Session outcome | Depends on | Completion check |
| --- | --- | --- | --- |
| FE-01 | **Done:** responsive Operations Center shell, Stitch-based design tokens, Geist typography, Phosphor iconography, and typed dummy data. | FND-01 | Dashboard passes quality checks and works at desktop, tablet, and mobile widths. |
| FE-02 | **Done:** Replaced dashboard dummy data with generated API-client query hooks and a protected live operations overview contract. | BE-15 | Dashboard renders API data and includes loading, empty, unavailable, and error states. |
| FE-03 | **Done:** Built public domain search with URL-backed query/filter/sort state, pagination, and responsive table/card results. | BE-15 | A shared search URL reproduces the exact visible result state. |
| FE-04 | **Done:** Added live search facet counts, immediate URL-backed filters, and removable applied-filter chips for technology, category, country, recency, and confidence. | BE-15 | Filters combine correctly, can be removed individually, and remain keyboard accessible. |
| FE-05 | **Done:** Added the typed public domain-profile route with identity, crawl state, current stack, confidence, and expandable redacted evidence. | BE-15 | A domain route displays current data and clearly distinguishes unknown from absent. |
| FE-06 | **Done:** Added typed domain/crawl history, response/network metadata, and scheduler-mediated refresh requests. | BE-15, BE-16 | Timeline/change and refresh states are readable, responsive, and policy-aware. |
| FE-07 | **Done:** Built the typed technology library with category/trend filters, URL-backed grid/list display, and 30-day net-change trends. | BE-15 | Technology browsing and filter state are URL-addressable and accessible. |
| FE-08 | **Done:** Built the typed technology profile with adoption, 30-day history, related technologies, and paginated domain panels. | BE-15 | Profile content handles sparse data, loading, and empty history correctly. |
| FE-09 | **Done:** Built URL-addressable multi-domain comparison with manual/search selection and a normalized current/stale/unknown category matrix. | BE-15 | Two or more valid domains render a clear comparable matrix. |
| FE-10 | **Done:** Built public analytics, commercial-provider rankings/profiles, URL-backed change windows, and typed aggregate contracts. | BE-15 | Charts include text summaries, accessible labels, and responsive fallback displays. |
| FE-11 | **Done:** Built authenticated admin import, domain policy, scheduler, queue, worker, audit, retry, and rule-management pages. | BE-16 | Every action handles authorization, confirmation, error, and optimistic/refetch state safely. |
| FE-12 | **Done:** Completed route-level accessibility checks, tracked visual-regression coverage, and the production performance pass. | FE-02 through FE-11 | Critical journeys meet WCAG 2.2 AA checks and production build budgets are recorded. |

## Suggested handoff order

1. Start the local stack with FND-02, then complete BE-01 through BE-05 to establish reliable corpus management.
2. Complete BE-06 through BE-13 before building public intelligence screens; this preserves the immutable-history model from the start.
3. Complete BE-14 and BE-15, then begin FE-02 through FE-10 in small page-level sessions.
4. Complete BE-16 and FE-11 together for the protected operational experience.
5. Finish BE-17, BE-18, and FE-12 before a public beta.

## Phase handoff template

Each completed session should record: delivered outcome, affected contract/migration, verification commands and results, configuration additions, operational impact, and the exact next unlocked phase. This keeps one-session phases independently reviewable and safe for external contributors.
