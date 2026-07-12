# Gate-as-Code (Phase 4.1)

**Status:** v0 · public format  
**Schema:** [`skill-manifest-schema.md`](../skill-manifest-schema.md) (`HarnessContract` / `schema_version = 1`)  
**CLI:** `zagens gate validate` · `zagens gate list` · `zagens queue add --gate-file`

---

## What it is

Gate-as-Code is the **portable, shareable** form of harness success criteria:

- One TOML file describes **what must be true** before a task is green.
- Every row uses the **predicate library** (`exit_code`, `tests_pass`, `file_exists`, …) — the same oracle as verify-loop, stage gate, and night queue.
- Skill manifests (`harness.toml` beside `SKILL.md`) and flat gate manifests **share the same schema**.

```text
  Contributor gate TOML          Runtime consumers
  ─────────────────────          ─────────────────
  docs/harness/gates/presets/ ──▶ zagens queue add --gate-file
  (public docs; edit here)       ▶ load_skill → stage gate
                                 ▶ [long_horizon.stage_gate].manifest

  crates/runtime-server/assets/gates/presets/
    └── copy of presets for `include_str!` / crates.io package
```

When changing a bundled preset, update **both** `docs/harness/gates/presets/` and `crates/runtime-server/assets/gates/presets/` (the latter is what `zagens gate --preset` embeds at compile time).
---

## Quick start

### Validate a contract

```bash
# Bundled preset
zagens gate validate --preset rust-cargo-smoke

# Your own file
zagens gate validate --file path/to/my-gate.toml

# JSON (CI-friendly)
zagens gate validate --preset go-build-vet --json
```

### Enqueue with a gate file

```bash
zagens queue add "fix failing tests" \
  --gate-file docs/harness/gates/presets/rust-cargo-smoke.toml

# Or bundled preset id (binary-embedded, no repo path required)
zagens queue add "go service hardening" --gate-preset go-build-vet
```

Inline predicates still work (`--gate file_exists:path=README.md`).

### List bundled presets

```bash
zagens gate list
```

| Preset | Purpose |
|--------|---------|
| `rust-cargo-smoke` | `cargo check` + `cargo test` |
| `go-build-vet` | `go build`, `go vet`, tests |
| `deliverables-min` | `deliverables/**` exists (no shell) |
| `go-microstack-migrated` | MicroStack Layer-2 rows (migration sample) |

Sources: [`presets/`](./presets/)

---

## Contract shape (flat gate)

Minimal gate manifest — no `[[stages]]`, only flat `[[verify]]` rows with `id`:

```toml
schema_version = 1

[harness]
id = "my-team-rust-gate"
description = "What this gate checks."

[verify_budget]
max_retries = 2
timeout_ms = 600000

[rollback]
strategy = "snapshot"

[[verify]]
id = "compile"
predicate = "tests_pass"
args = { toolchain = "cargo", cmd = "cargo check" }

[[verify]]
id = "tests"
predicate = "tests_pass"
args = { toolchain = "cargo" }
```

Staged skill contracts add `[[stages]]` and `stage = "…"` on verify rows — see [skill-manifest-schema.md](../skill-manifest-schema.md).

**Queue note:** `queue add --gate-file` imports **flat** rows only (`id` + predicate, no `stage`). Stage-bound rows are for skill/session gate, not overnight queue AND-gates.

---

## Docs in this directory

| File | Content |
|------|---------|
| [predicates.md](./predicates.md) | Registered predicates + `args` reference |
| [MIGRATION.md](./MIGRATION.md) | Legacy `[long_horizon.completion_gate]` → Gate-as-Code |
| [presets/](./presets/) | Curated starter gates (also embedded in CLI) |

---

## Contributing a gate

1. Copy a preset from [`presets/`](./presets/) or [`fixtures/harness/code-edit-skill-manifest.toml`](../../fixtures/harness/code-edit-skill-manifest.toml).
2. Set a unique `[harness].id`.
3. Use only [registered predicates](./predicates.md).
4. Run `zagens gate validate --file your-gate.toml`.
5. Open a PR adding the file under `docs/harness/gates/presets/` (or keep it project-local under `.zagens/gates/`).

Phase 4 exit criterion: **≥ 1 external gate contract reused** — track reuse via `[harness].id` in telemetry (`harness_verify`, `queue_gate_result`).

---

## Related

- [COMPOSABLE_HARNESS.md](../COMPOSABLE_HARNESS.md) — Layer-2/Layer-3 completion gate (legacy config path)
- [skill-manifest-schema.md](../skill-manifest-schema.md) — full schema + events
- `zagens_core::long_horizon::HarnessContract::validate()` — static checks (Rust / CI)
