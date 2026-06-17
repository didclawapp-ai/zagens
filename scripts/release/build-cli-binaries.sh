#!/usr/bin/env bash
# Build release `zagens` + `zagens-runtime` binaries and write SHA-256 sidecars.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

OUT="${1:-release-cli-artifacts}"
mkdir -p "$OUT"

echo "Building release CLI binaries..."
bash scripts/ci/cargo-retry.sh build -p zagens-cli --release --locked --features tui --bin zagens --bin zagens-runtime --bin zagens-tui

TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
EXT=""
if [[ "$TRIPLE" == *windows* ]]; then
  EXT=".exe"
fi

for name in zagens zagens-runtime zagens-tui; do
  src="target/release/${name}${EXT}"
  dest="${OUT}/${name}-${TRIPLE}${EXT}"
  cp "$src" "$dest"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$dest" > "${dest}.sha256"
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$dest" > "${dest}.sha256"
  fi
  echo "  ${dest}"
done

echo "OK: CLI artifacts in ${OUT}/"
