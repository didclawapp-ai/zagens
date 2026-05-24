# Backlog ADR — Enforce Linux Landlock / Windows sandbox

**Status:** Proposed  
**Related:** [SANDBOX_CAPABILITY_MATRIX.md](../SANDBOX_CAPABILITY_MATRIX.md), A6.3

## Context

`prepare_landlock` / `prepare_windows` set env markers only; commands run with full user privileges. macOS Seatbelt is the only enforced backend today.

## Decision (draft)

Implement via **helper binary** (Landlock ruleset → exec child) and Windows Job Object / Restricted Token — separate PRs per OS.

## Acceptance

- `exec_shell` in `workspace-write` policy cannot read `$HOME` outside workspace on Linux 5.13+.
- DS Pick settings copy matches actual enforcement (no “degraded” when enforced).
