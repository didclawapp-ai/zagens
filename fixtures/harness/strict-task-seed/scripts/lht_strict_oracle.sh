#!/usr/bin/env bash
# Harness-provided oracle for LHT strict-mode tasks (do not remove).
set -euo pipefail
cd "$(dirname "$0")/.."

echo "[oracle] go build"
go build ./...

echo "[oracle] go vet"
go vet ./...

if [ -f scripts/conformance.sh ]; then
  echo "[oracle] conformance"
  bash scripts/conformance.sh
else
  echo "[oracle] FAIL: scripts/conformance.sh missing"
  exit 1
fi

echo "[oracle] PASS"
