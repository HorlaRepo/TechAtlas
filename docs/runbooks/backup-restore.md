# Backup and restore rehearsal

The daily maintenance job creates a PostgreSQL custom dump, a complete raw-artifact archive, and a
SHA-256 manifest at `s3://BACKUP_S3_BUCKET/BACKUP_S3_PREFIX/YYYYMMDDTHHMMSSZ/`. It uses the generic
S3 API through rclone; Cloudflare R2 uses its account endpoint and `BACKUP_S3_REGION=auto`.

Backups are retained for 14 days. The worker retains raw artifacts for 30 days. Expired artifact
files are deleted only after backup succeeds and only when no non-expired immutable metadata row
shares their content-addressed location. Metadata remains immutable for provenance.

## Restore rehearsal

Run this at least monthly and after changing backup credentials, archive format, or migrations. It
uses an isolated Compose project and removes only its own temporary volumes on completion:

```bash
cd /opt/techatlas
TECHATLAS_ENV_FILE=/etc/techatlas/production.env \
  bash infrastructure/backup/rehearse-restore.sh YYYYMMDDTHHMMSSZ
```

The script downloads the archive, verifies both checksums before mutation, restores PostgreSQL and
artifacts into the isolated project, applies any newer forward migration, and rebuilds Meilisearch
from PostgreSQL. A success message is the required evidence; record the backup ID, elapsed time,
schema version, restoration result, and any follow-up action in the operations log.

Never run the rehearsal against the production Compose project. Redis is intentionally not restored;
its queue is recoverable and PostgreSQL remains authoritative. Do not delete a backup, a database
volume, or the production artifact volume while investigating a restore failure.
