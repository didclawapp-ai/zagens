# DS Pick 桌面端 — 文档预览架构方案

## 现状

| 模块 | 位置 | 问题 |
|---|---|---|
| 预览渲染 | `MarkdownPreview.tsx` | 只对 `.md` 做 markdown-it；非 Markdown 文件退化为 HTML escape，无语法高亮 |
| 预览 UI | `RightPanel.tsx` | overlay 模式已实现（点击文件 → 预览覆盖工作台 → 关闭预览回到工作台），但预览逻辑全部内联在面板组件中 |
| 文件读取 | sidecar HTTP API `/v1/threads/:id/workspace/file` | 只返回 UTF-8 字符串，不支持二进制文件（图片、PDF、Office） |
| Tauri 命令 | `commands.rs` | 只有 token/port/theme/api-key 相关，无文件读取命令 |
| 组件目录 | `components/` | 平铺结构，无子目录 |
| Tauri CSP | `tauri.conf.json` | 缺少 `img-src data:`，无法加载 base64 图片 |

### 代码审核发现的约束（2026-05-09）

| 约束 | 位置 | 影响 |
|---|---|---|
| **runtime API 明确拒绝非 UTF-8 内容** | `crates/tui/src/runtime_api.rs:1204` — `String::from_utf8(bytes).map_err(|_| ApiError::bad_request("file is not UTF-8 text; binary preview not supported"))` | 所有二进制文件（图片/PDF/Office）**必须**走 Tauri command，不能回退到 runtime API |
| **文本文件大小上限 512 KB** | `crates/tui/src/runtime_api.rs:891` — `MAX_WORKSPACE_FILE_BYTES = 512 * 1024` | 超过 512KB 的文本文件被 runtime API 拒绝；需要大文本预览需提升此常量或走 Tauri command |
| **`base64` crate 未引入** | `crates/desktop/Cargo.toml` — 依赖列表无 base64 | 实施二进制预览 Tauri command 前必须添加 |
| **CSP 无 `img-src`** | `tauri.conf.json` — `default-src 'self'` 会拦截 `data:` URI | 图片预览完全无法渲染；必须添加 `img-src 'self' data:` |
| **`language_from_name()` 已存在** | `crates/tui/src/runtime_api.rs:1028` — 服务端已做扩展名→语言映射 | 前端 `detector.ts` 应优先使用 API 返回的 `language_hint`，仅对二进制文件走自身映射 |

## 目标

支持以下格式的预览，每种渲染器独立文件，由 Dispatcher 统一路由。

| 格式 | Phase | 渲染器 |
|---|---|---|
| Markdown (`.md`) | 1 | `MarkdownRenderer` — markdown-it |
| 代码 (`.rs` `.ts` `.tsx` `.js` `.json` `.toml` `.yaml` `.py` `.sh` `.css` `.html` `.sql` …) | 1 | `CodeRenderer` — hljs |
| 纯文本 (`.txt` `.log` 无后缀) | 1 | `TextRenderer` — HTML escape |
| 图片 (`.png` `.jpg` `.gif` `.svg` `.webp`) | 1 | `ImageRenderer` — `<img>` base64 |
| CSV / TSV (`.csv` `.tsv`) | 1 | `CsvRenderer` — HTML `<table>` |
| PDF (`.pdf`) | 2 | `PdfRenderer` — pdf.js 或 iframe |
| Office (`.docx` `.xlsx` `.pptx`) | 2 | `OfficeRenderer` — Web 解析预览（优选与 React 栈匹配的库）；失败或 pptx 等场景 **兜底**「用系统默认程序打开」 |

## 目录结构

```
src/components/preview/                  ← 新建
├── index.ts                             ← barrel 导出（Dispatcher + 类型）
├── types.ts                             ← FileType 枚举、PreviewState、RendererProps
├── detector.ts                          ← 扩展名 + language_hint → FileType
├── PreviewContainer.tsx                 ← overlay 壳（关闭按钮 + 文件名 + 内容区）
├── PreviewDispatcher.tsx                ← 根据 FileType 分派渲染器
└── renderers/
    ├── index.ts
    ├── MarkdownRenderer.tsx             ← 从 MarkdownPreview.tsx 提取
    ├── CodeRenderer.tsx                 ← hljs 直接高亮
    ├── TextRenderer.tsx                 ← 纯文本 / 未知格式 fallback
    ├── ImageRenderer.tsx                ← base64 <img>
    ├── CsvRenderer.tsx                  ← HTML <table>
    ├── PdfRenderer.tsx                  ← Phase 2
    └── OfficeRenderer.tsx              ← Phase 2
```

### RightPanel.tsx 改动

- 删除内联的 `MarkdownPreview` 引用和 `PreviewState` 类型定义
- 改为从 `./preview` 导入：`PreviewContainer`、`PreviewDispatcher`、`detectFileType`
- 预览 overlay 区域替换为：

```tsx
<PreviewContainer title={preview.title} onClose={closePreview}>
  <PreviewDispatcher state={preview} />
</PreviewContainer>
```

### MarkdownPreview.tsx 处理

- 原有逻辑迁移至 `preview/renderers/MarkdownRenderer.tsx`
- `MarkdownPreview.tsx` 保留为兼容 re-export，标记 `@deprecated`，后续删除
- hljs 注册的 6 种语言（plaintext/javascript/typescript/rust/bash/json/markdown）随迁移带走，`CodeRenderer` 不再重复注册

## 核心类型

```typescript
// types.ts

export enum FileType {
  Markdown = 'markdown',
  Code = 'code',
  Text = 'text',
  Image = 'image',
  Csv = 'csv',
  Pdf = 'pdf',
  Office = 'office',
  Unknown = 'unknown',
}

export interface PreviewState {
  title: string;
  fileName?: string;
  content: string;        // 文本内容 或 base64（图片等二进制）
  language?: string;      // API 返回的 language_hint
  fileType: FileType;     // 由 detector 填充
  size?: number;          // bytes（磁盘上完整大小；截断预览时与已加载内容长度可不同）
  mimeType?: string;      // 二进制文件的 MIME（如 image/png）
  truncated?: boolean;    // 二进制预览：是否仅加载了前 N 字节
}

export interface RendererProps {
  state: PreviewState;
}
```

## 检测器

```typescript
// detector.ts — 纯函数，零依赖

const EXT_MAP: Record<string, FileType> = {
  '.md': FileType.Markdown,
  '.markdown': FileType.Markdown,
  // 以下代码类扩展名仅用于二进制文件路由判断；文本格式的语言选择
  // 优先使用 runtime API 返回的 language_hint（见 language_from_name）
  '.rs': FileType.Code, '.ts': FileType.Code, '.tsx': FileType.Code,
  '.js': FileType.Code, '.jsx': FileType.Code, '.json': FileType.Code,
  '.toml': FileType.Code, '.yaml': FileType.Code, '.yml': FileType.Code,
  '.py': FileType.Code, '.sh': FileType.Code, '.bash': FileType.Code,
  '.css': FileType.Code, '.html': FileType.Code, '.htm': FileType.Code,
  '.sql': FileType.Code, '.xml': FileType.Code, '.java': FileType.Code,
  '.go': FileType.Code, '.c': FileType.Code, '.h': FileType.Code,
  '.cpp': FileType.Code, '.hpp': FileType.Code,
  '.png': FileType.Image, '.jpg': FileType.Image, '.jpeg': FileType.Image,
  '.gif': FileType.Image, '.svg': FileType.Image, '.webp': FileType.Image,
  '.csv': FileType.Csv, '.tsv': FileType.Csv,
  '.pdf': FileType.Pdf,
  '.docx': FileType.Office, '.xlsx': FileType.Office, '.pptx': FileType.Office,
  '.txt': FileType.Text, '.log': FileType.Text,
};

export function detectFileType(
  fileName?: string,
  languageHint?: string | null
): FileType {
  // 1. language_hint 存在 → FileType.Code（文本文件已在 API 侧通过了 UTF-8 校验）
  // 2. 从 fileName 取扩展名匹配 EXT_MAP
  // 3. fallback → FileType.Text
}
```

## 后端补充 — 二进制文件读取

当前文本文件走 sidecar HTTP API（`/v1/threads/:id/workspace/file`），返回 JSON `{ content: string }`。该端点**明确拒绝非 UTF-8 内容**（`crates/tui/src/runtime_api.rs:1204`），因此二进制文件**只能**通过 Tauri command 读取——不存在 fallback 路径。

二进制文件（图片、PDF、Office）需要新增 Tauri command（下称 `read_thread_workspace_binary`，名称以实现为准）。

### WebView 信任边界（必须）

前端传来的**任意绝对路径字符串都不可单独作为信任依据**（拼接自 `browseThreadWorkspace` 的 `workspace` 字段亦然）：WebView 被误导或滥用时，`canonicalize` 只能规范化路径，不能代替「属于当前线程工作区」的授权。

**推荐契约**（二选一，优先 A）：

| 方案 | 参数 | Rust 侧行为 |
|---|---|---|
| **A（推荐）** | `thread_id` + `relative_path`（与 HTTP `/workspace/file` 相同的 `path` 语义） | 在桌面进程中解析该线程的**工作区根绝对路径**（与 sidecar 使用的根一致：例如缓存最近一次 browse 返回的 `workspace` + 随 `thread_id` 校验，或向 runtime 查询路径），用 `Path`/`PathBuf` 拼接 `relative_path`，`canonicalize` 后**前缀校验**必须在根之下 |
| **B** | `relative_path` | 在应用状态中登记「当前选中线程」时已写入**canonical 根目录**，command 仅收相对路径并在该根下拼接 |

禁止仅依赖 `absolute_path: String` 且无与线程/根绑定的校验。

### 路径与跨平台

- API 侧 `relative_path` 使用 `/`；Rust 端用 `PathBuf` 拼接并 `canonicalize`，避免在 TS 里手写 `joinPath` 处理 Windows 反斜杠与 `..`。
- 若 browse 尚未返回 `workspace` 就触发预览，应先等待列表加载完成或失败重试，避免根目录未就绪。

### Rust 侧（示意）

```rust
// commands.rs 新增 — 参数形状以方案 A 为准

#[derive(Serialize)]
pub struct BinaryFileResponse {
    pub mime_type: String,
    pub base64: String,
    pub size: u64,
    pub truncated: bool,
}

/// 单次读取并 Base64 编码的最大字节数（与产品「截断预览」策略一致，见下文「特殊处理」）
const PREVIEW_MAX_BINARY_BYTES: u64 = 10 * 1024 * 1024; // 10 MiB

#[tauri::command]
pub async fn read_thread_workspace_binary(
    thread_id: String,
    relative_path: String,
) -> Result<BinaryFileResponse, String> {
    // 1. 解析 canonical workspace_root（与 runtime 一致）
    // 2. 合并 relative_path → candidate，`canonicalize`，并 assert 以 workspace_root 为前缀
    // 3. metadata → size；若 size > PREVIEW_MAX_BINARY_BYTES，仅读前 PREVIEW_MAX_BINARY_BYTES 字节，truncated = true
    // 4. content sniff → mime_type（失败可回退扩展名）
    // 5. read 前缀 → base64编码
    todo!()
}
```

- 依赖：`base64` crate **需新增到 `crates/desktop/Cargo.toml`**（当前未引入）

### 前端调用

```typescript
// 在 RightPanel.onOpenFile 中 — 不传绝对路径给 Rust
const fileType = detectFileType(fileName);

if (fileType === FileType.Image || fileType === FileType.Pdf || fileType === FileType.Office) {
  const bin = await invoke<BinaryFileResponse>('read_thread_workspace_binary', {
    threadId: resumedThreadId,
    relativePath: relPath,
  });
  openPreview({
    title: fileName,
    fileName,
    content: bin.base64,
    fileType,
    size: bin.size,
    mimeType: bin.mime_type,
    truncated: bin.truncated,
  });
} else {
  const file = await readThreadWorkspaceFile(resumedThreadId, relPath);
  openPreview({
    title: fileName,
    fileName,
    content: file.content,
    language: file.language_hint ?? undefined,
    fileType: detectFileType(fileName, file.language_hint),
  });
}
```

## 数据流

```
用户点击文件
  │
  ├→ detector.detectFileType(fileName)
  │
  ├── if Image/Pdf/Office → invoke('read_thread_workspace_binary', threadId, relativePath)
  │                           → { mime_type, base64, size, truncated }
  │      （二进制必须走 Tauri；HTTP 端点拒绝非 UTF-8；路径授权在 Rust 内完成）
  │
  └── else (Markdown/Code/Text/Csv) → readThreadWorkspaceFile(threadId, relativePath)
                                       → { content, language_hint, truncated }
  │
  ▼
PreviewState { title, fileName, content, fileType, language?, size?, mimeType?, truncated? }
  │
  ▼
PreviewContainer（overlay 壳）
  │
  ▼
PreviewDispatcher(fileType)
  ├── Markdown → MarkdownRenderer
  ├── Code     → CodeRenderer
  ├── Text     → TextRenderer
  ├── Image    → ImageRenderer
  ├── Csv      → CsvRenderer
  ├── Pdf      → PdfRenderer       (Phase 2)
  └── Office   → OfficeRenderer    (Phase 2)
```

## 渲染器职责

| 渲染器 | 输入 | 输出 | 依赖 |
|---|---|---|---|
| `MarkdownRenderer` | `content: string` | markdown-it → DOMPurify → HTML | markdown-it, DOMPurify |
| `CodeRenderer` | `content` + `language` | hljs.highlight → `<pre><code>` | highlight.js |
| `TextRenderer` | `content: string` | HTML-escaped `<pre>` | 无 |
| `ImageRenderer` | `content: base64` + `mimeType` | `<img src="data:{mime};base64,{content}">` | 无；见安全节 **SVG** |
| `CsvRenderer` | `content: string` | parse → `<div class="overflow-x-auto"><table>` | 无（手写 parser） |
| `PdfRenderer` | 待定 | pdf.js 或 `<iframe src="file://...">` | pdf.js (Phase 2) |
| `OfficeRenderer` | 二进制或路径 | 优先 Web 内嵌（xlsx/docx 等）；不可用则提示并「用系统默认程序打开」 | Phase 2 选型见下文 |

### 特殊处理

- **大文件与截断（Phase 1 约定）**
  - **文本**：大于 runtime API 上限（512KB）时请求失败；若需预览更大文本，须改服务端常量或增加「仅桌面」读取路径（本节不展开）。
  - **二进制**：大于 `PREVIEW_MAX_BINARY_BYTES`（如 10 MiB）时，**仅读取前 N 字节**，`truncated: true`，UI 明确提示「预览已截断」。若后续产品改为整文件拒绝，可将 command 改为返回 `Err` 即可，与本文另一策略互斥，避免文档与实现不一致。
- **二进制误检测**：Content sniff 失败 → 回退扩展名或 `application/octet-stream`；若与扩展名严重不符可提示用户。
- **空文件**：显示「空文件」占位

## Phase 1 实施清单

| # | 任务 | 文件 | 类型 |
|---|---|---|---|
| 1 | 新建 `preview/types.ts` — FileType 枚举、PreviewState、RendererProps | 新文件 | 前端 |
| 2 | 新建 `preview/detector.ts` — 扩展名映射 + detectFileType() | 新文件 | 前端 |
| 3 | 新建 `preview/renderers/CodeRenderer.tsx` — hljs 直接高亮 | 新文件 | 前端 |
| 4 | 新建 `preview/renderers/TextRenderer.tsx` — 纯文本 escape | 新文件 | 前端 |
| 5 | 从 `MarkdownPreview.tsx` 提取 → `preview/renderers/MarkdownRenderer.tsx` | 提取 | 前端 |
| 6 | 新建 `preview/renderers/ImageRenderer.tsx` — base64 `<img>` | 新文件 | 前端 |
| 7 | 新建 `preview/renderers/CsvRenderer.tsx` — HTML table | 新文件 | 前端 |
| 8 | 新建 `preview/PreviewDispatcher.tsx` — 路由到渲染器 | 新文件 | 前端 |
| 9 | 新建 `preview/PreviewContainer.tsx` — 从 RightPanel 提取 overlay UI | 新文件 | 前端 |
| 10 | 新建 `preview/renderers/index.ts` + `preview/index.ts` | 新文件 | 前端 |
| 11 | 重构 `RightPanel.tsx` — 删除内联预览逻辑，接入 preview 模块；`onOpenFile` 中按 FileType 分流调用不同 API | 修改 | 前端 |
| 12 | Rust: 新增 `read_thread_workspace_binary(thread_id, relative_path)`（或等价命名）+ workspace 根解析与前缀校验 + 注册 `invoke_handler` | `commands.rs`, `main.rs` 等 | Rust |
| 13 | 补充 hljs 语言注册（toml, yaml, python, css, html, sql） | `CodeRenderer.tsx` | 前端 |
| 14 | `MarkdownPreview.tsx` 保留为 re-export 兼容层 | 修改 | 前端 |
| 15 | Tauri CSP: 添加 `img-src 'self' data:` | `tauri.conf.json` | 配置 |

## Phase 2 规划

| # | 任务 |
|---|---|
| 16 | `PdfRenderer` — pdf.js + base64 或 blob URL；纳入 CSP 评审（常见需 `worker-src` / `blob:` 等，与 Phase 1 仅 `img-src` 不同） |
| 17 | `OfficeRenderer` — 见下节「Office 内嵌预览选型」；兜底仍为「用系统默认程序打开」 |
| 18 | 聊天消息中文件路径自动检测 → 可点击 chip → 右侧预览 |
| 19 | AI 工具调用结果中的文件引用自动可点击 |

### Office 内嵌预览选型（与 Vue 方案的关系）

- **Microsoft Office.js** — **不适用**于本场景的「在内嵌 WebView 里预览工作区里的 `.docx/.xlsx/.pptx`」。Office.js 面向 **Office 外接程序（Add-in）**：脚本运行在 **Word / Excel / PowerPoint 等宿主** 内，通过 `Office` 全局 API 操作**当前宿主中已打开的文档**。它**不是**可在任意 React 应用里加载任意本地 Office 文件的通用渲染库；与 DS Pick 的 **Tauri + 本地线程工作区** 预览目标**不对应**。
- **「Office Viewer」类嵌入**（常指 Office Online / `officeapps.live.com` 等 **iframe** 查看）：多数实现依赖 **由微软服务拉取的文档 URL**（公开可访问或符合其接入方式的端点）。当前产品路径是 **runtime 线程工作区本地文件**，若直接套用，往往需要 **上传、临时可访问链接、或自建可被该服务访问的网关**，并带来 **隐私合规、必须联网、CSP（如 `frame-src`）与白名单** 等额外成本；采用前需单独评审。
- 社区里确有 **Vue 侧封装**（例如围绕表格/文档的 `vue-xlsx` 类组件；`vue-docs`、`vue-pptx` 等以 npm 上**实际包名与许可证**为准再选型）。思路是：在浏览器里解析 Office 格式并渲染，而不是只弹「外部打开」。
- **本仓库桌面 web-ui 技术栈为 React + Vite**（`crates/desktop/web-ui/package.json`），**默认不再引入 Vue runtime**。若要坚持使用仅提供 Vue 组件的库，等价选项是：维护 **Vue 子应用/微前端**（工程与产物体积成本显著），或寻找 **与框架无关** / **React** 的同类能力。
- **与当前栈更贴合的候选**（Phase 2 可评估）：**xlsx**（SheetJS 等）用于表格；**mammoth.js** 将 **docx** 转 HTML（输出需 **`DOMPurify` 消毒**）；**pptx** 在浏览器内「忠实排版预览」通常最难，库成熟度与体积需单独 POC；亦可评估 **商用文档 WebViewer SDK**（许可与包体积需单独评估）。**仍可保留系统关联程序打开作兜底**。
- 表格中「降级」含义不变：**解析失败、体积超限、或暂无可靠内嵌方案时**，通过 Tauri（如 `shell` / 系统 `open`）**用外部程序打开**本地文件。

## 安全约束

| 约束 | 实现位置 |
|---|---|
| WebView 与路径授权 | 二进制读取 command **必须**绑定 `thread_id` + `relative_path`（或与已登记 workspace 根绑定），在 Rust 内解析 canonical 根并做前缀校验；**禁止**仅信任前端传入的绝对路径 |
| 路径穿越防护 | `canonicalize` + 工作区根前缀校验；相对路径分段拒绝 `..` 或拼接后统一规范化再校验 |
| 文件大小上限 | 文本: runtime API 512KB；二进制: 单次编码不超过 `PREVIEW_MAX_BINARY_BYTES`（截断策略见「特殊处理」） |
| 二进制检测 | Rust: magic bytes / 扩展名 → MIME |
| XSS 防护 | `DOMPurify.sanitize()` — 已有，`MarkdownRenderer` 复用 |
| CSP 图片加载 | `tauri.conf.json` 增加 `img-src 'self' data:`（否则 `data:` 图片被 `default-src` 拦截） |
| SVG | `data:image/svg+xml` 经 `<img>` 仍存在脚本/CDATA 类风险讨论；Phase 1 可选：对 `.svg` 降级为「文本/Code 预览」、仅栅格格式走 `ImageRenderer`，或引入有限清洗策略；需与产品一致后写死 |
| HTML 文件安全 | 默认不渲染 HTML，归类为 Text 显示源码 |