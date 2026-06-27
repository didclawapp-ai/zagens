#!/usr/bin/env bash
# Load repo-root .env (if present) and run harness regression with R-015 longrun.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ -f .env ]]; then
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
fi

if [[ -z "${DEEPSEEK_API_KEY:-}" ]]; then
  echo "DEEPSEEK_API_KEY is not set. Add it to .env or export it in the shell." >&2
  exit 1
fi

exec bash scripts/ci/harness-regression.sh --with-longrun
