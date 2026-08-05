# Single-host deployment

This is the production procedure for the first Oracle VM deployment. It uses
`infrastructure/compose/docker-compose.prod.yml`; do not deploy the source-mounted development
Compose stack.

## Host preparation

Install Docker Engine with the Compose plugin, copy the repository to `/opt/techatlas`, and create
an access-controlled `/etc/techatlas/production.env` from `.env.production.example` (`chmod 600`).
Set a DNS A/AAAA record for `TECHATLAS_PUBLIC_HOST` to the VM before startup, allow inbound TCP 80
and 443 only, and keep PostgreSQL, Redis, Meilisearch, OTLP, Prometheus, Grafana, and Tempo off the
public firewall. Caddy obtains and renews TLS certificates automatically.

Use unique, secret values for PostgreSQL, Meilisearch, Grafana, and S3 credentials. Configure
Auth0 with callback, logout, web-origin, and CORS entries for `https://$TECHATLAS_PUBLIC_HOST`,
then set the Auth0 issuer, audience, JWKS URL, and public dashboard `VITE_AUTH0_*` build values in
the production environment file. Configure
the S3 bucket as private, TLS-only, encrypted at rest, and scoped to list/get/put/delete only under
the configured `BACKUP_S3_PREFIX`. Cloudflare R2 is the documented default, but any compatible S3
endpoint works.

The worker requires a licensed GeoLite2 Country MMDB at the configured host path and a crawler user
agent/contact appropriate for public collection.

## Release

```bash
cd /opt/techatlas
docker compose --env-file /etc/techatlas/production.env \
  -f infrastructure/compose/docker-compose.prod.yml build --pull
docker compose --env-file /etc/techatlas/production.env \
  -f infrastructure/compose/docker-compose.prod.yml up -d
```

The one-shot `migrate` service must exit successfully before the API, scheduler, and worker start.
Inspect it with `docker compose ... logs migrate`; do not force dependent services to start if it
fails. Confirm `https://$TECHATLAS_PUBLIC_HOST`, the API public endpoints, and the internal
container readiness checks after every release.

Prometheus and Grafana are loopback-only. Use `ssh -L 3003:127.0.0.1:3003 VM_HOST` for Grafana and
`ssh -L 9090:127.0.0.1:9090 VM_HOST` for Prometheus. The production worker defaults to two
concurrent crawls to protect the host and database.

## Rollback

Stop and return to the previously built image set only when the release did not include an
incompatible schema migration. Database migrations are forward-only: restore a tested backup into
an isolated rehearsal first when rollback requires data reversal. Rebuild Meilisearch with the
maintenance CLI after any search-index loss or restoration.

## Daily maintenance timer

Install `infrastructure/systemd/techatlas-backup.service` and
`infrastructure/systemd/techatlas-backup.timer` as root, then run:

```bash
systemctl daemon-reload
systemctl enable --now techatlas-backup.timer
systemctl start techatlas-backup.service
```

The first command verifies S3 credentials and creates a recoverable backup. It must succeed before
the artifact-pruner runs. Monitor timer failures through `systemctl status techatlas-backup.timer`
and `journalctl -u techatlas-backup.service`.
