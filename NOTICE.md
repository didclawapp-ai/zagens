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

## Other dependencies

Rust crates, npm packages, bundled Python, Tauri, React, and other libraries are subject to their respective licenses in `Cargo.lock`, package lockfiles, and vendor manifests.
