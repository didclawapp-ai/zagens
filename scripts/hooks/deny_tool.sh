#!/usr/bin/env sh
# Blocks tool_call_before for exec_shell (JSON deny response).
printf '%s\n' '{"decision":"deny","reason":"exec_shell blocked by test hook"}'
exit 0
