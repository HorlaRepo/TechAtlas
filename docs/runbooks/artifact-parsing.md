# Artifact parsing

BE-11 provides pure parsing and normalization for captured HTML, response headers, script references, DNS observations, and TLS observations. `techatlas-parser` does not acquire remote data, write storage, persist snapshots, or evaluate technology rules.

Each source reports `Available` or a typed unavailable reason (`NotCaptured`, `Invalid`, or `Unsupported`). Normalized evidence retains its source and field provenance, is sorted and deduplicated, and excludes sensitive response headers. This preserves the distinction between a signal that was unavailable and an available signal with no matching evidence.

## Session handoff

- The worker supplies DNS only from the public addresses validated for the final response. HTTPS probes use the final host as SNI, a bounded address set and timeout, and do not issue another HTTP request.
- A verified TLS handshake is preferred. If verification fails, one metadata-only handshake may record sanitized facts and the public detail explicitly labels the certificate validation failure. Certificate bytes, email attributes, serials, and arbitrary distinguished-name attributes are never persisted.
- DNS/TLS observations are immutable per-snapshot database records. Missing facts are represented by a typed unavailable reason, not by silently omitting the observation.
- The parser remains pure: it only normalizes its inputs for deterministic detection.
