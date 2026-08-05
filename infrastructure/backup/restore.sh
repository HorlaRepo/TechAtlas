#!/usr/bin/env bash
set -euo pipefail
umask 077

backup_id="${1:-}"
if ! [[ "${backup_id}" =~ ^[0-9]{8}T[0-9]{6}Z$ ]]; then
    echo "usage: restore.sh YYYYMMDDTHHMMSSZ" >&2
    exit 64
fi

required=(DATABASE_URL BACKUP_S3_ENDPOINT BACKUP_S3_BUCKET BACKUP_S3_ACCESS_KEY_ID BACKUP_S3_SECRET_ACCESS_KEY)
for name in "${required[@]}"; do
    if [[ -z "${!name:-}" ]]; then
        echo "${name} is required" >&2
        exit 64
    fi
done

prefix="${BACKUP_S3_PREFIX:-techatlas}"
prefix="${prefix#/}"
prefix="${prefix%/}"
work_dir="$(mktemp -d)"
cleanup() { rm -rf "${work_dir}"; }
trap cleanup EXIT

export RCLONE_CONFIG_TECHATLAS_BACKUP_TYPE=s3
export RCLONE_CONFIG_TECHATLAS_BACKUP_PROVIDER=Other
export RCLONE_CONFIG_TECHATLAS_BACKUP_ENDPOINT="${BACKUP_S3_ENDPOINT}"
export RCLONE_CONFIG_TECHATLAS_BACKUP_ACCESS_KEY_ID="${BACKUP_S3_ACCESS_KEY_ID}"
export RCLONE_CONFIG_TECHATLAS_BACKUP_SECRET_ACCESS_KEY="${BACKUP_S3_SECRET_ACCESS_KEY}"
export RCLONE_CONFIG_TECHATLAS_BACKUP_REGION="${BACKUP_S3_REGION:-auto}"
export RCLONE_CONFIG_TECHATLAS_BACKUP_NO_CHECK_BUCKET=true

rclone copy "techatlas_backup:${BACKUP_S3_BUCKET}/${prefix}/${backup_id}" "${work_dir}" --transfers=1 --checkers=2
(cd "${work_dir}" && sha256sum --check manifest.sha256)
pg_restore --dbname="${DATABASE_URL}" --clean --if-exists --no-owner --no-privileges "${work_dir}/postgres.dump"
tar --extract --gzip --file="${work_dir}/artifacts.tar.gz" --directory=/artifacts
psql "${DATABASE_URL}" --set=ON_ERROR_STOP=1 --command="SELECT count(*) AS raw_artifact_metadata FROM raw_artifacts;"
printf '{"restore_id":"%s","status":"complete"}\n' "${backup_id}"
