# Raw artifact storage

BE-10 stores eligible crawl response bodies outside PostgreSQL. The worker persists only UTF-8 HTML after removing values from credential-, token-, cookie-, secret-, key-, auth-, session-, and common personal-data-bearing form or metadata fields. Request credentials and sensitive response headers are never stored.

`WORKER_ARTIFACT_ROOT` selects the local filesystem root and defaults to `.techatlas/artifacts`. Objects are zstd-compressed and content-addressed at `sha256/<prefix>/<checksum>.zst`; PostgreSQL records the location, uncompressed SHA-256 checksum, byte counts, compression format, and expiry timestamp.

Set `WORKER_ARTIFACT_RETENTION_DAYS` to the approved local retention period (30 days by default).
BE-18 runs `techatlas-cli prune-expired-artifacts` only after the daily off-host backup completes.
It removes a content-addressed file only when every immutable metadata row using that location has
expired; metadata remains for provenance. Never edit artifact files in place: retrieval verifies
their decompressed size and checksum, and altered files are treated as corrupt.

If local artifact storage is unavailable or verification fails, the worker records `artifact_storage_failed` for the crawl attempt and does not create a successful snapshot. Content-addressed writes can leave harmless filesystem orphans if the subsequent database transaction fails; do not delete non-expired objects solely because they are not currently referenced.
