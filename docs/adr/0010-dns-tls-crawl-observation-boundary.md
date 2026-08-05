# DNS and TLS crawl observation boundary

## Status

Accepted (2026-08-05).

## Context

TechAtlas needs DNS and certificate facts on immutable crawl snapshots, while preserving SSRF
defences and avoiding storage of credentials, certificate bytes, or unnecessary personal data.

## Decision

DNS observations use only the final response host and addresses already validated as public by
the crawler. HTTPS TLS probing connects directly to those addresses with the final host as SNI,
under bounded timeout and address limits. A normal verified handshake is preferred; on failure, a
metadata-only unverified fallback may capture certificate facts. Fallback observations are marked
`validation_failed` publicly.

Persist only sanitized protocol, cipher, hostname-like subject, issuer organization/common name,
DNS SANs, and validity times. Do not retain raw certificate bytes, email attributes, serials, or
arbitrary distinguished-name values. Optional collection failures create unavailable observations
and do not turn a successful HTTP crawl into a failed attempt.

## Consequences

New snapshots carry immutable DNS/TLS provenance and public crawl details can distinguish verified,
validation-failed, unavailable, and non-applicable TLS states. Historic snapshots retain an
explicit `not_captured` public state rather than being backfilled or mutated.

## Alternatives considered

- Capture only verified certificates, which loses useful diagnostics for misconfigured public sites.
- Persist raw certificates or full distinguished names, which expands sensitive-data retention.
- Resolve or probe an unvalidated address separately, which would weaken SSRF protection.
