#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${MAXMIND_LICENSE_KEY:-}" ]]; then
  echo "MAXMIND_LICENSE_KEY must be set for GeoLite2 Country setup." >&2
  exit 1
fi

for command in awk curl tar; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Required command is unavailable: $command" >&2
    exit 1
  fi
done

repo_root="$(cd -- "$(dirname -- "$0")/.." && pwd)"
geoip_directory="$repo_root/.techatlas/geoip"
mmdb_path="$geoip_directory/GeoLite2-Country.mmdb"
version_path="$geoip_directory/GeoLite2-Country.version"
env_path="$repo_root/.env"

mkdir -p "$geoip_directory"
work_directory="$(mktemp -d "$geoip_directory/.download.XXXXXX")"
cleanup() {
  rm -rf "$work_directory"
}
trap cleanup EXIT

archive="$work_directory/GeoLite2-Country.tar.gz"
curl_config="$work_directory/curl.conf"
umask 077
printf '%s\n' \
  'fail' \
  'location' \
  'silent' \
  'show-error' \
  "output = \"$archive\"" \
  "url = \"https://download.maxmind.com/app/geoip_download?edition_id=GeoLite2-Country&license_key=${MAXMIND_LICENSE_KEY}&suffix=tar.gz\"" \
  > "$curl_config"
curl --config "$curl_config"

if [[ ! -s "$archive" ]]; then
  echo "MaxMind returned no GeoLite2 Country archive. Check the license key and download access." >&2
  exit 1
fi

archive_member="$(tar -tzf "$archive" | awk '/(^|\/)GeoLite2-Country\.mmdb$/ { print; exit }')"
if [[ -z "$archive_member" ]]; then
  echo "Downloaded MaxMind archive does not contain GeoLite2-Country.mmdb." >&2
  exit 1
fi

release_directory="${archive_member%%/*}"
release_date="${release_directory#GeoLite2-Country_}"
if [[ ! "$release_date" =~ ^[0-9]{8}$ ]]; then
  echo "Downloaded MaxMind archive has an unrecognised release identifier." >&2
  exit 1
fi
release_version="${release_date:0:4}-${release_date:4:2}-${release_date:6:2}"

candidate_mmdb="$work_directory/GeoLite2-Country.mmdb"
tar -xOf "$archive" "$archive_member" > "$candidate_mmdb"
if [[ ! -s "$candidate_mmdb" ]]; then
  echo "Downloaded GeoLite2 Country database is empty." >&2
  exit 1
fi

mv "$candidate_mmdb" "$mmdb_path"
printf '%s\n' "$release_version" > "$version_path"

if [[ ! -f "$env_path" ]]; then
  cp "$repo_root/.env.example" "$env_path"
fi

set_env_value() {
  local key="$1"
  local value="$2"
  local temporary
  temporary="$(mktemp "$env_path.XXXXXX")"
  awk -v key="$key" -v value="$value" '
    $0 ~ "^" key "=" { print key "=" value; found = 1; next }
    { print }
    END { if (!found) print key "=" value }
  ' "$env_path" > "$temporary"
  mv "$temporary" "$env_path"
}

set_env_value "WORKER_GEOIP_DATABASE_PATH" "$mmdb_path"
set_env_value "WORKER_GEOIP_DATABASE_VERSION" "$release_version"

echo "GeoLite2 Country ${release_version} is ready at ${mmdb_path}."
echo "Start or recreate the pipeline with: COMPOSE_PROFILES=pipeline pnpm docker:up"
