# CLI commands

`techatlas-cli` owns explicit maintenance operations. It does not start long-running services;
deployment automation invokes its explicit migration and retention commands before dependent
services start.

## `import-csv`

```sh
DATABASE_URL=postgres://user:password@host:5432/database \
  cargo run -p techatlas-cli -- import-csv \
  --file domains.csv \
  --source-name approved-list \
  --initiated-by operator@example.test
```

The CSV must have exactly one `domain` header. The command emits JSON containing the import ID, aggregate counts, and a result for every input row. It returns success when a document is processed, including rows rejected for invalid input or marked duplicate; malformed documents, file failures, configuration failures, and database failures return a non-zero status.

`CLI_DATABASE_CONNECT_TIMEOUT_MS` is an optional positive millisecond timeout for acquiring a PostgreSQL connection; it defaults to `5000`.

## `rebuild-search-index`

```sh
DATABASE_URL=postgres://user:password@host:5432/database \
MEILISEARCH_URL=http://localhost:7700 \
MEILI_MASTER_KEY=replace-with-a-secret \
  cargo run -p techatlas-cli -- rebuild-search-index
```

The command recreates the versioned `techatlas-domains-v1` index, applies its settings, and bulk-indexes every active authoritative PostgreSQL domain projection. It prints the indexed domain count and returns a non-zero status on database or Meilisearch failure.

## Deployment maintenance

The release job runs migrations explicitly before API, scheduler, and worker startup:

```sh
cargo run -p techatlas-cli -- migrate-database
```

The daily maintenance timer removes only expired raw-artifact files whose every immutable database
reference has expired. It requires `DATABASE_URL` and `WORKER_ARTIFACT_ROOT`:

```sh
cargo run -p techatlas-cli -- prune-expired-artifacts
```
