# Sandbox capability matrix (A6.1)

**Status:** Living doc aligned with `crates/tui/src/sandbox/` and DS Pick settings (`sandbox_mode`).

## Summary

| Platform | Backend | Policy declared (`sandbox_mode`) | Process isolation enforced | User notice |
|----------|---------|----------------------------------|----------------------------|-------------|
| **macOS** | Seatbelt (`sandbox-exec`) | Yes | **Yes** when `sandbox-exec` is present | None |
| **macOS** | — | Yes | **No** if `sandbox-exec` missing | Degraded mode (startup + optional elevation UI) |
| **Linux** | Landlock (planned) | Yes | **No** — env marker only (`DEEPSEEK_SANDBOX_UNENFORCED`) | Degraded mode + per-command stderr prefix |
| **Windows** | AppContainer / helper (planned) | Yes | **No** — env marker only | Degraded mode + per-command stderr prefix |
| **All** | `danger-full-access` / external OpenSandbox | Config | OpenSandbox replaces local exec when configured | See `sandbox_backend` in config |

## Policy modes (`sandbox_mode` / `SandboxPolicy`)

| Mode | Filesystem | Network | Notes |
|------|------------|---------|-------|
| `read-only` | Read-all, no writes | Off by default | macOS enforced via Seatbelt profile |
| `workspace-write` | Read-all; write CWD + optional roots | Optional flag | Default recommended policy |
| `danger-full-access` | Unrestricted | Yes | Escalation / YOLO; project config may deny |
| `external-sandbox` | Delegates to host container | Configurable | Avoid double-sandboxing |

## Enforcement signals

- **`DEEPSEEK_SANDBOX`**: which backend wrapper was selected (`seatbelt`, `landlock`, `windows:…`).
- **`DEEPSEEK_SANDBOX_UNENFORCED=1`**: Linux/Windows paths where policy is declared but not applied (see `mark_sandbox_policy_unenforced` in `sandbox/mod.rs`).
- **Shell tool**: prepends [`ExecEnv::sandbox_enforcement_warning`](../../crates/tui/src/sandbox/mod.rs) to stderr when applicable.

## UI surfacing (A6.2)

| Surface | Behavior |
|---------|----------|
| **TUI** | `policy_degraded_mode_notice()` logged once at interactive startup (`target: sandbox`). |
| **DS Pick** | Settings → Sandbox mode shows `settings.sandboxDegradedMode` when `platform !== 'darwin'`. |
| **macOS DS Pick** | No banner (Seatbelt expected); TUI still fully enforced on macOS CLI. |

## Backlog (A6.3)

- Linux: Landlock helper binary (apply ruleset → exec child) — see comment in `prepare_landlock`.
- Windows: AppContainer / Restricted token / Windows Sandbox integration — see `sandbox/windows.rs`.
- Optional: unify degraded copy across TUI status line and desktop toast.

## References

- Roadmap: [RUNTIME_EVOLUTION_ROADMAP.md](./RUNTIME_EVOLUTION_ROADMAP.md) § A6
- Implementation: `crates/tui/src/sandbox/{mod,policy,seatbelt,landlock,windows}.rs`
- Desktop settings: `crates/desktop/web-ui/src/components/SettingsPanel.tsx`
