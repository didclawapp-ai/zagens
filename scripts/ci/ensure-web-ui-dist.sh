#!/usr/bin/env bash
# Desktop `cargo clippy --all-targets` needs `crates/desktop/web-ui/dist` (Tauri frontendDist).
set -euo pipefail

dist="crates/desktop/web-ui/dist/index.html"
if [[ -f "${dist}" ]]; then
  exit 0
fi

echo "==> web-ui dist missing; npm ci + build (required for desktop clippy)"
(
  cd crates/desktop/web-ui
  npm ci --include=dev
  npm run build
)
