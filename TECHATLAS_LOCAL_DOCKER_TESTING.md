# TechAtlas local Docker end-to-end testing

## Purpose

Run this check before a production promotion to prove the implemented backend path works together:

```text
OIDC admin import -> PostgreSQL -> scheduler -> Redis Streams -> worker
-> public HTTP acquisition -> immutable artifact and snapshot -> deterministic rules
-> PostgreSQL projection -> Meilisearch -> public API -> Prometheus
```

It is an isolated, disposable local environment. It does not test Oracle VM networking, TLS,
public DNS, backups, or production credentials; those belong to the deployment runbook.

## One-command verification

Prerequisites are Docker Desktop (or Docker Engine with Compose), `curl`, `jq`, and
outbound HTTPS access to `example.com`. The public target is deliberate: the crawler correctly
rejects Docker-private and loopback addresses, so a container-hosted fixture would not exercise a
successful crawl safely.

From the repository root, run:

```bash
pnpm test:docker:e2e
```

The test uses the Docker project name `techatlas-e2e`, removes its test-data volumes before it
starts and again on exit, and never touches another Compose project. It retains only Cargo build
caches for faster repeat runs. Set
`TECHATLAS_E2E_KEEP=1` to retain the failing stack for inspection, or set
`TECHATLAS_E2E_TIMEOUT_SECONDS` to change the 15-minute timeout. Its host ports are isolated
from the normal development defaults (API `13000`, scheduler `13001`, worker `13002`, test OIDC
fixture `18080`, Meilisearch `17700`, Prometheus `19090`, Grafana `13003`, and Tempo `13200`); each has a
corresponding `TECHATLAS_E2E_*_PORT` override for a rare collision.

The first clean run downloads images and compiles the Rust services, so it
can take several minutes before the 15-minute pipeline wait begins. The check intentionally
recreates its data volumes on every run; retained Cargo caches cannot affect application data or
test outcomes.

The command performs these assertions:

- PostgreSQL migrations complete before the API, scheduler, or worker starts.
- A test-only local signer issues an RS256 access token with `admin:operate` through a disposable JWKS endpoint.
- The OIDC-protected CSV-import endpoint accepts `example.com`.
- The scheduler publishes the crawl job and the worker completes it.
- PostgreSQL contains a successful attempt, immutable snapshot, raw artifact, and all five
  deterministic rule observations.
- The worker updates Meilisearch and the public domain/search endpoints return the imported domain.
- Prometheus has healthy scrape targets for the API, scheduler, and worker; Grafana and Tempo are
  reachable.

An empty technology-detection result is valid for this target. The rule-observation assertion is
what proves that deterministic evaluation ran; the test does not make an unstable claim about a
third-party website's technology stack.

## Compose profiles

The development Compose file is `infrastructure/compose/docker-compose.dev.yml`.

| Profile | Services enabled |
| --- | --- |
| default | migration job, API, dashboard, PostgreSQL, Redis, Meilisearch |
| `pipeline` | scheduler and worker |
| `observability` | Prometheus, Grafana, Tempo, OpenTelemetry Collector |
| `test-fixture` | disposable RS256 JWT/JWKS fixture used only by Docker E2E |

For interactive development, create a private local environment file and start only what you need:

```bash
cp .env.example .env
pnpm docker:up
COMPOSE_PROFILES=pipeline,observability pnpm docker:up
```

The E2E command selects all relevant profiles itself and uses `.env.example`; its local signer
creates a new ephemeral key at startup and never writes credentials to the repository.

## Country enrichment

The default local stack starts without a GeoLite2 Country database. That represents unavailable
country evidence correctly: crawls and search still work, but country fields and country filters
have no observations. Production continues to require a licensed, locally managed MMDB and a
source version. Do not add an MMDB to the repository.

## Local observability

The E2E check starts telemetry with `OTEL_EXPORTER_OTLP_ENDPOINT` set to the local Collector. Its
isolated URLs are Prometheus at `http://localhost:19090`, Grafana at `http://localhost:13003`, and
Tempo at `http://localhost:13200`. The test verifies service reachability and Prometheus scrape health;
use Grafana and Tempo to inspect the resulting metrics, logs, and traces during diagnosis.

## Dashboard boundary

The dashboard redirects unauthenticated `/admin/*` visitors to Auth0 Universal Login and sends
Auth0 API access tokens to protected endpoints. The Docker E2E command verifies the API boundary
directly with its local test fixture; it does not exercise an external Auth0 browser login.

## Failure handling

On failure, rerun with preservation enabled:

```bash
TECHATLAS_E2E_KEEP=1 pnpm test:docker:e2e
```

Then inspect the isolated services:

```bash
docker compose --project-name techatlas-e2e \
  --env-file .env.example \
  --file infrastructure/compose/docker-compose.dev.yml \
  --profile pipeline --profile observability --profile test-fixture \
  logs --tail=200
```

Finish by stopping the preserved stack and deleting all of its test-owned data and build caches:

```bash
docker compose --project-name techatlas-e2e \
  --env-file .env.example \
  --file infrastructure/compose/docker-compose.dev.yml \
  --profile pipeline --profile observability --profile test-fixture \
  down --volumes --remove-orphans
```

Run the normal formatter, lint, unit, integration, contract, and frontend checks alongside this
test. Passing local E2E is a promotion gate for the implemented backend path, not a replacement
for production-specific validation.
