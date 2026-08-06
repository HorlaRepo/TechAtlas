# Infrastructure

This directory contains reproducible Docker, Compose, Caddy, Prometheus, and Grafana configuration. Infrastructure changes require documented validation, resource limits, secure configuration, and an operational rollback path.

The current local development stack is documented in [compose/README.md](./compose/README.md).
The local monitoring and incident procedure is documented in the [monitoring runbook](../docs/runbooks/monitoring.md).
The Oracle VM production procedure and S3-compatible recovery rehearsal are documented in the
[deployment](../docs/runbooks/deployment.md) and [backup/restore](../docs/runbooks/backup-restore.md)
runbooks. The production edge uses the Cloudflare Tunnel templates in `cloudflared/` and `systemd/`;
its credentials and rendered configuration remain host-only.

Production Compose consumes immutable ARM64 image archives built by GitHub Actions rather than
building on the VM or pulling from a registry. The CI-managed `/etc/techatlas/release.env` provides
the required image references; it is separate from the host-only secret environment file.
