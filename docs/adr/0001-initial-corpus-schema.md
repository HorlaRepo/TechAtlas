# Initial corpus schema invariants

## Status

Accepted

## Context

TechAtlas requires a durable PostgreSQL foundation before domain ingestion, scheduling, and crawling
can be implemented. The PRD requires canonical active-domain uniqueness, traceable source/import
provenance, idempotent crawl work, and immutable terminal crawl outcomes.

## Decision

- Use PostgreSQL UUID primary keys generated with `pgcrypto` for corpus records.
- Store normalized canonical domains as lowercase, trimmed text and enforce uniqueness only among
  active records; archival retains historical identity without blocking a later active record.
- Model sources, imports, rows, and domain-source membership separately so each accepted,
  duplicate, or rejected input row remains traceable without duplicating domain records.
- Keep one mutable crawl policy per domain and store individual crawl attempts separately.
  A database trigger rejects updates or deletes of terminal crawl attempts (`succeeded`, `failed`,
  or `cancelled`).

## Consequences

Future domain, import, scheduler, and worker code can rely on database-enforced identity,
provenance, idempotency, and terminal-history invariants. Future schema evolution must use new
forward-only migrations. Application code must redact input/error text before writing the bounded
provenance and failure-summary fields.

## Alternatives considered

- Application-only duplicate and terminal-state checks: rejected because all future writers would
  need to reproduce critical integrity rules.
- A single import JSON payload without row records: rejected because it cannot provide the required
  per-row outcomes and source provenance.
- Mutable crawl records: rejected because the product requires an immutable crawl history.
