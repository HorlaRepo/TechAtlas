#!/usr/bin/env bash

set -euo pipefail

readonly repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly compose_file="$repository_root/infrastructure/compose/docker-compose.dev.yml"
readonly environment_file="${TECHATLAS_E2E_ENV_FILE:-$repository_root/.env.example}"
readonly project_name="${TECHATLAS_E2E_PROJECT_NAME:-techatlas-e2e}"
readonly timeout_seconds="${TECHATLAS_E2E_TIMEOUT_SECONDS:-900}"
readonly domain="example.com"

compose() {
  docker compose \
    --ansi never \
    --project-name "$project_name" \
    --env-file "$environment_file" \
    --file "$compose_file" \
    "$@"
}

show_logs() {
  compose logs --tail=200 api scheduler worker migrate postgres redis meilisearch prometheus grafana tempo otel-collector oidc-test-fixture || true
}

remove_test_data_volumes() {
  local volume
  for volume in \
    postgres_data \
    redis_data \
    meilisearch_data \
    worker_artifacts \
    prometheus_data \
    grafana_data \
    tempo_data \
    dashboard_node_modules \
    pnpm_store; do
    docker volume rm "${project_name}_${volume}" >/dev/null 2>&1 || true
  done
}

reset_test_stack() {
  compose --profile pipeline --profile observability --profile test-fixture down --remove-orphans || true
  remove_test_data_volumes
}

cleanup() {
  local exit_code=$?
  if [[ "${TECHATLAS_E2E_KEEP:-0}" == "1" ]]; then
    printf 'Keeping Docker project %s for inspection.\n' "$project_name"
  else
    reset_test_stack
  fi
  exit "$exit_code"
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'Required command is unavailable: %s\n' "$1" >&2
    exit 1
  fi
}

wait_for_http() {
  local url=$1
  local label=$2
  local deadline=$((SECONDS + timeout_seconds))
  until curl --fail --silent "$url" >/dev/null 2>&1; do
    if (( SECONDS >= deadline )); then
      printf 'Timed out waiting for %s at %s.\n' "$label" "$url" >&2
      show_logs
      exit 1
    fi
    sleep 2
  done
}

sql_value() {
  compose exec -T postgres \
    psql --username techatlas --dbname techatlas --tuples-only --no-align --command "$1"
}

wait_for_pipeline() {
  local deadline=$((SECONDS + timeout_seconds))
  local state
  local query
  query="SELECT CASE WHEN
    (SELECT COUNT(*) FROM crawl_attempts attempts
      JOIN domains domains ON domains.id = attempts.domain_id
      WHERE domains.canonical_domain = '$domain' AND attempts.status = 'succeeded') >= 1
    AND (SELECT COUNT(*) FROM crawl_snapshots snapshots
      JOIN domains domains ON domains.id = snapshots.domain_id
      WHERE domains.canonical_domain = '$domain') >= 1
    AND (SELECT COUNT(*) FROM raw_artifacts artifacts
      JOIN crawl_snapshots snapshots ON snapshots.id = artifacts.crawl_snapshot_id
      JOIN domains domains ON domains.id = snapshots.domain_id
      WHERE domains.canonical_domain = '$domain') >= 1
    AND (SELECT COUNT(*) FROM detection_rule_observations observations
      JOIN crawl_snapshots snapshots ON snapshots.id = observations.crawl_snapshot_id
      JOIN domains domains ON domains.id = snapshots.domain_id
      WHERE domains.canonical_domain = '$domain') >= 5
    AND (SELECT last_success_at IS NOT NULL FROM search_index_state WHERE singleton)
    THEN 'ready' ELSE 'waiting' END;"

  while true; do
    state="$(sql_value "$query" 2>/dev/null | tr -d '[:space:]' || true)"
    if [[ "$state" == "ready" ]]; then
      return
    fi
    if (( SECONDS >= deadline )); then
      printf 'Timed out waiting for the crawl, detection, artifact, and search-index pipeline.\n' >&2
      show_logs
      exit 1
    fi
    sleep 2
  done
}

wait_for_prometheus_targets() {
  local deadline=$((SECONDS + timeout_seconds))
  local response
  while true; do
    response="$(curl --fail --silent --show-error "http://localhost:$PROMETHEUS_PORT/api/v1/targets" 2>/dev/null || true)"
    if jq --exit-status '
      .status == "success"
      and ([.data.activeTargets[] | select(.labels.job == "techatlas-api" or .labels.job == "techatlas-scheduler" or .labels.job == "techatlas-worker")] | length == 3)
      and all(.data.activeTargets[] | select(.labels.job == "techatlas-api" or .labels.job == "techatlas-scheduler" or .labels.job == "techatlas-worker"); .health == "up")
    ' >/dev/null <<<"$response"; then
      return
    fi
    if (( SECONDS >= deadline )); then
      printf 'Timed out waiting for Prometheus to scrape the API, scheduler, and worker.\n' >&2
      show_logs
      exit 1
    fi
    sleep 2
  done
}

require_command docker
require_command curl
require_command jq
if [[ ! -f "$environment_file" ]]; then
  printf 'Environment file does not exist: %s\n' "$environment_file" >&2
  exit 1
fi

trap cleanup EXIT

cd "$repository_root"
export API_PORT="${TECHATLAS_E2E_API_PORT:-13000}"
export SCHEDULER_PORT="${TECHATLAS_E2E_SCHEDULER_PORT:-13001}"
export WORKER_PORT="${TECHATLAS_E2E_WORKER_PORT:-13002}"
export DASHBOARD_PORT="${TECHATLAS_E2E_DASHBOARD_PORT:-15173}"
export POSTGRES_PORT="${TECHATLAS_E2E_POSTGRES_PORT:-15433}"
export REDIS_PORT="${TECHATLAS_E2E_REDIS_PORT:-16380}"
export MEILISEARCH_PORT="${TECHATLAS_E2E_MEILISEARCH_PORT:-17700}"
export PROMETHEUS_PORT="${TECHATLAS_E2E_PROMETHEUS_PORT:-19090}"
export GRAFANA_PORT="${TECHATLAS_E2E_GRAFANA_PORT:-13003}"
export TEMPO_PORT="${TECHATLAS_E2E_TEMPO_PORT:-13200}"
export OTEL_GRPC_PORT="${TECHATLAS_E2E_OTEL_GRPC_PORT:-14317}"
export OIDC_TEST_FIXTURE_PORT="${TECHATLAS_E2E_OIDC_TEST_FIXTURE_PORT:-18080}"
export OTEL_EXPORTER_OTLP_ENDPOINT="${OTEL_EXPORTER_OTLP_ENDPOINT:-http://otel-collector:4317}"
export ADMIN_OIDC_ISSUER="http://oidc-test-fixture:8080"
export ADMIN_OIDC_AUDIENCE="techatlas-e2e"
export ADMIN_OIDC_JWKS_URL="http://oidc-test-fixture:8080/jwks.json"

printf 'Starting an isolated local E2E stack in Docker project %s.\n' "$project_name"
reset_test_stack
compose --profile pipeline --profile observability --profile test-fixture up --build --detach \
  api scheduler worker prometheus tempo otel-collector grafana oidc-test-fixture

wait_for_http "http://localhost:$OIDC_TEST_FIXTURE_PORT/jwks.json" "OIDC test fixture"
wait_for_http "http://localhost:$API_PORT/readyz" "API readiness"
wait_for_http "http://localhost:$SCHEDULER_PORT/readyz" "scheduler readiness"
wait_for_http "http://localhost:$WORKER_PORT/readyz" "worker readiness"
wait_for_http "http://localhost:$GRAFANA_PORT/api/health" "Grafana health"
wait_for_http "http://localhost:$TEMPO_PORT/ready" "Tempo readiness"

token="$(curl --fail --silent --show-error --request POST \
  "http://localhost:$OIDC_TEST_FIXTURE_PORT/token" \
  | jq --raw-output '.access_token')"
if [[ -z "$token" || "$token" == "null" ]]; then
  printf 'The local OIDC test fixture did not issue an operator access token.\n' >&2
  exit 1
fi

printf 'Importing %s through the OIDC-protected admin API.\n' "$domain"
curl --fail --silent --show-error \
  --request POST \
  --header "Authorization: Bearer $token" \
  --header 'Content-Type: application/json' \
  --data '{"source_name":"local-e2e","csv":"domain\nexample.com\n"}' \
  "http://localhost:$API_PORT/api/v1/admin/imports/csv" \
  | jq --exit-status '.accepted_row_count == 1' >/dev/null

wait_for_pipeline

curl --fail --silent --show-error --get \
  --data-urlencode "q=$domain" \
  "http://localhost:$API_PORT/api/v1/public/search/domains" \
  | jq --exit-status --arg domain "$domain" \
    'any(.results[]; .canonical_domain == $domain)' >/dev/null
curl --fail --silent --show-error "http://localhost:$API_PORT/api/v1/public/domains/$domain" \
  | jq --exit-status --arg domain "$domain" \
    '.canonical_domain == $domain and .last_crawled_at != null' >/dev/null

wait_for_prometheus_targets

printf 'Local end-to-end verification passed for %s.\n' "$domain"
