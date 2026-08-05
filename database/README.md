# Database assets

- `migrations/`: forward-only SQLx migrations; never edit an applied migration.
- `seeds/`: safe, synthetic, or licensed seed data only.

PostgreSQL is the source of truth. Search indexes and aggregate views must be rebuildable from it.

## Migration validation

Run the migration integration test to start a disposable PostgreSQL 16 container, apply every
migration to its blank database, and verify the initial schema invariants:

```bash
cargo test -p techatlas-database --test migrations
```

The application must not apply migrations automatically. Release and deployment automation should
run the validated migration step before starting applications that depend on a new schema version.
