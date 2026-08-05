#!/usr/bin/env bash
set -euo pipefail
umask 077

required=(DATABASE_URL BACKUP_S3_ENDPOINT BACKUP_S3_BUCKET BACKUP_S3_ACCESS_KEY_ID BACKUP_S3_SECRET_ACCESS_KEY)
for name in "${required[@]}"; do
    if [[ -z "${!name:-}" ]]; then
        echo "${name} is required" >&2
        exit 64
    fi
done

backup_id="$(date -u +%Y%m%dT%H%M%SZ)"
prefix="${BACKUP_S3_PREFIX:-techatlas}"
prefix="${prefix#/}"
prefix="${prefix%/}"
retention_days="${BACKUP_RETENTION_DAYS:-14}"
if ! [[ "${retention_days}" =~ ^[1-9][0-9]*$ ]]; then
    echo "BACKUP_RETENTION_DAYS must be a positive integer" >&2
    exit 64
fi

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

dump_file="${work_dir}/postgres.dump"
artifact_file="${work_dir}/artifacts.tar.gz"
manifest_file="${work_dir}/manifest.sha256"

pg_dump --dbname="${DATABASE_URL}" --format=custom --no-owner --no-privileges --file="${dump_file}"
tar --create --gzip --file="${artifact_file}" --directory=/artifacts .
(cd "${work_dir}" && sha256sum "$(basename "${dump_file}")" "$(basename "${artifact_file}")" > "$(basename "${manifest_file}")")

remote="techatlas_backup:${BACKUP_S3_BUCKET}/${prefix}/${backup_id}"
rclone copy "${work_dir}" "${remote}" --transfers=1 --checkers=2
rclone delete "techatlas_backup:${BACKUP_S3_BUCKET}/${prefix}" --min-age="${retention_days}d"
rclone rmdirs "techatlas_backup:${BACKUP_S3_BUCKET}/${prefix}" --leave-root

printf '{"backup_id":"%s","retention_days":%s}\n' "${backup_id}" "${retention_days}"
