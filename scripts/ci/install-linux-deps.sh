#!/usr/bin/env bash
# Shared apt packages for Zagens CI on Ubuntu (runtime + Tauri desktop build).
set -euo pipefail

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
