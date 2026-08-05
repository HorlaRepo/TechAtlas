# Search indexing

BE-14 projects active PostgreSQL domain state into the replaceable Meilisearch `techatlas-domains-v1` index. PostgreSQL remains authoritative. Successful crawl transactions coalesce one outbox entry per domain; index delivery is asynchronous, idempotent, and never blocks crawl success.

Workers claim bounded outbox batches, load current domain projections, and upsert documents. Failures retry with exponential delay until `WORKER_INDEX_MAX_ATTEMPTS`; exhausted jobs remain `dead` in `search_index_outbox` with a bounded error summary. Worker logs include pending lag age and dead-job count after each polling cycle.

Run a full replacement from PostgreSQL with `techatlas-cli rebuild-search-index`. The command recreates the index and bulk-loads active domains, so use it after index loss, incompatible settings changes, or drift investigation. It requires `DATABASE_URL`, `MEILISEARCH_URL`, and `MEILI_MASTER_KEY`.

## Session handoff

- Delivered outcome: versioned domain documents, transactional outbox delivery, retry/dead state, lag signals, and explicit full rebuild command.
- Migration: `202608030009_search_index_outbox.sql` adds outbox and index state tables.
- Verification: document translation tests, migration/outbox assertions, queued-crawl enqueue coverage, and workspace format/lint/test checks.
- Operational impact: workers now require `MEILISEARCH_URL` and `MEILI_MASTER_KEY`; tune the `WORKER_INDEX_*` settings for delivery throughput and retry behavior.
- Next unlocked phase: BE-15, public read endpoints, OpenAPI, and generated client updates.
