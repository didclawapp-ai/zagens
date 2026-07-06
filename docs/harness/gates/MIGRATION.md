# Migration: legacy completion gate → Gate-as-Code

**From:** `[long_horizon.completion_gate]` in `~/.zagens/config.toml`  
**To:** Standalone `HarnessContract` TOML ([schema](../skill-manifest-schema.md))  
**Sample:** [`presets/go-microstack-migrated.toml`](./presets/go-microstack-migrated.toml) ← [`microstack-completion-gate.toml`](../../../fixtures/harness/microstack-completion-gate.toml)

---

## Why migrate

| Legacy | Gate-as-Code |
|--------|--------------|
| Embedded in user config | Portable file / preset / PR |
| `cmd` + optional `shell = "bash"` | Explicit `predicate` + `args` |
| Layer-2 + Layer-3 mixed in one table | Layer-2 verify in `[[verify]]`; Layer-3 deliverables still via completion_gate until schema v2 |
| Hard to validate in CI | `zagens gate validate --file …` |

Both paths still execute through **`predicate::run_manifest_verify_entry`** at runtime for legacy rows; new contracts should use predicate-native rows only.

---

## Layer-2 verify row mapping

### Simple command (exit code)

**Legacy:**

```toml
[[long_horizon.completion_gate.verify]]
id = "build"
cmd = "go build ./..."
```

**Gate-as-Code:**

```toml
[[verify]]
id = "build"
predicate = "exit_code"
args = { cmd = "go build ./..." }
```

### Tests

**Legacy:** `cmd = "cargo test"`  
**Gate-as-Code:**

```toml
[[verify]]
id = "tests"
predicate = "tests_pass"
args = { toolchain = "cargo" }
```

### gofmt (bash workaround → native)

Legacy MicroStack used bash because `gofmt -l` exits 0 even when files need formatting:

```toml
[[long_horizon.completion_gate.verify]]
id = "gofmt"
shell = "bash"
cmd = "test -z \"$(gofmt -l .)\""
```

**Preferred (cross-platform native probe):**

```toml
[[verify]]
id = "gofmt_clean"
predicate = "command_output_matches"
args = { command = "gofmt -l ." }
```

Runtime treats non-empty `gofmt -l` stdout as failure (`verify_platform::try_gofmt_list`).

### Coverage (bash awk → future)

Legacy bash coverage gate in MicroStack fixture is **not** copied verbatim — it depends on awk and per-package parsing.

Options today:

1. **Keep legacy row** in config for regression fixtures only.
2. **Use `exit_code`** with a project script that exits non-zero on low coverage.
3. **Wait for** built-in `zagens coverage-gate` (see `coverage_gate` CLI) wired as `shell=none` argv in a future preset revision.

Document platform constraints in your gate README if bash remains required.

### Git diff oracle

**Legacy:**

```toml
[[long_horizon.completion_gate.verify]]
id = "contracts_stable"
cmd = "git diff --exit-code contracts/"
```

**Gate-as-Code:**

```toml
[[verify]]
id = "contracts_stable"
predicate = "exit_code"
args = { cmd = "git diff --exit-code contracts/" }
```

**Prerequisite:** workspace is a git repo with committed baseline (same as legacy).

---

## Layer-3 deliverables (not yet in Gate-as-Code file)

Legacy deliverable rows remain under completion_gate:

```toml
[[long_horizon.completion_gate.deliverable]]
id = "gzip_middleware"
path = "middleware/gzip.go"
```

Phase 4.1 scope covers **Layer-2 predicate verify** only. Layer-3 path/glob/`tracked` deliverables stay in [COMPOSABLE_HARNESS.md](../COMPOSABLE_HARNESS.md) config until a schema extension adds `[[deliverable]]` to `HarnessContract`.

**Workaround:** combine Gate-as-Code file for Layer-2 + keep deliverables in config, or express file presence via `file_exists` / `file_count` predicates where sufficient.

---

## Config wrapper → standalone file

**Before (excerpt):**

```toml
[long_horizon.completion_gate]
mode = "enforce"
max_manifest_rounds = 5

[[long_horizon.completion_gate.verify]]
id = "build"
cmd = "go build ./..."
```

**After — standalone gate file:**

```toml
schema_version = 1
[harness]
id = "my-go-gate"

[verify_budget]
max_retries = 5
timeout_ms = 600000

[[verify]]
id = "build"
predicate = "exit_code"
args = { cmd = "go build ./..." }
```

**Wire it:**

```toml
# ~/.zagens/config.toml — stage gate (session-wide skill-style)
[long_horizon.stage_gate]
manifest = "/path/to/my-go-gate.toml"
enforce = true
```

Or per queue task:

```bash
zagens queue add "implement feature X" --gate-file /path/to/my-go-gate.toml
```

Mode / rounds for LHT completion gate remain in `[long_horizon.completion_gate]` when using the legacy integration path.

---

## Checklist

- [ ] Each legacy `cmd` row → `exit_code` or `tests_pass` or `command_output_matches`
- [ ] Bash-only rows replaced or documented with platform deps
- [ ] `[harness].id` set for telemetry
- [ ] `zagens gate validate --file …` passes
- [ ] Smoke: `zagens queue add … --gate-file …` or stage_gate manifest load
- [ ] Deliverables: migrated to predicates or explicitly left in completion_gate

---

## MicroStack regression

| Artifact | Role |
|----------|------|
| [`fixtures/harness/microstack-completion-gate.toml`](../../../fixtures/harness/microstack-completion-gate.toml) | Full legacy regression (Layer-2 + Layer-3) |
| [`presets/go-microstack-migrated.toml`](./presets/go-microstack-migrated.toml) | Predicate-native Layer-2 subset for Gate-as-Code demos |

Do **not** delete the legacy fixture — it validates the composable harness path. Use the migrated preset for queue / external sharing.
