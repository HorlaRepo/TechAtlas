# Single-host deployment and S3-compatible backups

## Status

Accepted

## Context

TechAtlas v1 deploys to one Oracle VM and PostgreSQL plus the raw-artifact volume are the
authoritative recoverable data. The PRD requires a release migration step, bounded resources,
retention, and a tested off-host restore procedure without binding the deployment to one storage
provider.

## Decision

- Production runs through a dedicated Docker Compose stack with release images, a loopback-only
  Caddy origin behind Cloudflare Tunnel (see ADR 0011), internal-only service networking, and
  explicit resource limits for the 2-vCPU/12-GB host.
- A one-shot CLI migration service completes before API, scheduler, and worker startup. Service
  processes never apply migrations themselves.
- PostgreSQL and the raw-artifact volume are backed up daily to an S3-compatible private bucket.
  Cloudflare R2 is the documented initial provider; endpoint, region, bucket, and credentials are
  configured through generic S3 variables.
- Backup archives are retained for 14 days. Raw artifact files are retained for 30 days and are
  pruned only after a successful backup when every immutable reference to a shared object expired.
- Redis and Meilisearch are not backed up: Redis is a recoverable queue and Meilisearch is rebuilt
  from PostgreSQL after restore.

## Consequences

Deployments require a Cloudflare-managed hostname and tunnel, a private S3-compatible bucket,
production secrets, and a documented restore rehearsal. Cloudflare reaches the loopback-only
Caddy origin; observability interfaces are bound to loopback for SSH-tunnel access.

## Alternatives considered

- Host-local dumps only: rejected because VM loss would also lose the backup.
- Provider-specific R2 tooling: rejected because the S3 API keeps the recovery path portable.
- Application-startup migrations: rejected because a partially upgraded multi-process deployment
  can serve against an incompatible schema.
