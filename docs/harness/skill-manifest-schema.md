# Harness contract schema (Phase 2a.1)

**Status:** Draft v0 · Phase 2a  
**Audience:** Skill authors, gate-as-code contributors, runtime implementers  
**Aligns with:** [`HarnessVerifyLoop::VerifyStageSpec`](../../crates/runtime-server/src/long_horizon/harness_verify_loop.rs) · [`KernelEvent::StageGateBlocked`](../../crates/core/src/engine/kernel_event.rs) · [HARNESS_EVENT_TAXONOMY](../../doc_Private/docs/HARNESS_EVENT_TAXONOMY.md) §3.2

---

## 1. One schema, two surfaces

**Skill manifest** and **gate manifest** are the same TOML contract:

| Field | Skill use | Gate use (queue / completion) |
|-------|-----------|-------------------------------|
| `[[stages]]` | Ordered workflow + per-stage tool exposure | Optional (flat gates omit) |
| `[[verify]]` | Stage-bound predicates (`stage = "…"`) | Flat rows (`id = "…"`, no `stage`) |
| `[verify_budget]` | Per-stage retry / timeout for verify-loop | Same for `HarnessVerifyLoop` |
| `[rollback]` | Snapshot on verify exhaustion | Queue rollback policy (future) |

Predicate names **must** be registered in `long_horizon/predicate::*` (`exit_code`, `file_exists`, `tests_pass`, `command_output_matches`, `file_count`). No second oracle.

Legacy Layer-2 completion gate `[long_horizon.completion_gate.verify]` rows (`cmd` / `argv`) remain supported via `predicate::run_manifest_verify_entry`; new contracts should prefer explicit `predicate` + `args`.

---

## 2. Top-level shape

```toml
schema_version = 1

[harness]
id = "office-write"
description = "Write DOCX with readback verify"

[verify_budget]
max_retries = 2
timeout_ms = 300000

[rollback]
strategy = "snapshot"   # snapshot | none

[[stages]]
id = "prepare"
tools = ["read_office", "load_office_payload", "glob_files"]

[[stages]]
id = "write"
tools = ["write_office"]
requires = ["prepare"]

[[verify]]
stage = "write"
predicate = "file_exists"
args = { path = "deliverables/report.docx" }
```

### 2.1 `[[stages]]`

| Field | Required | Description |
|-------|----------|-------------|
| `id` | ✓ | Stable stage id (telemetry + `stage_gate_blocked.stage`) |
| `tools` | | Tool names exposed while this stage is **current** (after prior `requires` satisfied) |
| `requires` | | Stage ids that must be verify-passed before this stage unlocks |

**Exposure rule (H2):** While stage *N* is current, only `stages[N].tools` plus [always-allowed meta tools](#24-always-allowed-tools) are registered for the model. Earlier-stage write tools stay hidden until their verify passes.

### 2.2 `[[verify]]`

| Field | Required | Description |
|-------|----------|-------------|
| `stage` | skill | Skill stage id; omit for flat gate manifests |
| `id` | gate | Row id for queue / completion gate; used as `harness_verify.stage` when `stage` absent |
| `predicate` | ✓ | Registered predicate name |
| `args` | | JSON object passed to `predicate::evaluate` |

Maps to runtime:

```rust
VerifyStageSpec {
    stage: entry.stage.or(entry.id).unwrap_or("verify-{i}"),
    predicate: entry.predicate,
    args: entry.args,
}
```

### 2.3 `[verify_budget]`

| Field | Default | Description |
|-------|---------|-------------|
| `max_retries` | `2` | Harness verify-loop retries per stage |
| `timeout_ms` | `300000` | Predicate exec timeout (shell-backed predicates) |

### 2.4 Always-allowed tools

While a staged contract is active, these stay visible so the model can load guidance and run T4 assertions:

- `load_skill`, `request_user_input`
- `assert_file_count`, `assert_output_matches`, `assert_tests_pass`

Calling any other tool before the current stage verify passes → **`stage_gate_blocked`** (execution fallback) even if exposure filtering missed it.

---

## 3. Events

### `stage_gate_blocked` (kernel_events)

Emitted when a blocked tool is invoked (see taxonomy §3.2):

```json
{
  "event_type": "stage_gate_blocked",
  "turn_id": "turn_…",
  "step_idx": 5,
  "skill": "office-write",
  "stage": "readback_verify",
  "tool_name": "write_office",
  "code": "stage_gate_blocked",
  "suggestion": "Complete readback verify before writing again."
}
```

### `harness_verify`

Stage verify and flat gate rows produce `harness_verify` via `HarnessVerifyLoop` (unchanged from Phase 1b).

---

## 4. Examples

| Fixture | Purpose |
|---------|---------|
| [`fixtures/harness/office-write-skill-manifest.toml`](../../fixtures/harness/office-write-skill-manifest.toml) | Office staged skill (Phase 2a.3 pilot) |
| [`fixtures/harness/code-edit-skill-manifest.toml`](../../fixtures/harness/code-edit-skill-manifest.toml) | Code edit + tests gate |
| [`fixtures/harness/microstack-completion-gate.toml`](../../fixtures/harness/microstack-completion-gate.toml) | Legacy Layer-2 completion gate (migrate to `predicate` rows over time) |

### 4.1 Co-located skill manifest

Place `harness.toml` beside `SKILL.md`. `load_skill` activates stage gate for the session when the file parses successfully.

Optional global override:

```toml
[long_horizon.stage_gate]
manifest = "fixtures/harness/office-write-skill-manifest.toml"
enforce = true
```

---

## 5. Rust types

Parsed by [`HarnessContract`](../../crates/core/src/long_horizon/harness_contract.rs) (`zagens_core::long_horizon::HarnessContract`).

---

## 6. Related docs

- [COMPOSABLE_HARNESS.md](./COMPOSABLE_HARNESS.md) §6 — Layer-2/Layer-3 completion gate
- [HARNESS_LOOP_ITERATION.md](../../doc_Private/docs/HARNESS_LOOP_ITERATION.md) §3.2 — skill definition upgrade
- [RUNTIME_ARCHITECTURE.md](../tech/RUNTIME_ARCHITECTURE.md) §0.1 — harness naming (no `verify_step` for harness events)
