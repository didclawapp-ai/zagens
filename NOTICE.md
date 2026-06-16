# Third-Party Notices

**Zagens** is released under the [MIT License](LICENSE) at the repository root.

This file records **additional attribution** for components with separate copyright holders embedded in Zagens.

## Agent runtime lineage (deepseek-tui / CodeWhale, MIT)

The embedded agent runtime sidecar incorporates code whose lineage traces to deepseek-tui / CodeWhale. The runtime is built from the Rust crates under `crates/` (agent, config, core, runtime-server, tools, etc.).

| Field | Value |
|-------|-------|
| Component | Embedded agent runtime (sidecar) |
| Lineage | deepseek-tui / CodeWhale |
| License | MIT |
| Copyright | DeepSeek CLI Contributors (2024–2025) |
| Full license text | [third-party/deepseek-tui/LICENSE](third-party/deepseek-tui/LICENSE) |

Per the MIT license, the copyright and permission notice in that file must be retained in copies or substantial portions of the runtime components.

**Installed builds:** `bundle:prepare` stages license texts into the desktop bundle under `legal/` (`zagens-LICENSE.txt`, `deepseek-tui-runtime-LICENSE.txt`, `THIRD-PARTY-NOTICES.txt`) next to the application binary.

**Engine divergence (Kernel v3, 2026-06):**

- **From Zagens v0.7.x:** The agent turn engine under `crates/core/src/engine/` (event-sourced `KernelEvent` log, `TurnMachine` / `EffectInterpreter`, `KernelTurnHost` seam) **diverges from upstream CodeWhale / deepseek-tui** for the kernel loop and session resume substrate.
- **From Zagens v0.7.6 (Kernel v3 Phase 3b batch 5 closure):** The legacy turn loop is removed; production uses `LiveTurnMachine` + `EffectInterpreter` + `V3TurnHost`; session resume is log-first (`log_transcript_repair` default on). `TurnLoopHost` remains only as a deprecated adapter shim. Merge with upstream turn-engine code is no longer feasible—only cherry-pick of peripheral modules (tools, desktop shell, MCP).

See `doc_Private/docs/tech/AGENT_KERNEL_V3_PHASE3_DESIGN.md`.

## Other dependencies

Rust crates, npm packages, bundled Python, Tauri, React, and other libraries are subject to their respective licenses in `Cargo.lock`, package lockfiles, and vendor manifests.
