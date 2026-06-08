#!/usr/bin/env bash
# Reclaim disk on GitHub-hosted Ubuntu runners before large Rust builds.
set -euo pipefail

if [ "${GITHUB_ACTIONS:-}" != "true" ] || [ "${RUNNER_OS:-}" != "Linux" ]; then
  exit 0
fi

echo "==> Disk before GHA cleanup"
df -h /

sudo rm -rf /usr/share/dotnet /usr/local/lib/android /opt/ghc /opt/hostedtoolcache/CodeQL 2>/dev/null || true
if command -v docker >/dev/null 2>&1; then
  sudo docker image prune --all --force 2>/dev/null || true
fi
sudo apt-get clean 2>/dev/null || true

echo "==> Disk after GHA cleanup"
df -h /
