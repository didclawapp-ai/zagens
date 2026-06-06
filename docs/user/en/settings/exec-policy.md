# Execution policy

Control **what the agent is allowed to do** on your machine under **Settings → System → Security**.

## Sandbox mode

| Mode | Effect |
|------|--------|
| **Workspace write** | Read/write inside workspace root (default) — `workspace-write` |
| **Read only** | No file writes — `read-only` |
| **Full access** | Broader write scope — **`danger-full-access`** (use sparingly) |

Maps to `sandbox_mode` in `config.toml`. The desktop **Settings → System** dropdown may show “Full access” (internal value `full-access`); after save, treat **`danger-full-access`** in config as authoritative.

## Feature toggles

| Setting | Controls |
|---------|----------|
| **Shell tool** | `exec_shell` family |
| **Web search** | `web_search` / `fetch_url` / `web.run` |
| **Exec policy** | Runtime enforcement of sandbox + tool policy |
| **Sub-agents** | `agent_spawn` and related tools |

## Approval policy

Separate dropdown: **on-request**, **untrusted**, **never**, **auto**. See [Tool approval](/docs/settings/approval) for dialog behavior.

## External sandbox

Optional OpenSandbox backend routes shell over HTTP instead of local spawn — configure in `config.toml` (`sandbox_backend = "opensandbox"`).

## Tips

- Daily coding: `workspace-write` + `on-request` approval.
- Air-gapped: disable web search and shell.
- Office sessions ignore shell regardless of toggle.

Related: [Network policy](/docs/settings/network) · [Shell tools](/docs/tools/shell)
