#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

echo "== cargo test hooks =="
cargo test -p zagens-cli hooks

HOOKS_DIR="$ROOT/scripts/hooks"
SAMPLE='{"event":"session_start","context":{"session_id":"sess_test","workspace":"/tmp"}}'

run_hook() {
  local name="$1"
  local stdin="$2"
  shift 2
  echo "== $name =="
  printf '%s' "$stdin" | sh "$HOOKS_DIR/$name" "$@"
  echo "exit=$?"
}

run_hook echo_context.sh "$SAMPLE"
run_hook shell_env.sh '{}'
run_hook updated_input.sh '{}'
run_hook deny_tool.sh '{"event":"tool_call_before","context":{"tool_name":"exec_shell"}}'
run_hook deny_message.sh '{"event":"message_submit","context":{"message":"hello"}}'
set +e
printf '%s' '{"event":"message_submit","context":{"message":"BLOCK_ME"}}' | sh "$HOOKS_DIR/deny_message.sh"
code=$?
set -e
if [[ "$code" -ne 2 ]]; then
  echo "deny_message.sh expected exit 2 for BLOCK_ME, got $code" >&2
  exit 1
fi

echo "All hook smoke checks passed."
