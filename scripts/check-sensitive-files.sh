#!/usr/bin/env bash
set -euo pipefail

# .gitignore protects normal staging. This check makes prohibited paths fail in
# CI as well, including when someone uses `git add --force` locally.
blocked_paths=()

while IFS= read -r path; do
  case "${path}" in
    .env|.env.*)
      if [[ "${path}" != ".env.example" && "${path}" != ".env.production.example" ]]; then
        blocked_paths+=("${path}")
      fi
      ;;
    AGENTS.md|Extras.md|Auth0_Setup.md|VISION.md|.envrc)
      blocked_paths+=("${path}")
      ;;
    *.pem|*.key|*.p12|*.pfx|*.secrets|secrets/*|credentials/*)
      blocked_paths+=("${path}")
      ;;
  esac
done < <(git ls-files)

if (( ${#blocked_paths[@]} > 0 )); then
  printf '%s\n' "Refusing to commit prohibited local or sensitive files:" >&2
  printf '  %s\n' "${blocked_paths[@]}" >&2
  exit 1
fi

printf '%s\n' "Sensitive-file path check passed."
