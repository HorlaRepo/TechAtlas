# Local Docker development environment

This Compose stack runs the current dashboard and the v1 backing services needed by upcoming backend phases.

```bash
cp .env.example .env
pnpm docker:up
```

| Service | Host URL / port | Internal endpoint |
| --- | --- | --- |
| API | http://localhost:3000 | `api:3000` |
| Scheduler (`pipeline`) | http://localhost:3001 | `scheduler:3001` |
| Worker (`pipeline`) | http://localhost:3002 | `worker:3002` |
| Dashboard | http://localhost:5173 | `dashboard:5173` |
| PostgreSQL | `localhost:5433` | `postgres:5432` |
| Redis | `localhost:6380` | `redis:6379` |
| Meilisearch | http://localhost:7700 | `meilisearch:7700` |

The nonstandard PostgreSQL and Redis host ports avoid conflict with common local stacks. The backend must use the internal endpoints when it joins this Compose network. Local credentials in `.env.example` are development-only and must never be used in deployment.

The dashboard is fixed to port `5173` so its Auth0 callback URI remains stable. Configure
`http://localhost:5173` in Auth0's allowed callback, logout, web-origin, and CORS settings before
opening an `/admin/*` route locally.

For the Oracle VM deployment, use the separate release stack at
`docker-compose.prod.yml` and the [deployment runbook](../../docs/runbooks/deployment.md). It has
no source bind mounts or public datastore ports. Production images are built as ARM64 release
archives by GitHub Actions and pinned through `/etc/techatlas/release.env`; the VM does not build
or pull application images during deployment.

The API exposes `GET /healthz` for process liveness and `GET /readyz` for PostgreSQL, Redis, and Meilisearch readiness. The latter returns HTTP 503 when any dependency is unavailable.

Use `pnpm docker:logs` to follow service output and `pnpm docker:down` to stop the stack. Volumes are retained by `docker:down` to preserve local data.

## Observability profiles

Run Prometheus, Grafana, Tempo, and the OpenTelemetry Collector with
`OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4317 COMPOSE_PROFILES=observability pnpm docker:up`.
Prometheus listens on port `9090`, Grafana on `3003`, Tempo on `3200`, and the Collector accepts
local OTLP/gRPC on `4317`. Add the `pipeline` profile to run the scheduler and worker. Set up
optional local country enrichment with `MAXMIND_LICENSE_KEY=... pnpm geoip:setup` before starting
the profile. The command keeps the MMDB and license key out of Git; see the
[country-enrichment runbook](../../docs/runbooks/country-enrichment.md). Production still requires
a mounted, versioned GeoLite2 Country MMDB.
