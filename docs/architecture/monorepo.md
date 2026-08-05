# Monorepo architecture

TechAtlas is one product with several deployable processes and shared contracts. Cargo workspaces manage Rust modules, pnpm workspaces manage the dashboard/packages, and Turborepo will orchestrate cross-language tasks when executable projects are bootstrapped.

## Ownership flow

    dashboard -> generated API client -> API contract -> API
                                             |
    scheduler -> queue <- worker -> crawler/parser/detector
                                             |
                  database/search/storage/telemetry adapters

Applications compose dependencies. Domain models and pure logic do not depend on applications or infrastructure.

## Rules

- Public API contracts are generated from the Rust API and consumed by generated TypeScript client code.
- PostgreSQL is the source of truth; Meilisearch is a rebuildable projection.
- Crawl jobs are at-least-once messages and handlers must be idempotent.
- Raw snapshots and detections are immutable; current state is a derived projection.
- Shared UI remains presentational and feature-independent.
- New cross-cutting or irreversible choices require an ADR.

