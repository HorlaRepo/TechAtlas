# Automated local GeoLite2 provisioning

## Status

Accepted

## Context

Country enrichment requires a licensed, versioned GeoLite2 Country database. Requiring every
developer to manually download and mount it made the feature easy to omit and left country data
unavailable in local crawls.

## Decision

- Keep country lookup local and source it only from MaxMind GeoLite2 Country, as established by ADR-0002.
- Provide `pnpm geoip:setup`, which accepts `MAXMIND_LICENSE_KEY` only for the running command,
  downloads the database to ignored local storage, and writes only path and release metadata to `.env`.
- Mount the configured database read-only into the local worker. The worker remains functional
  without GeoIP configuration.
- Use an authenticated, audited scheduler-mediated bulk recrawl for successful domains that lack
  country observations; do not mutate snapshots or bypass crawler policy.

## Consequences

Local onboarding gains one explicit MaxMind-dependent setup command. The license key is not
committed, logged, or stored by TechAtlas. Existing country data remains immutable and is populated
only by new successful crawls.

## Alternatives considered

- Manual downloads: rejected because setup was repeatedly missed.
- Online GeoIP lookup during crawling: rejected for credential, cost, and availability reasons.
- Rewriting existing snapshots: rejected because crawl evidence is immutable.
