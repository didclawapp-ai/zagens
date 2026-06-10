# Sandbox capability matrix (A6.1)

**Status:** Living doc aligned with `crates/runtime-server/src/sandbox/` and Zagens settings (`sandbox_mode`).

## Summary

| Platform | Backend | Policy declared (`sandbox_mode`) | Process isolation enforced | User notice |
|----------|---------|----------------------------------|----------------------------|-------------|
| **macOS** | Seatbelt (`sandbox-exec`) | Yes | **Yes** when `sandbox-exec` is present | None |
| **macOS** | — | Yes | **No** if `sandbox-exec` missing | Degraded mode (startup + optional elevation UI) |
| **Linux** | Landlock (planned) | Yes | **No** — env marker only (`DEEPSEEK_SANDBOX_UNENFORCED`) | Degraded mode + per-command stderr prefix |
| **Windows** | **Elevated** (recommended) | Yes | **Yes** when `zagens sandbox setup` completed | Settings shows enforced copy; Online user has full outbound network (§13.7) |
| **Windows** | **Unelevated** (fallback) | Yes | **Yes** — write isolation + weak network env | No profile read isolation (G0 Fail); Settings note |
| **Windows** | — | Yes | **No** — before cap/setup artifacts | Degraded mode + setup CTA |
| **All** | `danger-full-access` / external OpenSandbox | Config | OpenSandbox replaces local exec when configured | See `sandbox_backend` in config |

## Windows elevated (Phase 2 / G2)

| Capability | Elevated offline user | Elevated online user (`network_access: true`) |
|------------|----------------------|-----------------------------------------------|
| Workspace write | Yes (restricted token + ACL) | Yes |
| Write outside workspace | **No** | **No** |
| Profile read (`.ssh`, etc.) | **No** (grant-exclusion + deny-read) | **No** |
| System read (`Program Files`, profile root grant) | Yes | Yes |
| Outbound network | **Blocked** (WFP per-SID; loopback permitted) | **Unrestricted** — no host allowlist |
| DNS resolution side channel | System Dnscache may still resolve names; data connections blocked | N/A |
| Background `exec_shell` | Yes (runner IPC) | Yes |
| Setup / teardown | `zagens sandbox setup` / `teardown` (Admin/UAC) | Same |

Default mode when `[windows] sandbox` is unset: **elevated** if setup is complete, else **unelevated** (PR-2.12).

Acceptance probes (maintainer): `cargo run --example g2_acceptance -p zagens-windows-sandbox` (12 checks).

## Policy modes (`sandbox_mode` / `SandboxPolicy`)

| Mode | Filesystem | Network | Notes |
|------|------------|---------|-------|
| `read-only` | Read-all, no writes | Off by default | macOS enforced via Seatbelt profile |
| `workspace-write` | Read-all; write CWD + optional roots | Optional flag | Default recommended policy |
| `danger-full-access` | Unrestricted | Yes | Escalation / YOLO; project config may deny |
| `external-sandbox` | Delegates to host container | Configurable | Avoid double-sandboxing |

## Enforcement signals

- **`DEEPSEEK_SANDBOX`**: backend marker (`seatbelt`, `landlock`, `windows:…`).
- **`DEEPSEEK_SANDBOX_UNENFORCED=1`**: policy declared but OS isolation not applied (Linux; Windows plan failure fallback).
- **Shell tool**: `ShellResult.sandbox_enforced`, `sandbox_denial_code` (Win32, PR-2.13); stderr warning when `enforced: false`.

## UI surfacing (A6.2)

| Surface | Behavior |
|---------|----------|
| **TUI** | `policy_degraded_mode_notice()` logged once at interactive startup (`target: sandbox`). |
| **Zagens (Windows)** | Settings → enforced copy when elevated setup complete; setup hint when not; degraded on Linux. |
| **Zagens (non-Windows, non-macOS)** | `settings.sandboxDegradedMode` when not enforced. |
| **macOS Zagens** | No banner when Seatbelt available. |

## Backlog

- Linux: Landlock/bwrap helper — see `doc_Private/docs/tech/LINUX_SANDBOX_DESIGN.md`.
- Windows Phase 3: ConPTY interactive sandbox (PR-3.1), optional private desktop (PR-3.2).
- Optional: unify degraded copy across TUI status line and desktop toast.

## References

- Design (maintainer): `doc_Private/docs/tech/WINDOWS_SANDBOX_DESIGN.md`
- Implementation: `crates/windows-sandbox/`, `crates/runtime-server/src/sandbox/`
- Desktop settings: `crates/desktop/web-ui/src/components/SettingsPanel.tsx`
