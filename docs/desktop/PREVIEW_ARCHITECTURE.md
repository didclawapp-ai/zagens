# Zagens 桌面端 — 文档预览架构

> **状态（2026-05）**：Phase 1 已在 Zagens（`crates/desktop` + `web-ui`）落地。本文档作为 **架构说明 + 约束索引 + 后续 Phase 2 备忘**。

## 实施结果概览

| 范围 | 说明 |
|---|---|
| 前端模块 | `crates/desktop/web-ui/src/components/preview/` — `types`、`detector`、`PreviewContainer`、`PreviewDispatcher`、`renderers/*` |
| 入口 | `RightPanel.tsx` — 工作台文件点击 → overlay；文本走 runtime API，二进制走 Tauri |
| 二进制读取 | Tauri **`read_thread_workspace_binary`**（`crates/desktop/src/commands.rs`）：`thread_id` + `relative_path`，Rust 内 `GET /v1/threads/:id` 取 workspace 根后，按 runtime 同源规则 `canonicalize` + 根前缀校验 |
| CSP | `tauri.conf.json` 已含 `img-src 'self' data:` |
| Markdown 兼容 | `MarkdownPreview.tsx` → 仅 re-export `MarkdownRenderer`，`@deprecated` |
| PDF / Office | **二进制可读 + 占位 UI**（`OfficePlaceholder`），内嵌渲染为 Phase 2 |
| 截断提示 | `PreviewState.truncated`；`ImageRenderer` 与占位页展示超过 10 MiB 截断说明 |

## 背景约束（runtime API，仍适用）

| 约束 | 位置 | 影响 |
|---|---|---|
| 非 UTF-8 被拒绝 | `crates/tui/src/runtime_api.rs` — `workspace/file` 使用 `String::from_utf8` | 图片 / PDF / Office **不能**用该 HTTP 端点读正文 |
| 文本上限 512 KB | `MAX_WORKSPACE_FILE_BYTES` | 超大纯文本需改常量或另开桌面读路径（未做） |
| `language_hint` | 服务端 `language_from_name` | 前端 **`detectFileType`** 优先用 hint；`markdown` → `MarkdownRenderer` |

## 支持格式与渲染器

| 格式 | 渲染器 | 备注 |
|---|---|---|
| Markdown（`.md` / `.mdx` 等） | `MarkdownRenderer` | markdown-it + DOMPurify |
| 代码 / 带 `language_hint` 的文本 | `CodeRenderer` | highlight.js |
| 纯文本 / 未知 | `TextRenderer` | HTML escape |
| 图片 | `ImageRenderer` | `data:{mime};base64,...` |
| CSV / TSV | `CsvRenderer` | HTML `<table>` |
| PDF / Office | **占位页** | 已含文件大小；`truncated` 时额外说明 |

## 数据流（实现）

```
用户点击文件
  ├→ detectFileType(title)（二进制路由；无 language_hint）
  ├→ isBinaryFileType → invoke('read_thread_workspace_binary', { threadId, relativePath })
  │     （Rust：Bearer 调本机 runtime → thread.workspace + 安全拼接相对路径）
  └→ 否则 readThreadWorkspaceFile(threadId, relPath)
```

## 二进制读取（`read_thread_workspace_binary`）

- **参数**：`thread_id`、`relative_path`（与 HTTP `workspace/file` 的 `path` 语义一致，`..` 禁止）。
- **工作区根**：`http://127.0.0.1:{port}/v1/threads/{id}`，解析 JSON `thread.workspace`（与 browse API 同源）。
- **读上限**：`PREVIEW_MAX_BINARY_BYTES`（10 MiB），超出则截断，`truncated: true`；`sniff_mime` 魔数判断 MIME。
- **依赖**：`base64`、`reqwest`、`serde_json`（`crates/desktop/Cargo.toml`）。

## Phase 1 清单 — 已闭合

| # | 任务 | 状态 |
|---:|---|---|
| 1–15 | 预览模块、CSP、hljs、`read_thread_workspace_binary`、路径校验、truncated UI | ✅ |

## Phase 2（未排期）

| 项 | 说明 |
|---|---|
| `PdfRenderer` / `OfficeRenderer` | pdf.js、SheetJS / mammoth 等；CSP 扩展 |
| 聊天 / 工具内可点击路径 | 产品增量 |

### Office / PDF 选型备忘

- **Office.js**：Add-in 宿主 API，非通用本地 docx 嵌入方案。
- **Office Online iframe**：常需可访问 URL，与纯本地工作区需额外架构。
- **React 友好**：SheetJS、mammoth + DOMPurify、pptx 单独 POC 或商用 Viewer。

## 安全约束

| 约束 | 说明 |
|---|---|
| 路径 | 仅 **`thread_id` 对应的 runtime workspace 根** + 相对路径；禁止 `..` 与越界 |
| 鉴权 | 读线程详情使用 **与 sidecar 相同的 Bearer token**（`AppContext.runtime_token`） |
| 穿越 | `canonicalize` + `starts_with(workspace_base)`（与 `runtime_api.rs` `safe_thread_subpath` 一致） |
| 体积 | 文本 512 KB（API）；二进制最多读 10 MiB 用于预览 |

---

**依赖**：桌面 shell 须能访问本机 runtime；二进制预览失败时检查 sidecar 是否就绪、线程是否存在。
