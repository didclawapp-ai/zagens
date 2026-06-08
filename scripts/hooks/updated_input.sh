#!/usr/bin/env sh
# Rewrites tool input: forces command to "echo hook-rewritten".
printf '%s\n' '{"updatedInput":{"command":"echo hook-rewritten"}}'
exit 0
