#!/usr/bin/env bash
# Shared apt packages for Zagens CI on Ubuntu (runtime + Tauri desktop build).
set -euo pipefail

if [ -f scripts/ci/free-github-disk.sh ]; then
  bash scripts/ci/free-github-disk.sh
fi

for i in 1 2 3 4 5; do
  sudo apt-get update && break
  echo "apt-get update failed (attempt $i); retrying in 15s"
  sleep 15
done

sudo apt-get install -y \
  build-essential \
  curl \
  file \
  libayatana-appindicator3-dev \
  libdbus-1-dev \
  librsvg2-dev \
  libssl-dev \
  libwebkit2gtk-4.1-dev \
  libxdo-dev \
  pkg-config \
  wget

# Large runtime binaries (zagens-runtime / zagens) can OOM the default GHA
# runner during lld link, surfacing as "signal 7 [Bus error]".
if [ "${GITHUB_ACTIONS:-}" = "true" ] && ! swapon --show 2>/dev/null | grep -q .; then
  echo "Adding 4G swap for Rust link jobs..."
  sudo fallocate -l 4G /swapfile 2>/dev/null || sudo dd if=/dev/zero of=/swapfile bs=1M count=4096 status=progress
  sudo chmod 600 /swapfile
  sudo mkswap /swapfile
  sudo swapon /swapfile
  swapon --show
fi
