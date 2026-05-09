# DS Pick

> **DS Pick** — Desktop coding companion for [DeepSeek V4](https://platform.deepseek.com). A **Tauri 2** application ([`crates/desktop/`](crates/desktop/)) with a React + system WebView UI that drives the same **local agent runtime** as our CLI over **HTTP/SSE** (see [`docs/RUNTIME_API.md`](docs/RUNTIME_API.md)).

[简体中文 README](README.zh-CN.md)

[![CI](https://github.com/Hmbown/DeepSeek-TUI/actions/workflows/ci.yml/badge.svg)](https://github.com/Hmbown/DeepSeek-TUI/actions/workflows/ci.yml)
[![npm](https://img.shields.io/npm/v/deepseek-tui)](https://www.npmjs.com/package/deepseek-tui)
[![crates.io](https://img.shields.io/crates/v/deepseek-tui-cli?label=crates.io)](https://crates.io/crates/deepseek-tui-cli)

## Features

- Chat, tools, sessions, and workspace-aware flows from a native window  
- **Workbench** — workspace tree, snapshot / restore (with runtime trust mode), API key helpers  
- **Sidecar runtime** — `deepseek-tui serve --http` powers the local API used by the shell (bundled or your own build)

Implementation notes, build matrices, and preview behavior: **[`docs/desktop/README.md`](docs/desktop/README.md)** · **[`docs/desktop/PREVIEW_ARCHITECTURE.md`](docs/desktop/PREVIEW_ARCHITECTURE.md)**

## Build from source

Requires **Rust 1.88+**, **Node.js** for the web UI, and [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
cd crates/desktop
npm ci
npm run bundle:prepare    # Vite production build + stage `deepseek-tui` into binaries/
cargo build -p deepseek-desktop
```

Release / installer: from `crates/desktop`, run `npx @tauri-apps/cli@2 build`.

## Monorepo: CLI, TUI, and shared engine

This repository is a **single workspace**: desktop shell, `deepseek` dispatcher, terminal TUI, and shared crates (`core`, `tools`, `protocol`, …). The historical README that centered the **terminal TUI** is archived under **[`docs/archive/tui-readme-era/`](docs/archive/tui-readme-era/ABOUT.md)** (install, shortcuts, pricing tables, and the full feature list still apply to the CLI/TUI).

| Topic | Document |
|--------|----------|
| Runtime HTTP/SSE API | [docs/RUNTIME_API.md](docs/RUNTIME_API.md) |
| Architecture | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| CLI / TUI install | [docs/INSTALL.md](docs/INSTALL.md) |
| Archived root README (TUI-led) | [docs/archive/tui-readme-era/README.md](docs/archive/tui-readme-era/README.md) |
| TUI lineage docs (prompts, deps, handoff, reviews) | [docs/tui/README.md](docs/tui/README.md) |
| Changelog | [CHANGELOG.md](CHANGELOG.md) |
| Contributing | [CONTRIBUTING.md](CONTRIBUTING.md) |

## License

[MIT](LICENSE)

> Not affiliated with DeepSeek Inc.
