# Auth0 administrator authorization

## Status

Accepted

## Context

Administrator operations can create domains, import source data, and change crawl eligibility. A
single static shared token cannot identify the actor for audit records, cannot express read-only
access, and makes safe rotation difficult.

## Decision

- Use Auth0 as the administrator identity provider, while retaining configured OpenID Connect
  issuer, audience, and JWKS endpoints at the API boundary.
- Accept only RS256 tokens. Require a non-empty `sub` and map Auth0's standard `permissions`
  claim: `admin:read` permits reads and `admin:operate` permits reads and mutations.
- Cache JWKS keys for five minutes and refresh for unknown key IDs or expired cache entries. Bound
  JWKS retrieval by the normal dependency timeout.
- Apply separate per-subject read and mutation rate limits.
- Write immutable PostgreSQL audit events for domain creation, CSV import, and crawl-policy
  changes. Audit metadata contains counts or policy values, never raw CSV input or credentials.

## Consequences

Deployments must configure Auth0 and keep the issuer, audience, and JWKS endpoint available to
the API. Administrators gain individually attributable audit records and least-privilege read
access. If Auth0 is unavailable after cached keys expire, protected requests return a stable
service-unavailable error instead of bypassing authentication.

## Alternatives considered

- Static shared API token: rejected because it has no per-user attribution or role separation.
- API-managed passwords and sessions: rejected because this adds credential storage and lifecycle
  responsibilities outside the product's current scope.
- Trusting an unverified forwarded identity header: rejected because upstream configuration errors
  could grant administrator access.
