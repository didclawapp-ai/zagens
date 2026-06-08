#!/usr/bin/env sh
# Example hook: log stdin JSON to stderr (stdout stays clean unless you intend to deny/rewrite).
input=$(cat)
echo "hook=echo_context event_payload=$input" >&2
exit 0
