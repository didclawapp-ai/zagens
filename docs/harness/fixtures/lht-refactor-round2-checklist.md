# LHT round-2 checklist — Electron→Tauri polish (label_rust class)

Use after the **first** LHT pass when `cargo check` is green but cross-layer gaps remain
(`electron/` still on disk, frontend build not verified, legacy tree not removed).

**LabelMakePro 实测 Round 2（43× `not_impl` 补全）：** 见专用清单
[`lht-label-rust-round2-checklist.md`](./lht-label-rust-round2-checklist.md)（含可粘贴开场指令）。

Paste into a new thread or append as a second checklist segment. Each item should carry
`[verify: …]` where noted.

## Round 2 — integration & cleanup

1. **Remove legacy Electron tree** — delete or fully migrate `electron/`; no duplicate main process
   - `[verify: test ! -d electron]`（harness 原生目录探测，Windows 无需 bash）

2. **Frontend production build** — Vite/webpack bundle succeeds against Tauri APIs
   - `[verify: npm run build]`

3. **Adapter shim wired** — `tauri-api.ts` (or `desktop-api.ts`) assigns `window.electronAPI`
   - Manual: grep shows shim import in app entry

4. **IPC smoke** — top 3 user flows invoke Rust commands without console errors
   - Document which commands were exercised

5. **Release binary** — backend release build + bundle step
   - `[verify: cargo build --release]` (from `src-tauri/` if polyglot)

6. **Deliverables manifest** — optional `.zagens/lht-deliverables.toml` lists any IPC not in `commands/`
   - See [`lht-deliverables.example.toml`](./lht-deliverables.example.toml)

## Strict-mode expectations

With **LHT strict** + completion gate enforce:

- Residual `electron/` → **integration enforce** nudge (not prose-only completion)
- `first_gap_count > 0` or integration gaps → UI **conditionally complete** (amber)
