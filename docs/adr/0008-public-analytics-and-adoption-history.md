# Public analytics and adoption history

## Status

Accepted

## Context

Public analytics must support intelligence research without revealing operational health or
capacity information. The current adoption endpoint represents a present-time count and cannot
support an adoption-over-time view.

## Decision

- Keep public analytics limited to intelligence aggregates: adoption, rankings, technology and
  provider movements, changes, migrations, and domain discovery aggregates.
- Keep queue health, worker throughput, dependency readiness, and detection-success telemetry in
  authenticated administrator operations views and the observability stack.
- Model adoption history as a rebuildable projection containing one UTC daily, distinct-domain
  adoption count for each technology. It is derived from immutable observations, retained
  indefinitely for v1, and may be rebuilt without changing crawl or detection history.
- Public history responses distinguish unavailable or insufficient history from a measured zero;
  later chart controls use bounded time windows.

## Consequences

The public analytics contract can grow without disclosing deployment health. GAP-PH-03 must add
the projection, a bounded read path, rebuild procedure, and accessible chart summaries. The
existing protected Operations Center remains the source of operational visibility.

## Alternatives considered

- Exposing aggregate operational telemetry publicly: rejected because small corpus counts can
  reveal capacity and outage details without improving research workflows.
- Computing every chart from immutable snapshots at request time: rejected because a bounded,
  predictable public read model is required as history grows.
- Monthly adoption totals: rejected because they obscure useful short-term adoption movements.
