# Local observability stack

## Status

Accepted

## Context

TechAtlas needs diagnosable operational failures before deployment hardening. The PRD requires
structured logs, OpenTelemetry, Prometheus, Grafana, and observable crawler, queue, index, and
dependency health.

## Decision

- Services emit JSON logs and bounded Prometheus metrics at `/metrics`.
- Traces use optional OTLP/gRPC export through an OpenTelemetry Collector into Tempo.
- Local monitoring uses Prometheus for pull scraping, Tempo for trace storage, and provisioned
  Grafana data sources and an operations dashboard.
- Prometheus alert rules are shipped without a notification receiver; external routing belongs to
  the deployment phase.
- The monitoring stack and scheduler/worker remain Compose profiles to keep the normal local
  development path lightweight.

## Consequences

Operators can correlate a metric alert with secret-free logs and traces locally. Metric labels must
remain bounded and must never include identifiers, domains, URLs, tokens, or raw errors.

## Alternatives considered

- Logs only: rejected because it cannot provide alertable aggregate rates or latency/lag views.
- Hosted-only monitoring: rejected because local failure diagnosis would require external
  credentials and network access.
- Always-on worker/scheduler Compose services: rejected because worker startup requires an
  operator-provided GeoLite database.
