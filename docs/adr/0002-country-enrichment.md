# Local GeoLite2 country enrichment

## Status

Accepted

## Context

Public search requires a country filter, but country is not intrinsic domain identity and must be
traceable to the crawl evidence that produced it.

## Decision

- Resolve country only from the public IP addresses already validated for a successful crawl.
- Use a locally managed MaxMind GeoLite2 Country MMDB configured by path and release identifier.
- Persist one immutable country observation per crawl snapshot; derive a domain's current country
  from its newest observation.
- Do not call a third-party GeoIP or WHOIS service during crawling.

## Consequences

Country can be unknown and represents IP location, not incorporation or audience location. MMDB
updates require an explicit release identifier and normal crawl/reindex processing.

## Alternatives considered

- Online GeoIP APIs: rejected because they add per-crawl credentials, cost, and availability risk.
- WHOIS: rejected because privacy redaction and registrant data do not reliably identify hosting location.
