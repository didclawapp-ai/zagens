#!/usr/bin/env bash
# Local pre-push lint gate (remote CI is PR-first — see .github/workflows/ci.yml).
#
#   scripts/ci/ci-push-gate.sh --local  → exit 0 = skip verify-lint, exit 1 = run full lint
#
# Skip local lint when:
#   - commit message contains [skip ci] or [ci skip], or
#   - every changed file vs upstream is docs / housekeeping only.
#
# Remote CI runs on: pull_request, release tags, schedule, workflow_dispatch — not on
# direct pushes to master/main (merge should land via PR after CI green).
set -euo pipefail

reason=code_changes
run_full=true

emit() {
  local val="$1"
  if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    {
      echo "run_full=${val}"
      echo "reason=${reason}"
    } >>"$GITHUB_OUTPUT"
  fi
  echo "ci-push-gate: run_full=${val} reason=${reason}" >&2
}

is_housekeeping_path() {
  local path="$1"
  case "$path" in
    *.md | LICENSE | NOTICE.md | SECURITY.md | CONTRIBUTING.md | LOCAL_DEV_VERIFY.md | project_rules.md)
      return 0
      ;;
    docs/* | doc_Private/* | deliverables/* | assets/* | producthunt/* | .cursor/*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

all_paths_housekeeping() {
  local path
  local saw_path=0
  while IFS= read -r path; do
    [[ -z "$path" ]] && continue
    saw_path=1
    if ! is_housekeeping_path "$path"; then
      return 1
    fi
  done
  ((saw_path == 0)) && return 1
  return 0
}

commit_message() {
  git log -1 --format=%B 2>/dev/null || true
}

message_requests_skip() {
  local msg
  msg="$(commit_message)"
  grep -qiE '\[(skip ci|ci skip)\]' <<<"$msg"
}

changed_files_local() {
  local base=""
  if base="$(git merge-base HEAD '@{upstream}' 2>/dev/null)" && [[ -n "$base" ]]; then
    git diff --name-only "$base" HEAD
    return
  fi
  local branch remote
  branch="$(git branch --show-current 2>/dev/null || true)"
  for remote in "origin/${branch}" "origin/master" "origin/main"; do
    if base="$(git rev-parse "$remote" 2>/dev/null)"; then
      git diff --name-only "$base" HEAD
      return
    fi
  done
  git show --name-only --format= HEAD
}

decide_local() {
  if message_requests_skip; then
    reason=skip_ci_marker
    run_full=false
    return
  fi

  local files
  files="$(changed_files_local)"
  if all_paths_housekeeping <<<"$files"; then
    reason=docs_only
    run_full=false
    return
  fi

  reason=code_changes
  run_full=true
}

if [[ "${1:-}" != --local ]]; then
  echo "ci-push-gate: remote branch pushes do not trigger CI; use --local for pre-push hook" >&2
  exit 2
fi

decide_local
emit "$run_full"

if [[ "$run_full" == "true" ]]; then
  exit 1
fi
exit 0
