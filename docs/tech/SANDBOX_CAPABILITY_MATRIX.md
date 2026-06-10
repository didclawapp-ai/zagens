# Sandbox capability matrix (A6.1)

**Status:** Living doc — aligned with `crates/runtime-server/src/sandbox/`, `crates/windows-sandbox/`, and Zagens Settings → **Sandbox** (`SandboxSettingsPanel.tsx`). **Windows native sandbox (Phase 0–3) is implemented** as of Zagens 0.7.x.

## Summary

| Platform | Backend | Policy declared (`sandbox_mode`) | Process isolation enforced | User notice |
|----------|---------|----------------------------------|----------------------------|-------------|
| **macOS** | Seatbelt (`sandbox-exec`) | Yes | **Yes** when `sandbox-exec` is present | None |
| **macOS** | — | Yes | **No** if `sandbox-exec` missing | Degraded mode (startup + optional elevation UI) |
| **Linux** | Landlock (planned) | Yes | **No** — env marker only (`DEEPSEEK_SANDBOX_UNENFORCED`) | Degraded mode + per-command stderr prefix |
| **Windows** | **Elevated** (recommended) | Yes | **Yes** when `zagens sandbox setup` completed | Settings → enforced; Online user has full outbound network (§13.7) |
| **Windows** | **Unelevated** (fallback) | Yes | **Yes** — write isolation + weak network env | No profile read isolation (G0 Fail); Settings note |
| **Windows** | — | Yes | **No** — before setup / uninitialized install | First-run onboarding wizard + setup CTA |
| **All** | `danger-full-access` / external OpenSandbox | Config | OpenSandbox replaces local exec when configured | See `sandbox_backend` in config |

## Windows native sandbox (implemented)

Implementation: `crates/windows-sandbox/` (restricted token + ACL + WFP + elevated helper binaries). Runtime wiring: `crates/runtime-server/src/sandbox/mod.rs`, `tools/shell/windows_sandbox.rs`.

### Elevated (Gate G2)

| Capability | Elevated offline user | Elevated online user (`network_access: true`) |
|------------|----------------------|-----------------------------------------------|
| Workspace write | Yes (restricted token + ACL) | Yes |
| Write outside workspace | **No** | **No** |
| Profile read (`.ssh`, etc.) | **No** (grant-exclusion + deny-read) | **No** |
| System read (`Program Files`, profile root grant) | Yes | Yes |
| Outbound network | **Blocked** (WFP per-SID; loopback permitted) | **Unrestricted** — no host allowlist |
| DNS resolution side channel | System Dnscache may still resolve names; data connections blocked | N/A |
| Background `exec_shell` | Yes (runner IPC) | Yes |
| Interactive `exec_shell` (`tty: true`) | Yes (ConPTY via command-runner) | Yes |
| Session read-dir CLI | `zagens sandbox add-read-dir <path>` | Same |
| Optional private desktop | `[windows] sandbox_private_desktop = true` (default on wizard elevated path) | Same |
| Setup / teardown | `zagens sandbox setup` / `teardown` (Admin/UAC) | Same |

Default when `[windows] sandbox` is unset: **elevated** if setup is complete, else **unelevated**.

### Unelevated (Gate G1)

| Capability | Unelevated |
|------------|------------|
| Workspace write | Yes (restricted token + cap-SID ACL) |
| Write outside workspace | **No** |
| Profile read (`.ssh`, etc.) | **Not isolated** (G0 PoC Fail — cap SID deny-read ineffective on read path) |
| Outbound network | Best-effort env/PATH poison — **not** WFP-enforced |
| Background / interactive shell | Yes (same-process restricted token) |
| Setup required | No — works on fresh install without UAC |

### Config (`~/.zagens/config.toml`)

| Key | Values | Notes |
|-----|--------|-------|
| `[windows] sandbox` | `elevated` \| `unelevated` | Also `DEEPSEEK_WINDOWS_SANDBOX` env |
| `[windows] sandbox_private_desktop` | bool | Optional isolated desktop for child processes |
| `[windows] sandbox_initialized` | bool | Set by desktop first-run wizard; gates full Settings UI |
| Global `sandbox_mode` | `read-only` \| `workspace-write` \| … | Applies across platforms |

CLI: `zagens sandbox setup` · `teardown` · `add-read-dir` · `poc deny-read` (G0 PoC).

### Acceptance probes (maintainers)

| Gate | Command | Pass |
|------|---------|------|
| **G0** | `zagens sandbox poc deny-read` | Records Pass/Fail in `unelevated_deny_read_poc.json` — **Fail** in production (read isolation not claimed for unelevated) |
| **G1** | `cargo test -p zagens-windows-sandbox` + Agent unelevated probes | Write isolation + enforced spawn |
| **G2** | `cargo run --example g2_acceptance -p zagens-windows-sandbox` | **14/14** when setup complete (incl. ConPTY + add-read-dir) |

```powershell
$env:ZAGENS_HOME = "<home with completed setup>"
# Workspace under Documents\Zagens\… — not under grant-excluded ~/.zagens
cargo run --example g2_acceptance -p zagens-windows-sandbox
```

Report: `{ZAGENS_HOME}/.sandbox/g2_acceptance_report.json`. Quick smoke: `g2_debug_spawn`, `g2_net_probe`. Teardown residuals: `g2_teardown_verify` (**destructive** — dedicated home only).

## Policy modes (`sandbox_mode` / `SandboxPolicy`)

| Mode | Filesystem | Network | Notes |
|------|------------|---------|-------|
| `read-only` | Read-all, no writes | Off by default | macOS enforced via Seatbelt profile |
| `workspace-write` | Read-all; write CWD + optional roots | Optional flag | Default recommended policy |
| `danger-full-access` | Unrestricted | Yes | Escalation / YOLO; project config may deny |
| `external-sandbox` | Delegates to host container | Configurable | Avoid double-sandboxing |

Enterprise overlay (optional): `allowed_windows_sandbox_modes`, `require_windows_sandbox_setup` in `requirements.toml` — example [`fixtures/harness/windows-enterprise-requirements.toml`](../../fixtures/harness/windows-enterprise-requirements.toml).

## Enforcement signals

- **`DEEPSEEK_SANDBOX`**: backend marker (`seatbelt`, `landlock`, `windows:elevated`, `windows:unelevated`, …).
- **`DEEPSEEK_SANDBOX_UNENFORCED=1`**: policy declared but OS isolation not applied (Linux; Windows when plan/spawn fails).
- **Shell tool**: `ShellResult.sandbox_enforced`, `sandbox_denial_code` (Win32); `windows_sandbox_mode` in metadata; stderr warning when `enforced: false`.
- **`diagnostics` tool**: configured vs effective Windows mode, setup completion.

## UI surfacing (A6.2)

| Surface | Behavior |
|---------|----------|
| **TUI** | `policy_degraded_mode_notice()` logged once at interactive startup (`target: sandbox`). |
| **Zagens → Sandbox (Windows)** | **First run:** onboarding wizard only (elevated + UAC provisioning or unelevated). **After init:** full panel — global `sandbox_mode`, `[windows] sandbox`, private desktop, configured vs effective status, setup hint when elevated without artifacts. |
| **Zagens (Linux)** | `settings.sandboxDegradedMode` when not enforced. |
| **macOS Zagens** | No banner when Seatbelt available. |

## Backlog

- **Linux:** Landlock/bwrap helper — see `doc_Private/docs/tech/LINUX_SANDBOX_DESIGN.md`.
- **Windows hardening (non-blocking):** DNS name-resolution layer WFP filters; optional unified degraded copy across TUI status line and desktop toast.

## References

- Design (maintainer): `doc_Private/docs/tech/WINDOWS_SANDBOX_DESIGN.md` · `WINDOWS_SANDBOX_IMPLEMENTATION_PLAN.md`
- Implementation: `crates/windows-sandbox/`, `crates/runtime-server/src/sandbox/`
- Desktop UI: `crates/desktop/web-ui/src/components/SandboxSettingsPanel.tsx`
