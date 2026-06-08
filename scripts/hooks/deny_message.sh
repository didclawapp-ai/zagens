#!/usr/bin/env sh
# Blocks message_submit when message contains BLOCK_ME (exit 2).
input=$(cat)
case "$input" in
  *BLOCK_ME*) exit 2 ;;
esac
exit 0
