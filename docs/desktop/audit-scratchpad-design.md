# Audit Scratchpad (public pointer)

Zagens full-repo audit external memory (`scratchpad_*` tools, `audit-repo` skill, coverage gates) is documented here for discoverability.

## Where to read

| Audience | Document |
|----------|----------|
| Harness composition & coverage gate overview | [COMPOSABLE_HARNESS.md](../harness/COMPOSABLE_HARNESS.md) |
| Long-horizon / AuditScratchpadPanel context | [LONG_HORIZON_CODE_TASKS.md](../harness/LONG_HORIZON_CODE_TASKS.md) |
| Bundled skill (P0–P2 workflow) | `crates/runtime-server/assets/skills/audit-repo/SKILL.md` |

Maintainer design notes and the “full completion” iteration live in the private desktop docs tree (`doc_Private/docs/desktop/audit-scratchpad-design.md`, `audit-full-completion-iteration.md`) and are not published with the open-source tree.

## Runtime contracts (short)

- Inventory is SSOT for area completion (`done` / `deferred`).
- `write_file` to audit deliverables is gated on inventory closeout + coverage ratios (or approved `partial_closeout` / staged intermediate paths).
- Prefer `scratchpad_defer_remaining` for mass P2 defer; never batch `scratchpad_set_area(deferred)`.
