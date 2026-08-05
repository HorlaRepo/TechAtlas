#!/usr/bin/env sh
set -eu

temporary_directory=$(mktemp -d)
trap 'rm -rf "$temporary_directory"' EXIT

OPENAPI_OUTPUT="$temporary_directory" pnpm exec openapi-ts
diff -ru src/generated "$temporary_directory"
