#!/usr/bin/env bash
# Cron-friendly: refresh /var/www/zagens/download/stats.json from Nginx download log.
# Example crontab (hourly):
#   17 * * * * /var/www/zagens/deploy/update-download-stats.sh >> /var/log/zagens-stats.log 2>&1

set -euo pipefail

WEB_ROOT="${WEB_ROOT:-/var/www/zagens}"
LOG="${NGINX_DOWNLOAD_LOG:-/var/log/nginx/zagens-download.log}"
OFFSET="${DOWNLOAD_COUNT_OFFSET:-0}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_SCRIPT="${SCRIPT_DIR}/../scripts/aggregate-download-stats.mjs"

if [ ! -f "$LOG" ]; then
  echo "skip: log missing ($LOG)"
  exit 0
fi

if ! command -v node >/dev/null 2>&1; then
  echo "error: node not found"
  exit 1
fi

exec node "$REPO_SCRIPT" \
  --log "$LOG" \
  --out "${WEB_ROOT}/download/stats.json" \
  --offset "$OFFSET"
