#!/usr/bin/env bash
# Publish the zagens-* runtime crate chain to crates.io in dependency order.
#
# Usage:
#   bash scripts/release/pre-publish-check.sh          # run first
#   bash scripts/release/publish-crates.sh --dry-run    # simulate upload (needs deps on registry for later crates)
#   bash scripts/release/publish-crates.sh --publish    # real upload (requires `cargo login`)
#
# Options:
#   --from PKG     Resume after PKG (skip earlier crates)
#   --allow-dirty  Pass through to cargo publish (local experiments only)
#   --wait SECS    Pause between publishes (default 90) for index propagation
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

CRATES=(
  zagens-protocol
  zagens-secrets
  zagens-topic-memory
  zagens-tools
  zagens-config
  zagens-core
  zagens-runtime-adapters
  zagens-runtime-orchestrator
  zagens-runtime-api
  zagens-cli
)

mode=""
from_pkg=""
allow_dirty=()
wait_secs=90

usage() {
  sed -n '2,12p' "$0" | sed 's/^# \?//'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run | --publish)
      mode="$1"
      shift
      ;;
    --from)
      from_pkg="${2:?--from requires a package name}"
      shift 2
      ;;
    --allow-dirty)
      allow_dirty=(--allow-dirty)
      shift
      ;;
    --wait)
      wait_secs="${2:?--wait requires seconds}"
      shift 2
      ;;
    -h | --help)
      usage 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage 2
      ;;
  esac
done

if [[ -z "${mode}" ]]; then
  echo "ERROR: pass --dry-run or --publish" >&2
  usage 2
fi

if [[ "${mode}" == "--publish" && ${#allow_dirty[@]} -eq 0 ]]; then
  if [[ -n "$(git status --porcelain 2>/dev/null || true)" ]]; then
    echo "ERROR: working tree is dirty. Commit first or pass --allow-dirty." >&2
    exit 1
  fi
fi

if [[ "${mode}" == "--publish" ]]; then
  if ! cargo search zagens-protocol --limit 1 >/dev/null 2>&1; then
    echo "WARNING: could not reach crates.io (cargo search failed). Continuing anyway." >&2
  fi
  if ! cargo owner --list -p zagens-protocol >/dev/null 2>&1; then
    echo "NOTE: zagens-protocol not on crates.io yet (expected for first publish)." >&2
  fi
fi

skip=1
if [[ -n "${from_pkg}" ]]; then
  skip=0
fi

for pkg in "${CRATES[@]}"; do
  if [[ "${skip}" -eq 0 ]]; then
    if [[ "${pkg}" == "${from_pkg}" ]]; then
      skip=1
    else
      continue
    fi
  fi

  echo ""
  echo "==> ${mode} ${pkg}"
  if [[ "${mode}" == "--dry-run" ]]; then
    if ! cargo publish -p "${pkg}" --dry-run "${allow_dirty[@]}"; then
      echo "ERROR: dry-run failed for ${pkg}." >&2
      echo "If missing registry deps, publish earlier crates first, then --from ${pkg}." >&2
      exit 1
    fi
  else
    cargo publish -p "${pkg}" "${allow_dirty[@]}"
    if [[ "${pkg}" != "zagens-cli" ]]; then
      echo "    waiting ${wait_secs}s for crates.io index…"
      sleep "${wait_secs}"
    fi
  fi
done

echo ""
if [[ "${mode}" == "--publish" ]]; then
  echo "OK: publish chain complete."
  echo "Verify in a clean environment:"
  echo "  cargo install zagens-cli --version 0.7.0 --bin zagens --locked"
  echo "  zagens --version && zagens doctor"
else
  echo "OK: dry-run chain complete."
fi
