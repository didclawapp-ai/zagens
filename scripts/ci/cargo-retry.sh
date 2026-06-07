#!/usr/bin/env bash
# Retry cargo when crates.io flakes (e.g. curl 56 "Connection was reset" on GH Actions).
set -euo pipefail

max="${CARGO_RETRY_MAX_ATTEMPTS:-3}"
wait="${CARGO_RETRY_WAIT_SECONDS:-20}"
attempt=1

while true; do
  if cargo "$@"; then
    exit 0
  fi
  code=$?
  if (( attempt >= max )); then
    exit "$code"
  fi
  echo "::warning::cargo $* failed (attempt ${attempt}/${max}); retrying in ${wait}s..." >&2
  sleep "$wait"
  attempt=$((attempt + 1))
done
