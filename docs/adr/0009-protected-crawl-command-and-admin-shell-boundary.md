# Protected crawl command and admin shell boundary

## Status

Accepted

## Context

The administrator shell presents a Quick Crawl control that currently performs no operation.
Administrators need a permitted refresh path, but the scheduler remains the sole owner of normal
crawl-job creation and crawler safeguards must never be bypassed.

## Decision

- Quick Crawl accepts only an existing, active domain with an enabled crawl policy. It requires
  `admin:operate` and uses the existing per-subject mutation rate limit.
- The command atomically makes the policy eligible at the current UTC time and records an
  immutable `domain.crawl_requested` audit event. It does not publish directly to Redis or create
  a crawl attempt; the scheduler reserves and publishes the work in its normal cycle.
- Requests for unknown domains return not found and disabled policies return conflict. Existing
  queued or running work remains the scheduler's idempotency boundary.
- Runtime configuration remains environment-managed. Monitoring remains in the protected
  Operations Center and Grafana/Prometheus; browser-managed Settings and a separate Monitoring
  surface are deferred to their dedicated gap phase.
- The protected shell shows only controls with an implemented destination or command. Public
  technology catalogue navigation is linked explicitly; unavailable analytics, notifications,
  help, and settings controls are omitted.

## Consequences

The API gains a protected, auditable crawl-eligibility command and the dashboard can provide an
honest Quick Crawl dialog. The normal scheduler, robots, SSRF, politeness, retry, and queue
semantics remain unchanged.

## Alternatives considered

- Reusing the anonymous public refresh endpoint: rejected because it lacks administrator audit
  attribution and uses the public cooldown policy.
- Directly enqueueing a crawl from the API: rejected because it bypasses scheduler ownership and
  weakens idempotency control.
- Creating unknown domains from the global shell: rejected because corpus membership remains an
  explicit domain-management action.
