# DS Pick

> **DS Pick** — 面向 [DeepSeek V4](https://platform.deepseek.com) 的**桌面端**编程助手。**Tauri 2** 应用（代码在 [`crates/desktop/`](crates/desktop/)），React + 系统 WebView 前端，通过 **HTTP/SSE** 连接与 CLI 相同的**本地智能体运行时**（接口说明见 [`docs/RUNTIME_API.md`](docs/RUNTIME_API.md)）。

[English README](README.md)

[![CI](https://github.com/Hmbown/DeepSeek-TUI/actions/workflows/ci.yml/badge.svg)](https://github.com/Hmbown/DeepSeek-TUI/actions/workflows/ci.yml)
[![npm](https://img.shields.io/npm/v/deepseek-tui)](https://www.npmjs.com/package/deepseek-tui)
[![crates.io](https://img.shields.io/crates/v/deepseek-tui-cli?label=crates.io)](https://crates.io/crates/deepseek-tui-cli)

## 功能摘要

- 原生窗口内的对话、工具调用、会话与工作区  
- **工作台** — 工作区目录树、快照与恢复（需运行时信任模式）、API Key 等  
- **Sidecar 运行时** — `deepseek-tui serve --http` 为壳提供本地 API（可随包内置或使用自建二进制）

实施说明、构建与预览架构：**[`docs/desktop/README.md`](docs/desktop/README.md)** · **[`docs/desktop/PREVIEW_ARCHITECTURE.md`](docs/desktop/PREVIEW_ARCHITECTURE.md)**

## 从源码构建

需要 **Rust 1.88+**、用于前端的 **Node.js**，以及 [Tauri v2 环境](https://v2.tauri.app/start/prerequisites/)。

```bash
cd crates/desktop
npm ci
npm run bundle:prepare    # Vite 生产构建并将 deepseek-tui 放入 binaries/
cargo build -p deepseek-desktop
```

发布安装包：在 `crates/desktop` 下执行 `npx @tauri-apps/cli@2 build`。

## 单仓：CLI、TUI 与共享引擎

本仓库是**单一 Cargo workspace**：除桌面壳外，仍包含 `deepseek` 调度器、**终端 TUI** 以及 `core` / `tools` / `protocol` 等共享 crate。以**终端 TUI 为主角**的旧版根目录 README 已归档在 **[`docs/archive/tui-readme-era/`](docs/archive/tui-readme-era/ABOUT.md)**（安装方式、快捷键、定价表与完整功能列表仍以该文档为准）。

| 主题 | 文档 |
|------|------|
| 运行时 HTTP/SSE | [docs/RUNTIME_API.md](docs/RUNTIME_API.md) |
| 架构 | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| CLI / TUI 安装 | [docs/INSTALL.md](docs/INSTALL.md) |
| 归档的根 README（TUI 为主） | [docs/archive/tui-readme-era/README.zh-CN.md](docs/archive/tui-readme-era/README.zh-CN.md) |
| TUI 相关文档（提示词分析、依赖图、交接与评审） | [docs/tui/README.md](docs/tui/README.md) |
| 更新日志 | [CHANGELOG.md](CHANGELOG.md) |
| 贡献指南 | [CONTRIBUTING.md](CONTRIBUTING.md) |

## 许可证

[MIT](LICENSE)

> 本项目与 DeepSeek Inc. 无隶属关系。
