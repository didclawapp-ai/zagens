# Predicate reference (Gate-as-Code)

All machine success checks go through **`long_horizon/predicate::*`**. Gate-as-Code `[[verify]]` rows reference these names only.

Registry source: `zagens_core::long_horizon::predicates` (keep in sync with runtime `predicate/types.rs`).

---

## Summary

| Predicate | Exec | Typical use |
|-----------|------|-------------|
| `exit_code` | async (shell) | Arbitrary command; exit 0 = pass |
| `tests_pass` | async (shell) | Canonical `cargo test` / `go test ./...` |
| `file_exists` | sync | Path relative to workspace |
| `file_count` | sync | Glob min/max file counts |
| `command_output_matches` | sync (native) | In-process probes (`grep`, `gofmt -l`, `test ! -d …`) |

---

## `exit_code`

Run a shell command; **exit code 0 = pass**.

```toml
[[verify]]
id = "build"
predicate = "exit_code"
args = { cmd = "go build ./..." }
```

| Arg | Required | Notes |
|-----|----------|-------|
| `cmd` or `command` | ✓ | Passed to platform shell wrapper |
| `timeout_ms` | | Override `[verify_budget].timeout_ms` per row (future) |

Uses `resolve_command_root` for nested `go.mod` / `Cargo.toml` layouts.

---

## `tests_pass`

Shorthand for common test commands; delegates to `exit_code`.

```toml
[[verify]]
id = "unit_tests"
predicate = "tests_pass"
args = { toolchain = "cargo" }

[[verify]]
id = "go_tests"
predicate = "tests_pass"
args = { toolchain = "go" }

[[verify]]
id = "custom"
predicate = "tests_pass"
args = { cmd = "cargo test -p my-crate" }
```

| Arg | Default | Values |
|-----|---------|--------|
| `toolchain` | `auto` | `cargo`, `go`, `auto` |
| `package` | — | Cargo `-p` package name |
| `cmd` | — | Full override |

---

## `file_exists`

```toml
[[verify]]
id = "readme"
predicate = "file_exists"
args = { path = "README.md" }
```

| Arg | Required |
|-----|----------|
| `path` | ✓ (workspace-relative) |

---

## `file_count`

```toml
[[verify]]
id = "deliverables"
predicate = "file_count"
args = { glob = "deliverables/**/*.docx", min = 1, max = 10 }
```

| Arg | Required | Default |
|-----|----------|---------|
| `glob` | ✓ | — |
| `min` | | `0` |
| `max` | | unlimited |

---

## `command_output_matches`

Native in-process probes (no full shell spawn). **Exit 0 with assertion semantics** for special cases like `gofmt -l`.

```toml
[[verify]]
id = "gofmt_clean"
predicate = "command_output_matches"
args = { command = "gofmt -l ." }

[[verify]]
id = "no_stub_marker"
predicate = "command_output_matches"
args = { command = "grep -c todo! src/lib.rs" }
```

Supported native patterns include `grep`, `gofmt -l`, and `test ! -d path` — see `verify_platform.rs`.

---

## Validation errors

`zagens gate validate` fails when:

- Unknown `predicate` name
- Duplicate `[[stages]].id`
- `requires` references missing stage
- `[[verify]].stage` references missing stage
- Empty contract (no stages and no verify rows)

Warnings (non-fatal): empty `[harness].id`, flat verify row without `id`, staged contract without stage-bound verify.
