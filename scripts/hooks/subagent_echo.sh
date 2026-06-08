#!/usr/bin/env sh
# Logs subagent_start / subagent_end context fields to stderr.
input=$(cat)
echo "subagent_hook payload=$input" >&2
exit 0
