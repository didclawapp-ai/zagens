# Third-Party Notices

**Zagens** is a proprietary desktop product — see [LICENSE](../LICENSE) at the repository root.

This file records **third-party** components embedded in Zagens. They are not part of the Zagens product license.

## Agent runtime (deepseek-tui lineage, MIT)

Zagens embeds an agent runtime sidecar whose source lineage is MIT-licensed third-party code. The runtime is built from the Rust crates under `crates/` (agent, config, core, tui, tools, etc.).

| Field | Value |
|-------|-------|
| Component | Embedded agent runtime (sidecar) |
| Lineage | deepseek-tui / CodeWhale (third-party) |
| License | MIT |
| Copyright | DeepSeek CLI Contributors (2024–2025) |
| Full license text | [third-party/deepseek-tui/LICENSE](deepseek-tui/LICENSE) |

Per the MIT license, the copyright and permission notice in that file must be retained in copies or substantial portions of the runtime components.

## Other dependencies

Rust crates, npm packages, bundled Python, Tauri, React, and other libraries are subject to their respective licenses in `Cargo.lock`, package lockfiles, and vendor manifests.
