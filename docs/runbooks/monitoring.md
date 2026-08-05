# Monitoring and incident diagnosis

Start the monitoring stack with the core services:

```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4317 \
COMPOSE_PROFILES=observability pnpm docker:up
```

Prometheus is available at `http://localhost:9090`, Grafana at `http://localhost:3003`, and Tempo
at `http://localhost:3200`. Add the scheduler and worker targets with:

```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4317 \
COMPOSE_PROFILES=observability,pipeline pnpm docker:up
```

The protected dashboard Monitoring page can link to Grafana when
`VITE_ADMIN_OBSERVABILITY_URL` is set at dashboard build time. Use the local Grafana URL above
for development. In production, use an independently authenticated, administrator-only URL; do
not include credentials in the value.

Each service exposes `/healthz`, `/readyz`, and `/metrics`. Metrics contain only bounded labels;
use correlation IDs, trace IDs, and structured logs to investigate a specific job or request. The
default local worker starts without country enrichment; configure a licensed GeoLite Country MMDB
only when testing country-specific behavior.

## Baseline alerts

- `TechAtlasDependencyUnavailable`: inspect `/readyz`, then PostgreSQL, Redis, or Meilisearch
  health and connectivity.
- `TechAtlasSchedulerStalled`: inspect scheduler JSON logs and `scheduler_tick` metrics.
- `TechAtlasIndexLagging`: inspect index dead-job count, worker logs, and the search-index runbook.
- `TechAtlasCrawlFailuresElevated`: inspect crawler outcome logs and traces; do not log remote
  response bodies or sensitive headers.
- `dns_capture` and `tls_capture` operation outcomes distinguish successful optional collection
  from unavailable facts. Investigate sustained unavailable outcomes and their structured reason
  without treating them as failed HTTP crawls.

## Simulated dependency failure

Stop Redis with `docker compose -f infrastructure/compose/docker-compose.dev.yml stop redis`.
Within two scrape intervals, the API `/readyz` response reports Redis unavailable and Prometheus
shows `techatlas_dependency_ready{dependency="redis"} 0`; the dependency alert becomes pending,
then firing after two minutes. Restore Redis with `docker compose -f infrastructure/compose/docker-compose.dev.yml start redis`, confirm readiness returns `200`, and confirm the alert clears. Record the timestamp, impacted service, metric, correlated log/trace, and recovery action in an incident note.
