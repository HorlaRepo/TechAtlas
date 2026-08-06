#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID}" -ne 0 ]]; then
    echo "deploy-release.sh must run as root" >&2
    exit 77
fi

release_sha="${1:-}"
release_directory="${2:-}"
if ! [[ "${release_sha}" =~ ^[0-9a-f]{40}$ ]]; then
    echo "usage: deploy-release.sh COMMIT_SHA RELEASE_DIRECTORY" >&2
    exit 64
fi
if [[ "${release_directory}" != "/tmp/techatlas-release-${release_sha}" || ! -d "${release_directory}" ]]; then
    echo "release directory is missing or does not match the commit" >&2
    exit 66
fi

readonly repository="/opt/techatlas"
readonly production_environment="/etc/techatlas/production.env"
readonly release_environment="/etc/techatlas/release.env"
readonly compose_file="${repository}/infrastructure/compose/docker-compose.prod.yml"
readonly tunnel_service="cloudflared-techatlas.service"

for required_path in "${repository}/.git" "${production_environment}" "${release_directory}/backend.tar.gz" "${release_directory}/dashboard.tar.gz" "${release_directory}/backup.tar.gz"; do
    if [[ ! -e "${required_path}" ]]; then
        echo "required deployment path is missing: ${required_path}" >&2
        exit 66
    fi
done

repository_owner="$(stat --format '%U' "${repository}")"
if ! id "${repository_owner}" >/dev/null 2>&1; then
    echo "repository owner does not exist: ${repository_owner}" >&2
    exit 66
fi

run_as_repository_owner() {
    runuser --user "${repository_owner}" -- "$@"
}

if [[ -n "$(run_as_repository_owner git -C "${repository}" status --porcelain --untracked-files=no)" ]]; then
    echo "refusing to replace locally modified tracked deployment files" >&2
    exit 65
fi

run_as_repository_owner git -C "${repository}" fetch --no-tags --depth=1 origin "${release_sha}"
if ! run_as_repository_owner git -C "${repository}" cat-file -e "${release_sha}^{commit}"; then
    echo "requested release commit was not fetched: ${release_sha}" >&2
    exit 66
fi
run_as_repository_owner git -C "${repository}" checkout --detach "${release_sha}"

for archive in backend dashboard backup; do
    docker load --input "${release_directory}/${archive}.tar.gz"
done

readonly backend_image="techatlas-backend:${release_sha}"
readonly dashboard_image="techatlas-dashboard:${release_sha}"
readonly backup_image="techatlas-backup:${release_sha}"
for image in "${backend_image}" "${dashboard_image}" "${backup_image}"; do
    docker image inspect "${image}" >/dev/null
done

release_environment_temporary="$(mktemp "${release_environment}.XXXXXX")"
cleanup_temporary_environment() {
    rm -f -- "${release_environment_temporary}"
}
trap cleanup_temporary_environment EXIT
{
    printf 'BACKEND_IMAGE=%s\n' "${backend_image}"
    printf 'DASHBOARD_IMAGE=%s\n' "${dashboard_image}"
    printf 'BACKUP_IMAGE=%s\n' "${backup_image}"
    printf 'TECHATLAS_RELEASE_SHA=%s\n' "${release_sha}"
} >"${release_environment_temporary}"
chown root:root "${release_environment_temporary}"
chmod 0600 "${release_environment_temporary}"
mv -- "${release_environment_temporary}" "${release_environment}"
trap - EXIT

compose=(docker compose --env-file "${production_environment}" --env-file "${release_environment}" -f "${compose_file}")
"${compose[@]}" config --quiet

wait_for_service_exit() {
    local service="$1"
    local timeout_seconds="$2"
    local container_id
    local deadline
    local state
    local exit_code

    container_id="$("${compose[@]}" ps --all --quiet "${service}")"
    if [[ -z "${container_id}" ]]; then
        echo "${service} did not create a container" >&2
        return 1
    fi

    deadline=$((SECONDS + timeout_seconds))
    while (( SECONDS < deadline )); do
        state="$(docker inspect --format '{{.State.Status}}' "${container_id}")"
        if [[ "${state}" == "exited" ]]; then
            exit_code="$(docker inspect --format '{{.State.ExitCode}}' "${container_id}")"
            if [[ "${exit_code}" == "0" ]]; then
                return 0
            fi
            echo "${service} exited with status ${exit_code}" >&2
            "${compose[@]}" logs "${service}" >&2
            return 1
        fi
        sleep 2
    done

    echo "timed out waiting for ${service} to complete" >&2
    "${compose[@]}" logs "${service}" >&2
    return 1
}

"${compose[@]}" up --detach --wait --wait-timeout 180 postgres redis meilisearch
"${compose[@]}" up --detach --force-recreate artifact-init
wait_for_service_exit artifact-init 60

"${compose[@]}" up --detach --force-recreate migrate
wait_for_service_exit migrate 300

"${compose[@]}" up --detach --remove-orphans --wait --wait-timeout 240 \
    api scheduler worker dashboard caddy prometheus tempo otel-collector grafana

public_host="$(awk -F= '$1 == "TECHATLAS_PUBLIC_HOST" { print substr($0, index($0, "=") + 1); exit }' "${production_environment}")"
origin_port="$(awk -F= '$1 == "CADDY_ORIGIN_PORT" { print substr($0, index($0, "=") + 1); exit }' "${production_environment}")"
origin_port="${origin_port:-8081}"
if [[ -z "${public_host}" ]]; then
    echo "TECHATLAS_PUBLIC_HOST is missing from ${production_environment}" >&2
    exit 65
fi
curl --fail --silent --show-error --retry 5 --retry-all-errors --retry-delay 2 \
    --header "Host: ${public_host}" "http://127.0.0.1:${origin_port}/" >/dev/null

systemctl enable --now "${tunnel_service}"
systemctl is-active --quiet "${tunnel_service}"

rm --recursive --force -- "${release_directory}"
echo "deployed TechAtlas release ${release_sha}"
