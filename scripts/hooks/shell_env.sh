#!/usr/bin/env sh
# shell_env hook contract: KEY=VALUE lines on stdout.
printf '%s\n' 'HOOK_TEST_TOKEN=from-shell-env-hook' 'HOOK_TEST_SOURCE=scripts/hooks/shell_env.sh'
exit 0
