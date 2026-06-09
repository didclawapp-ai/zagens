#!/usr/bin/env bash
# Install pre-commit (fmt) and pre-push (lint) hooks into .git/hooks/.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
hooks_src="${repo_root}/scripts/ci/hooks"
hooks_dst="${repo_root}/.git/hooks"

for name in pre-commit pre-push prepare-commit-msg; do
  cp "${hooks_src}/${name}" "${hooks_dst}/${name}"
  chmod +x "${hooks_dst}/${name}"
  echo "installed ${hooks_dst}/${name}"
done

echo "Git hooks ready. pre-push runs scripts/ci/verify-lint.sh (SKIP_VERIFY=1 to bypass)."
