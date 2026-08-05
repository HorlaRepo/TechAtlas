# Configuration matrix

Configuration is environment based. Validation errors name only the missing or invalid variable; values, URLs with credentials, tokens, and keys are never written to logs or API responses.

| Service | Required configuration | Optional configuration / safe defaults |
| --- | --- | --- |
| API | `DATABASE_URL`, `REDIS_URL`, `MEILISEARCH_URL`, `MEILI_MASTER_KEY`, `ADMIN_OIDC_ISSUER`, `ADMIN_OIDC_AUDIENCE`, `ADMIN_OIDC_JWKS_URL` | `API_BIND_ADDRESS=0.0.0.0:3000`, dependency timeout, admin rate limits, public freshness and refresh limits, `RUST_LOG` |
| Scheduler | `DATABASE_URL`, `REDIS_URL` | bind address, polling/batch limits, operation timeout, crawl and outbox retry policy, `RUST_LOG` |
| Worker | `DATABASE_URL`, `REDIS_URL`, `MEILISEARCH_URL`, `MEILI_MASTER_KEY` | consumer identity/region, queue and politeness limits, crawler limits, artifact retention, retry/index tuning, optional GeoIP database, `RUST_LOG` |
| Dashboard | `VITE_AUTH0_DOMAIN`, `VITE_AUTH0_CLIENT_ID`, `VITE_AUTH0_AUDIENCE` | `VITE_API_PROXY_TARGET` for Compose deployments and optional `VITE_ADMIN_OBSERVABILITY_URL` for the protected Monitoring deep link; these are public build-time configuration |
| CLI | `DATABASE_URL` for import/migration/retention; `MEILISEARCH_URL` and `MEILI_MASTER_KEY` for index rebuild | `CLI_DATABASE_CONNECT_TIMEOUT_MS=5000`; `WORKER_ARTIFACT_ROOT` is required only for retention pruning |

`WORKER_GEOIP_DATABASE_PATH` is a host path in local development; Compose mounts it read-only into
the worker. Use `MAXMIND_LICENSE_KEY=... pnpm geoip:setup` to populate the path and matching
`WORKER_GEOIP_DATABASE_VERSION` without retaining the license key.

`WORKER_CRAWLER_DNS_TIMEOUT_MS`, `WORKER_CRAWLER_DNS_MAX_ADDRESSES`,
`WORKER_CRAWLER_TLS_TIMEOUT_MS`, and `WORKER_CRAWLER_TLS_MAX_ADDRESSES` bound optional DNS and TLS fact collection after a successful
HTTP acquisition. TLS metadata is collected only from addresses already validated as public crawl
targets; it never contains certificate bytes, credentials, or full distinguished names.

## Validation

The API, scheduler, and worker validate their own process configuration before opening a listener or processing work. Validate a local API configuration without connecting to services by running its configuration-focused unit tests:

```bash
cargo test -p techatlas-common
```

For a full local stack, copy `.env.production.example` only as a reference, set local non-production values in an uncommitted `.env`, then run:

```bash
pnpm docker:up
curl --fail http://localhost:3000/readyz
```

Production Compose intentionally fails interpolation for required deployment secrets and Auth0 dashboard build configuration. Do not place credentials or access tokens in tracked environment files.

`VITE_ADMIN_OBSERVABILITY_URL` must be an absolute credential-free `http` or `https` URL. It is shown only to authenticated dashboard users, but it is still bundled into client assets; point it at an independently protected admin observability endpoint, never a URL containing credentials.
