#!/usr/bin/env bash
set -euo pipefail

backup_id="${1:-}"
if ! [[ "${backup_id}" =~ ^[0-9]{8}T[0-9]{6}Z$ ]]; then
    echo "usage: rehearse-restore.sh YYYYMMDDTHHMMSSZ" >&2
    exit 64
fi

compose_file="infrastructure/compose/docker-compose.prod.yml"
environment_file="${TECHATLAS_ENV_FILE:-/etc/techatlas/production.env}"
if [[ ! -r "${environment_file}" ]]; then
    echo "unable to read deployment environment file: ${environment_file}" >&2
    exit 66
fi

project="techatlas-restore-${backup_id,,}"
compose=(docker compose --env-file "${environment_file}" --project-name "${project}" -f "${compose_file}" --profile maintenance)
cleanup() {
    "${compose[@]}" down --volumes --remove-orphans
}
trap cleanup EXIT

"${compose[@]}" up --detach postgres meilisearch
"${compose[@]}" run --rm --no-deps restore /backup/restore.sh "${backup_id}"
"${compose[@]}" run --rm --no-deps maintenance-cli migrate-database
"${compose[@]}" run --rm --no-deps maintenance-cli rebuild-search-index
echo "restore rehearsal completed successfully for ${backup_id}"
