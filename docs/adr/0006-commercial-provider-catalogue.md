# Commercial provider catalogue

## Status

Accepted

## Context

Public analytics needs provider-level adoption and movement rankings, but the technology catalogue
previously represented only technologies and their categories. A provider may own multiple
technologies, while community or otherwise unclassified technologies must remain valid.

## Decision

- Providers are normalized commercial operators with stable slug and display name.
- A technology may have zero or one provider through an immutable mapping table.
- Provider adoption counts distinct active domains across all mapped current technologies.
- Provider movement aggregates additions, removals, and migrations for mapped technologies.
- Initial mappings identify Vercel as the commercial operator for Next.js and map Stripe,
  Cloudflare, PostHog, and Shopify to themselves.

## Consequences

Provider analytics and public profiles are available without changing detection history or
requiring every technology to have a provider. Future catalogue changes require an explicit
forward-only migration or a separately designed managed-catalogue workflow.

## Alternatives considered

- Provider name stored directly on technologies: rejected because it would mix optional provider
  identity into the immutable technology catalogue and make shared providers harder to query.
- Required provider assignment: rejected because many technologies have no clear commercial
  operator.
- Multi-provider mapping: rejected for v1 because the catalogue has no evidence model for
  allocating adoption or movement between multiple providers.
