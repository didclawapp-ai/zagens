#!/usr/bin/env bash
# Decide whether a push needs the full CI matrix (and local pre-push lint).
#
# Remote: sourced from .github/workflows/ci.yml (gate job).
# Local:  scripts/ci/ci-push-gate.sh --local  → exit 0 = skip, exit 1 = run full lint
#
# Skip heavy CI on branch pushes when:
#   - commit message contains [skip ci] or [ci skip], or
#   - every changed file is docs / housekeeping only (see is_housekeeping_path).
#
# Always run full CI for: release tags, pull_request, schedule, workflow_dispatch,
# and any push that touches code, CI scripts, or workflows.
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
  if [[ -n "${COMMIT_MESSAGE:-}" ]]; then
    printf '%s' "$COMMIT_MESSAGE"
    return
  fi
  git log -1 --format=%B 2>/dev/null || true
}

message_requests_skip() {
  local msg
  msg="$(commit_message)"
  grep -qiE '\[(skip ci|ci skip)\]' <<<"$msg"
}

changed_files_for_push() {
  local before="${GITHUB_EVENT_BEFORE:-}"
  local head="${GITHUB_SHA:-HEAD}"
  if [[ -n "$before" && "$before" != "0000000000000000000000000000000000000000" ]]; then
    git diff --name-only "$before" "$head"
    return
  fi
  git show --name-only --format= "$head"
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

decide() {
  local event="${GITHUB_EVENT_NAME:-local}"
  local ref="${GITHUB_REF:-}"

  if [[ "$ref" == refs/tags/zagens-v* || "$ref" == refs/tags/ds-pick-v* ]]; then
    reason=release_tag
    run_full=true
    return
  fi

  if [[ "$event" == "pull_request" || "$event" == "schedule" || "$event" == "workflow_dispatch" ]]; then
    reason="$event"
    run_full=true
    return
  fi

  if message_requests_skip; then
    reason=skip_ci_marker
    run_full=false
    return
  fi

  local files
  if [[ "${1:-}" == --local ]]; then
    files="$(changed_files_local)"
  else
    files="$(changed_files_for_push)"
  fi

  if all_paths_housekeeping <<<"$files"; then
    reason=docs_only
    run_full=false
    return
  fi

  reason=code_changes
  run_full=true
}

decide "${1:-}"
emit "$run_full"

if [[ "${1:-}" == --local ]]; then
  if [[ "$run_full" == "true" ]]; then
    exit 1
  fi
  exit 0
fi
