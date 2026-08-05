# TechAtlas — Canonical Product Requirements Document

> **Version:** 1.0.0  
> **Status:** Foundation / implementation-ready  
> **Last updated:** 2026-08-03  
> **Canonical source:** This document supersedes `VISION.md` and `Extras.md` for product, architecture, and delivery decisions. The source files remain as context only.

## 1. Executive summary

TechAtlas is an internet technology intelligence platform. It continuously collects publicly available technical signals from websites, transforms them into explainable technology detections, preserves immutable crawl history, and makes the resulting intelligence searchable through a public web experience and REST API.

TechAtlas is not an on-demand scanner. Crawling is the collection mechanism; the durable, queryable intelligence database is the product.

### 1.1 Product promise

Users can answer questions such as:

- Which domains use Next.js, Stripe, Cloudflare, or PostHog?
- Which domains added, removed, or migrated a technology in a selected period?
- What stack does a domain use today, and what evidence supports each detection?
- Which technologies or providers are growing or declining in the indexed corpus?

### 1.2 Goals

1. Deliver a production-minded intelligence platform that runs on one Oracle Cloud Free Tier VM in v1.
2. Build a continuously growing corpus from administrator-managed domain sources.
3. Preserve an immutable record of successful crawls and normalized detections.
4. Keep detection deterministic, auditable, and explainable.
5. Keep public research frictionless while protecting operational controls.
6. Establish service boundaries that scale horizontally without a rewrite.
7. Provide a contribution-ready monorepo with explicit contracts, quality gates, and operations.

### 1.3 Success measures

| Area | Initial measurable outcome |
| --- | --- |
| Collection | Eligible domains are crawled on configured cadence with retry/failure visibility. |
| Intelligence | Every published detection includes technology, category, confidence, method, rule version, and evidence. |
| History | A successful crawl never overwrites a prior crawl or detection set. |
| Discovery | Users can search/filter domains and technologies and open profiles. |
| Operations | Admins can import domains, inspect queue/worker health, manage schedules, and trigger permitted actions. |
| Quality | CI enforces formatting, linting, tests, contract validation, and builds. |

## 2. Scope and product decisions

### 2.1 In scope for v1

- Public domain, technology, search, comparison, analytics, and information pages.
- Administrator-only operations under `/admin`.
- CSV, public-list, and manual domain ingestion.
- Polite HTTP collection of public web resources: redirects, response metadata, HTML, headers, script references, cookies, DNS, TLS, meta tags, and favicon hashes when available.
- Deterministic rules-based fingerprinting with stored evidence and confidence.
- Immutable crawl snapshots and normalized technology history.
- PostgreSQL persistence, Redis Streams queueing, Meilisearch indexing, raw artifact storage, and observability.
- A REST API documented through OpenAPI and a generated TypeScript client.
- Docker Compose deployment for the initial single-host target.

### 2.2 Explicitly out of scope for v1

- Browser rendering, JavaScript execution, Playwright, screenshots, or CAPTCHA/paywall bypassing.
- AI/LLM-based detection or opaque classification.
- Public accounts, multi-tenancy, billing, paid plans, and public API keys.
- Kubernetes, multi-region deployment, and distributed database/search clusters.
- Automated discovery through search engines, backlinks, or third-party APIs.
- Security scanning, vulnerability scoring, or security claims about domains.

### 2.3 Deferred opportunities

Future work may include browser rendering, screenshots, public API/API keys, accounts, premium datasets, GraphQL, notifications, natural-language discovery, structured AI summaries, trend forecasting, and multi-node deployment. Deferred AI work consumes structured intelligence; it never replaces deterministic detection.

### 2.4 Resolved decisions

| Topic | Decision |
| --- | --- |
| Authentication | Public product pages are account-free. v1 authenticates and authorizes only `/admin/*` and admin API operations. |
| Detection | Deterministic rules only; AI is excluded from the detection pipeline. |
| Deployment | One Oracle Free Tier VM first; services remain horizontally scalable by contract. |
| Repository | One Rust + TypeScript monorepo using Cargo workspaces, pnpm workspaces, and Turborepo. |
| Contracts | Rust HTTP models generate OpenAPI; TypeScript API types/client are generated and never hand-duplicated. |

## 3. Users and jobs to be done

### 3.1 Public researchers

Developers, recruiters, sales teams, investors, agencies, and security researchers can browse without an account.

They need to find domains using selected technologies, understand a domain's present and historical stack, compare stacks, explore adoption trends, and request a refresh within published limits.

### 3.2 Administrators

Administrators operate collection and the platform. They import/manage domains, configure schedules/priorities, inspect jobs, maintain detection rules, and observe health. Admin actions are authenticated, authorized, and auditable.

## 4. Product experience

### 4.1 Public information architecture

| Route | Purpose | Required v1 capability |
| --- | --- | --- |
| `/` | Product landing/discovery | Global search, headline metrics, discoveries/trends, navigation. |
| `/search` | Domain intelligence search | Text query, facets, filters, sort, pagination, shareable URL state. |
| `/domains` | Domain catalogue | Browse/filter and link to profiles. |
| `/domains/:domain` | Domain profile | Current stack, evidence, metadata, timeline, crawls, refresh request. |
| `/technologies` | Technology library | Categories, trend state, filters, grid/list. |
| `/technologies/:slug` | Technology profile | Metadata, adoption, trends, related technologies/domains. |
| `/compare` | Domain comparison | Compare two or more valid domains by normalized category. |
| `/analytics` | Intelligence analytics | Adoption, growth/decline, changes, top categories/providers. |
| `/about` | Product transparency | Methodology, sources, policies, evidence model. |

Public UI must distinguish unavailable data from the absence of a technology.

### 4.2 Domain profile

The domain profile is the core intelligence surface. It displays canonical identity, status, first-indexed and last-crawled time, current stack grouped by category, detection confidence/method/evidence/last observed time, historical additions/removals/migrations/version changes, crawl history, response headers, redirect chain, DNS, TLS/certificate facts, HTML metadata, script references, and known analytics/payment/CDN/hosting classifications.

A refresh action is transparent, rate-limited, and scheduler-mediated.

### 4.3 Search and discovery

Search supports text matching plus combinable filters for technology, category, country when known, crawl recency, and confidence. Results show query time, total/result estimate, applied filters, sort, and pagination. URLs encode search state for sharing.

### 4.4 Comparison

Users compare two or more domains. The comparison presents a normalized category matrix for frontend, backend, CMS, CDN, analytics, payment, and hosting. It clearly marks common, unique, unknown, and stale data, with source profile links and observation time.

### 4.5 Analytics

Analytics prioritizes intelligence over crawler statistics: popular technologies by category, growth/decline, adoption over time, top countries where available, recent stack changes, large migrations, newest/frequently crawled domains, detection success rate, worker throughput, and queue health. Public operational views only expose safe aggregates.

### 4.6 Administrator experience

The protected application provides Overview, Domains, Imports, Queue, Workers, Scheduler, Detection Rules, Monitoring, and Settings.

Administrators can:

- Import CSV files with validation errors, deduplication, source attribution, and per-row result.
- Add/update/archive domains and configure crawl eligibility, priority, and frequency.
- Trigger a permitted crawl/refresh, inspect job state, and safely retry eligible failures.
- View scheduler decisions, queue depth/age, worker heartbeat/throughput, and crawl/detection outcomes.
- Create, version, test, activate/deactivate, and roll back deterministic rules.
- Review audit records for sensitive operational actions.

## 5. Functional requirements

### 5.1 Ingestion

- **FR-ING-001:** Accept validated CSV imports with a domain column and optional source, priority, country, and crawl-frequency fields.
- **FR-ING-002:** Normalize domains (case, scheme/path removal, IDN support where available) and prevent duplicate active canonical domains.
- **FR-ING-003:** Record source, importer, time, row result, and validation failures; never silently drop rows.
- **FR-ING-004:** Support manual domain entry and explicit adapters for approved public lists.

### 5.2 Scheduling and queueing

- **FR-SCH-001:** Each domain has eligibility, priority, desired frequency, last/next crawl, retry count, and failure state.
- **FR-SCH-002:** Only the scheduler creates normal crawl jobs. Workers consume jobs and do not make scheduling decisions.
- **FR-SCH-003:** Defaults support high (24h), medium (7d), and low (30d) priority, configurable per domain/source.
- **FR-SCH-004:** Failed jobs use bounded retries and exponential backoff with visible terminal failure. Delivery is at-least-once; processing is idempotent.
- **FR-SCH-005:** Public refresh requests are validated, rate-limited, recorded, and routed through the scheduler; no direct unrestricted enqueueing.

### 5.3 Crawling

- **FR-CRW-001:** Workers use an HTTP client with explicit connect/read/total timeouts, redirect limits, response-size limits, user agent, and cancellation.
- **FR-CRW-002:** Workers honor robots.txt and crawl policy, enforce per-domain politeness, and never attempt authentication, CAPTCHA/paywall bypass, or private-network access.
- **FR-CRW-003:** Capture, where obtainable, final URL, redirects, timings, status, headers, cookies, HTML, metadata, script references, DNS, TLS/certificate facts, and favicon hash.
- **FR-CRW-004:** Persist every crawl outcome with normalized failure classification. Only successful acquisitions create successful immutable snapshots.

### 5.4 Snapshot, detection, and history

- **FR-DAT-001:** Store raw artifacts compressed and integrity-verifiable outside relational rows. PostgreSQL stores metadata, checksum, storage location, and retention state.
- **FR-DAT-002:** Every successful crawl creates an immutable snapshot. Existing snapshots, raw artifacts, detections, and evidence are never edited in place.
- **FR-DET-001:** Deterministic rules emit technology, category, confidence, method, rule version, and redacted evidence.
- **FR-DET-002:** Deduplicate identical results within a snapshot without losing provenance. Missing signals do not prove technology absence.
- **FR-DET-003:** Rule changes apply forward. Historical reprocessing creates an explicit derived run/version and preserves original results.
- **FR-DET-004:** Calculate material changes between successful snapshots: additions, removals, migrations, and reliable version changes.

### 5.5 API and indexing

- **FR-API-001:** Expose versioned, read-only public endpoints for search, domains, technologies, comparison, and aggregate analytics.
- **FR-API-002:** Namespace admin endpoints; protect them with authentication, authorization, suitable rate limits, and audit events.
- **FR-SRC-001:** PostgreSQL is the source of truth. Meilisearch is a rebuildable read index, never the sole business record.
- **FR-SRC-002:** Index updates are asynchronous, retryable, observable, and idempotent. UI tolerates documented index freshness lag.

## 6. Data model and invariants

| Entity | Responsibility | Invariant |
| --- | --- | --- |
| `domain` | Canonical crawl target and policy | One active canonical identity per domain. |
| `domain_source` / `import` | Corpus provenance | Traceable to source and row. |
| `crawl_job` | Scheduled work | Idempotency key prevents duplicate logical work. |
| `crawl` | Attempt/outcome | Never overwritten after terminalization. |
| `snapshot` | Successful immutable capture | References exact raw artifact checksums/metadata. |
| `raw_artifact` | HTML/network artifacts | Integrity and access policy are explicit. |
| `technology` / `category` | Normalized catalogue | Stable slug/ID; aliases map to canonical technology. |
| `detection` / `evidence` | Explainable per-snapshot intelligence | Includes rule version and bounded/redacted evidence. |
| `technology_change` | Derived historical change | Links prior and current observations. |
| `detection_rule` / `rule_version` | Rule lifecycle | Published versions are immutable. |
| `refresh_request` | Public refresh intent | Never bypasses scheduler policy. |
| `admin_audit_event` | Sensitive action record | Append-only and attributable. |

Data rules:

- Use UTC and typed IDs everywhere.
- Preserve raw observation separately from normalized/derived state.
- Use a current-state projection for fast reads; derive it from immutable observations.
- Redact secrets, cookies, tokens, personal data, and sensitive headers before storage/exposure.
- Use forward-only reviewed SQLx migrations; never modify an applied migration.
- Treat crawler-originating text as untrusted and sanitize it before rendering.

## 7. Technical architecture

### 7.1 System flow

    Domain sources / admin input / refresh request
                       |
                       v
                  Scheduler
                       |
                       v
              Redis Streams queue
                       |
                       v
             Crawl worker pool -----> raw snapshot storage
                       |
                       v
          Parser + deterministic detector -----> PostgreSQL
                       |
                       v
              Indexing projection -----> Meilisearch
                       |
                       v
                    Axum API
                 /          \
        Public React UI    Admin React UI

### 7.2 Deployable applications

| Application | Responsibility | Scaling contract |
| --- | --- | --- |
| `apps/api` | Axum REST API, public reads, admin commands, OpenAPI | Stateless; shared stores/services. |
| `apps/scheduler` | Eligibility, priority, retries, job publication | Single active leader first; lease/locking supports HA later. |
| `apps/worker` | Queue consumption, safe acquisition, processing handoff | Many identical idempotent consumers. |
| `apps/cli` | Import, rules, backfill, maintenance, development tasks | Explicit commands; never hidden production dependency. |
| `apps/dashboard` | React public/admin user interface | Independently buildable/deployable static application. |

### 7.3 Shared Rust crates

| Crate | Boundary |
| --- | --- |
| `models` | Domain entities, value objects, DTO boundaries; no infrastructure clients. |
| `common` | Small cross-cutting primitives: errors, IDs, time, config interfaces. |
| `database` | SQLx repositories, transactions, migrations integration, projections. |
| `crawler` | HTTP acquisition, robots/politeness, redirect and safety policy. |
| `parser` | Pure extraction/normalization from artifacts. |
| `detector` | Rule schema/evaluation, scoring, evidence production. |
| `queue` | Redis Streams abstractions and message contracts. |
| `storage` | Raw-artifact storage interface/implementations. |
| `search` | Meilisearch documents, indexing, query translation. |
| `telemetry` | Logs, metrics, tracing, health/readiness helpers. |

Dependencies point inward: applications compose crates; adapters depend on models; models never depend on apps, framework, database, Redis, Meilisearch, or HTTP clients. Avoid catch-all utility crates and cycles.

### 7.4 Frontend architecture

The dashboard uses React, TypeScript, Vite, Tailwind CSS, TanStack Router, TanStack Query, and a shared component library.

    apps/dashboard/src/
      app/          providers, router, layouts, bootstrap
      routes/       thin route modules and parameter/search validation
      features/     domain, technology, search, comparison, analytics, admin
      components/   app-level compositions only
      lib/          narrow framework integrations/utilities
    packages/ui/    reusable presentational primitives/design tokens
    packages/api-client/ generated API types/client

Route modules orchestrate; feature modules own feature-specific UI, hooks, state, schemas, and tests. Shared UI is accessible, documented, presentational, and free of business logic. Server state belongs in TanStack Query; shareable URL state belongs in the router; local transient state stays local. Do not add global client state without a documented cross-feature need.

### 7.5 API contract lifecycle

1. Change Rust endpoint models/handlers with validation and tests.
2. Generate and validate `contracts/openapi/openapi.yaml`.
3. Generate `packages/api-client`.
4. Update dashboard calls against generated code.
5. Run contract, API, and frontend checks in CI.

Breaking API changes require an ADR, versioning/migration plan, and explicit consumer updates. Generated output is never hand-edited.

## 8. Technology choices

| Layer | v1 choice |
| --- | --- |
| Backend | Rust, Axum, Tokio, SQLx |
| Database | PostgreSQL |
| Queue | Redis Streams |
| Search | Meilisearch |
| Web | React, TypeScript, Vite, Tailwind CSS, TanStack Router, TanStack Query, shadcn/ui-compatible primitives |
| Observability | OpenTelemetry, Prometheus, Grafana, structured logs |
| Runtime | Docker Compose, Caddy, Oracle Cloud Free Tier VM |
| Monorepo | Cargo workspaces, pnpm workspaces, Turborepo |

A substitution that changes a deployable boundary, datastore, API contract, or developer workflow requires an ADR.

## 9. UX and design system

The Stitch materials under `stitch_techatlas_intelligence_dashboard/` are visual references, not production source code. Translate their intent into reusable components and tokens; do not copy standalone HTML into the application.

### 9.1 Visual language

- Dark-first, calm, engineering-first intelligence UI: authoritative, compact, and legible rather than flashy.
- Deep navy-charcoal canvas `#0F1417`, layered neutral surfaces, soft borders only where same-tone surfaces meet, minimal shadow.
- Teal primary `#2BB6A8` (bright interactive `#5BDACB`) for primary actions, success, and active navigation.
- Purple discovery accent `#8B5CF6` only for discoveries, trends, and insight emphasis.
- Hanken Grotesk for headings, Inter for body/interface, JetBrains Mono for labels, timestamps, and technical values.
- Strict 8px spacing scale; desktop 12 columns/24px gutters/max 1440px; tablet 8 columns/16px; mobile 4 columns/16px.
- Base 8px radius for controls/small cards; 16px containers/modals; pills only for status/discovery tags.

### 9.2 Required primitives

Build and use shared button, input, select/combobox, tooltip, dialog, menu, badge/status, card, data table, empty/loading/error state, pagination, filter chip, metric card, timeline event, chart frame, toast, page header, sidebar, and responsive data-display components. All support keyboard use, visible focus, semantic labels, disabled/loading states, and compact/mobile variants.

### 9.3 Accessibility and responsiveness

Meet WCAG 2.2 AA: semantic structure, keyboard access, focus order/indicator, contrast, screen-reader names, motion reduction, and no color-only status. Test representative public/admin views at desktop/tablet/mobile. Dense data may reflow or use disclosure/cards; it may not be horizontally unusable.

## 10. Security, safety, and compliance

- Validate input at every API boundary; use parameterized queries and least-privilege credentials.
- Enforce authentication, authorization, secure sessions, CSRF protection for cookie-authenticated mutations, security headers, and audit logs.
- Never place secrets in source, logs, fixtures, snapshots, generated clients, or example docs.
- Rate limit APIs and refresh requests; apply stronger admin protections.
- Reject loopback, link-local, private, multicast, unspecified, and metadata-service targets before and after DNS resolution. Bound redirects and response bodies.
- Honor robots.txt, identify crawler/contact, rate-limit per domain, avoid bypass behavior, and provide opt-out/removal policy before public launch.
- Redact sensitive headers/cookies; define artifact retention/deletion policy before production collection.
- Run dependency/security checks in CI; remediate critical findings before release.

## 11. Reliability, performance, and observability

- Every service exposes health/readiness checks appropriate to its dependencies.
- Queue consumers and indexing are idempotent/retryable with bounded backoff and terminal/dead-letter visibility.
- Scheduler leadership, jobs, worker heartbeats, and deployment version are observable.
- Backup/restore verification, retention, and capacity monitoring are runbook requirements.
- Use correlation IDs and structured secret-free logs.

Initial validation targets (not production promises until measured):

| Measure | Target |
| --- | --- |
| Simple public read p95 | Under 500 ms excluding network transit |
| Indexed search p95 | Under 1 s at v1 corpus scale |
| Availability after beta | 99.5% monthly target |
| Required telemetry | Crawl outcome, queue age, detector duration, index lag, DB pool saturation, worker health |

Initial capacity is one 2-vCPU, 12-GB RAM, 200-GB storage Oracle VM. Resource limits, concurrency, response bounds, index batch sizes, retention, alerts, and degradation policies must protect the public API and database.

## 12. Quality strategy

### 12.1 Test pyramid

- Unit: domain rules, parsers, detector scoring, UI utilities/components.
- Integration: API/database, Redis Streams, search projection, raw storage, crawler safety behavior.
- Contract: OpenAPI validation and generated-client compatibility.
- End-to-end: critical public search/domain journey and authenticated admin import/operations journey.
- Non-functional: representative load, migration rehearsal, backup restore, accessibility, and crawler abuse/SSRF tests.

### 12.2 Definition of done

A change is complete only when behavior/acceptance criteria, tests, migrations/generated contracts where needed, telemetry/error handling, UI accessibility/responsiveness, and relevant docs/ADRs are updated; formatting, linting, type, build, and test checks pass.

### 12.3 CI gates

CI runs Rust fmt/clippy/test/build; TypeScript lint/typecheck/test/build; OpenAPI/client drift checks; migration validation; dependency/security checks; and targeted integration/contract tests. No merge with stale generated artifacts or failing gates.

## 13. Repository layout

    apps/             deployable api, scheduler, worker, cli, dashboard
    crates/           focused Rust domain and infrastructure crates
    packages/         TypeScript UI, generated API client, configuration
    contracts/openapi/ API contract artifacts and validation
    database/         forward-only migrations and safe seed assets
    infrastructure/   Compose, Docker, Caddy, Prometheus, Grafana
    docs/             architecture, ADRs, API docs, runbooks
    examples/         seed domains, sample requests/responses, demo data
    tests/            cross-service contract, integration, fixtures
    scripts/          documented developer and CI automation

Contribution rules are defined in `AGENTS.md`; durable architecture decisions live in `docs/architecture/` and `docs/adr/`.

## 14. Delivery roadmap

| Phase | Outcome | Exit criteria |
| --- | --- | --- |
| 0. Foundation | Repository, local environment, contracts, standards, CI skeleton | Boundaries and baseline checks documented. |
| 1. Corpus/crawl | Ingestion, scheduler, safe worker, raw snapshots | Seed data imports and scheduled crawl produces immutable artifacts/outcomes. |
| 2. Detection/data | Rules, catalogue, projections, index | Explainable current/history detections queryable through API. |
| 3. Public intelligence | Search, profiles, library, compare, analytics | Core public journeys responsive, accessible, API-backed. |
| 4. Operations | Admin auth, imports, rules, monitoring | Admin can safely operate collection with auditability. |
| 5. Harden/deploy | Observability, backups, security, Compose/Caddy | Measured VM beta with runbooks and alerts. |

## 15. Acceptance scenarios

1. An admin imports a CSV with valid, duplicate, and invalid domains and receives a traceable per-row result; valid canonical domains become schedulable.
2. Scheduler publishes one idempotent job for an eligible domain; a worker safely captures allowed evidence and creates an immutable successful snapshot.
3. A rule detects Next.js from documented signals; the profile shows confidence, method/rule version, redacted evidence, and observation time.
4. A later snapshot removes Next.js and adds another framework; current state changes while older observations remain queryable and a change event is visible.
5. A public user filters `Technology=Stripe` and `Hosting=Cloudflare`, shares the URL, opens a profile, and requests refresh without bypassing limits.
6. An unauthenticated visitor cannot access admin data/actions; a permitted admin can see safe operational controls and audit state.
7. An API response change flows through OpenAPI and generated TypeScript client; CI fails if either is stale.

## 16. Risks and mitigations

| Risk | Mitigation |
| --- | --- |
| Single-VM exhaustion | Bounded concurrency/queues/bodies, resource limits, retention, metrics, degradation. |
| Legal or target-site harm | Robots/politeness/opt-out, public-data-only policy, user agent/contact, no bypass behavior. |
| False positives | Explainable evidence, rule versioning, thresholds, tests, review/rollback. |
| Queue duplication/failure | Idempotency, acknowledgement discipline, backoff, terminal visibility, reconciliation. |
| Search drift | PostgreSQL authoritative; index jobs observable, retryable, rebuildable. |
| UI inconsistency | Shared tokens/primitives, feature-first composition, visual/a11y review, no copied Stitch HTML. |
| Contribution friction | Clear boundaries, contract flow, AGENTS rules, ADRs, reproducible scripts, CI. |

## 17. Questions to resolve before production launch

- Which public datasets and licences are approved?
- What opt-out/removal process and raw-artifact retention period are appropriate?
- What initial detection-rule catalogue and confidence policy define beta quality?
- Which admin identity/session mechanism and roles are required?
- What backup destination and tested restore RPO/RTO are acceptable?
- What exact public rate, crawl concurrency, and domain-politeness budgets are safe?

These do not block repository foundation, but they block unrestricted production crawling or public launch.

