# Scripts

Scripts must be explicit, documented, deterministic, and safe to rerun. Do not hide production-critical behavior in undocumented scripts.

`check-api-client.sh` regenerates the OpenAPI TypeScript client into a temporary directory and compares it with `packages/api-client/src/generated`. It is run through `pnpm api:client:check`.

`setup-geoip.sh` downloads MaxMind GeoLite2 Country to ignored local runtime storage and updates
the untracked worker GeoIP settings. Run it as `MAXMIND_LICENSE_KEY=... pnpm geoip:setup`; the
license key is required only for that command and is neither persisted nor logged.

`test-local-e2e.sh` starts a disposable, namespaced Docker stack, runs migrations, starts a
test-only local RS256 JWT/JWKS fixture, imports the public `example.com` target through
the OIDC-protected admin API, and waits for crawl persistence, deterministic rule observations,
search indexing, public API search, and Prometheus scraping. Run it with
`pnpm test:docker:e2e`. It removes only its `techatlas-e2e` Docker project and test-data volumes
on exit, retaining Cargo caches for faster repeat runs; set `TECHATLAS_E2E_KEEP=1` to preserve
the stack for diagnosis.
