# 文档读写能力增强方案

> 状态：已审核通过，可进入实施阶段  
> 审核日期：2026-05-10  
> 涉及 crate：`crates/tui`（ReadFileTool 扩展 + WriteOfficeTool 新增）、`crates/desktop`（无改动）

---

## 1. 现状分析

### 1.1 现有格式检测链

[ReadFileTool](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/tools/file.rs#L77-L188) 已有格式检测链，按优先级依次尝试：

```
read_file 执行路径:
  ├── is_pdf()?  →  pdftotext (外部命令) 或 pdf-extract (纯 Rust fallback)
  ├── is_docx()? →  zip::ZipArchive → 读 word/document.xml → 正则提取 <w:t>
  └── else       →  常规文本流式读取 (UTF-8 / GB18030 / Windows-1252)
```

- `is_pdf()` (L194-213)：双重检测 — 后缀名 `.pdf` + `%PDF-` 魔数字节嗅探
- `is_docx()` (L426-432)：仅检查 `.docx` 后缀名
- `read_docx()` (L434-501)：ZIP 解包 → 正则 `<w:t[^>]*>(.*?)</w:t>` → 按 `</w:p>` 分段输出

### 1.2 已有可复用依赖

[`crates/tui/Cargo.toml`](file:///f:/DeepSeek-TUI-desktop/crates/tui/Cargo.toml) 中已存在：

| 依赖 | 用途 |
|------|------|
| `zip = "2.2"` | ZIP 解包 (OOXML 容器) |
| `regex = "1.11"` | 正则提取 XML 标签文本 |
| `pdf-extract = "0.7"` | PDF 纯 Rust 回退 |
| `encoding_rs = "0.8"` | 编码检测 |

DOCX 读取已落地，为 XLSX / PPTX 提供了现成的模式模板。

---

## 2. 读取增强（Phase 1 — 纯增量、零新依赖）

### 2.1 XLSX 文本提取 (Phase 1a)

**OOXML 结构：**

```
file.xlsx (ZIP)
├── xl/sharedStrings.xml       ← 共享字符串表 (SSI)
├── xl/worksheets/sheet1.xml   ← 单元格引用 (<c r="A1" t="s"><v>0</v></c>)
├── xl/worksheets/sheet2.xml
├── xl/workbook.xml            ← 工作表名称列表
└── xl/_rels/workbook.xml.rels ← sheet rId → 文件路径映射
```

**关键点：**

- `t="s"` 表示值是 SSI 索引，需从 `sharedStrings.xml` 查表
- `t="inlineStr"` 是内联字符串（`<is><t>`），不走 SSI
- `t="str"` 是公式结果字符串（极少见）
- 多 sheet 需三层间接引用：`workbook.xml` → `r:id` → `workbook.xml.rels` → `worksheets/sheetN.xml`

**输出格式：**

```
=== Sheet: 数据汇总 ===
[A1] 项目名称  [B1] 金额  [C1] 日期
[A2] 开发费用  [B2] 50000  [C2] 2025-01-15
...
```

**实施要点：**

- `is_xlsx()` — 扩展名 `.xlsx` 检测
- `read_xlsx()` — 同 `read_docx` 模式，ZIP 解包 + XML 解析
- 共享字符串表可能极大（MB 级），建议逐元素流式提取而非 `read_to_string`
- 估计代码量：~180-200 行

### 2.2 PPTX 文本提取 (Phase 1b)

**OOXML 结构：**

```
file.pptx (ZIP)
├── ppt/slides/slide1.xml      ← 正文 (DrawingML 命名空间 a:)
├── ppt/slides/slide2.xml
├── ppt/notesSlides/notesSlide1.xml  ← 演讲者备注 (可选)
└── ppt/slides/_rels/          ← 每页的图片/媒体关系 (非顺序)
```

**幻灯片顺序方案：** 枚举 `ppt/slides/slide1..N.xml`，按文件名数字排序。ZIP 内部文件名是稳定排序的，与 99% PPTX 实际结构一致（已在 `read_docx`/`read_xlsx` 的 ZIP 条目枚举中验证同模式可行），无需额外解析 `presentation.xml`。

**正则：** `<a:t[^>]*>(.*?)</a:t>` — 不需担心 `<a:tab>` (自闭合标签，无 `</a:tab>`)。

**输出格式：**

```
=== Slide 1 ===
标题文本
正文内容...

=== Slide 2 ===
...
```

**实施要点：**

- `is_pptx()` — 扩展名 `.pptx` 检测
- `read_pptx()` — 遍历 `ppt/slides/slideN.xml`，提取 `<a:t>` 文本
- 估计代码量：~80-100 行

### 2.3 DOCX 读取增强（可选 Phase 1c）

当前只提取 `<w:t>` 纯文本。可增强以保留结构：
- `<w:pStyle>` → 标题层级标注
- `<w:tbl>` → 表格输出
- `<w:numPr>` → 列表编号

这是锦上添花，非必需。

### 2.4 集成改造点（Phase 1）

| 改造点 | 文件 | 说明 |
|--------|------|------|
| `is_xlsx()` + `read_xlsx()` | `crates/tui/src/tools/file.rs` | 加入 ReadFileTool 执行链 |
| `is_pptx()` + `read_pptx()` | `crates/tui/src/tools/file.rs` | 同上 |
| `ReadFileTool::description()` | `crates/tui/src/tools/file.rs` | 更新描述，声明 xlsx/pptx 支持 |

---

## 3. 生成方案（Phase 2 — write_office 统一入口）

### 3.1 总体架构：单入口 + 多引擎 + 显式兜底

```
write_office(format, path, payload)
        │
        ├── XLSX → 首选: rust_xlsxwriter (纯 Rust, 零 Python)
        │
        ├── DOCX → 首选: Python + python-docx
        │           兜底: 纯 Rust 最小 OOXML (标题/段落/简单列表)
        │
        └── PPTX → Python + python-pptx
                    无 Rust 兜底 → 返回清晰错误 + 环境指引
```

**设计原则：**

- 对外只暴露一个工具 `write_office`，`format` ∈ `{xlsx, docx, pptx}`
- 内部按格式选引擎，优先级写死在 Rust 层
- 引擎路径写入 tool result metadata（便于排障）

### 3.2 各格式决策依据

| 格式 | 引擎选择 | 理由 |
|------|---------|------|
| **XLSX** | `rust_xlsxwriter` (纯 Rust) | 1.3K+ stars, MIT, 活跃维护, API 覆盖 95% 场景。**放弃 Python 兜底** — 该 crate 完全胜任 |
| **DOCX** | Python + `python-docx` 为首选 | python-docx 质量极高，版式/样式/表格/图片全面支持。Rust 兜底仅覆盖标题/段落/简单列表（**不含表格** — 当 payload 含 `type: "table"` block 且 Python 不可用时直接返回错误） |
| **PPTX** | Python + `python-pptx` 唯一 | 无纯 Rust 库可用。失败即返回环境指引 |

**新增依赖：**

| 依赖 | 用途 | 体积增量 |
|------|------|---------|
| `rust_xlsxwriter` | XLSX 生成 | ~150KB 编译产物 |

### 3.3 Rust 基础设施（所有生成路径共用）

#### 3.3.1 Python 检测 — `find_python()`

**目标：** 为 RLM、code_execution、write_office 提供统一的可复用函数。

**候选链：**
```
python3 → python → py -3 (Windows)
```

**推荐 API 形状：**

```rust
/// Returns (binary_name, major, minor) if Python ≥ min_version is found.
/// Python version extracted via `sys.version_info[:2]`, compared as (u16, u16).
fn find_python(min_version: Option<(u16, u16)>) -> Option<(String, u16, u16)> {
    let candidates: &[&[&str]] = if cfg!(windows) {
        &[&["python3"], &["python"], &["py", "-3"]]
    } else {
        &[&["python3"], &["python"]]
    };
    for args in candidates {
        let mut cmd = Command::new(args[0]);
        cmd.args(&args[1..])
            .args(["-c", "import sys; print(sys.version_info[0], sys.version_info[1])"]);
        if let Ok(output) = cmd.output() {
            if let Some((major, minor)) = parse_version_tuple(&output.stdout) {
                if min_version.map_or(true, |(min_maj, min_min)| {
                    major > min_maj || (major == min_maj && minor >= min_min)
                }) {
                    return Some((args[0].to_string(), major, minor));
                }
            }
        }
    }
    None
}
```

> **无新增依赖**：版本比较用 `(u16, u16)` 元组，避免引入 `semver` crate。Python 端输出 `sys.version_info[:2]` 两个整数，足够满足 `≥ 3.8` 的单一需求。

**改动影响范围：**

| 位置 | 当前行为 | 改动后 |
|------|---------|--------|
| `crates/tui/src/repl/runtime.rs:L182` | 硬编码 `Command::new("python3")` | 使用 `find_python(None)?.0` |
| `crates/tui/src/core/engine/tool_catalog.rs:L434` | 硬编码 `tokio::process::Command::new("python3")` | 使用 `find_python(None)?.0` |
| `crates/tui/src/tools/office_write.rs` (新建) | — | 使用 `find_python(Some((3, 8)))` |

#### 3.3.2 专用 venv — `~/.deepseek/office-py/`

**优于全局 `pip --user`，原因：**

- 避免 Debian/Ubuntu PEP 668 的 `--break-system-packages` 问题
- 版本隔离：`python-docx` / `python-pptx` / `openpyxl` 版本 pin 在 `requirements-office.txt`
- 与现有 `~/.deepseek/` 目录风格一致

**首次执行流程：**

```
find_python ≥ 3.8 ✓
    ↓
~/.deepseek/office-py/ 不存在？
    ↓
python3 -m venv ~/.deepseek/office-py/
    ↓
~/.deepseek/office-py/bin/pip install -r requirements-office.txt
    ↓
写入 ~/.deepseek/office-py/.requirements-installed-v1 版本戳
```

**Windows 适配：** `venv/bin/python` → `venv\Scripts\python.exe`

#### 3.3.3 脚本交付

遵循已有 `include_str!` 模式（与 [子代理 prompt 嵌入](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/tools/subagent/mod.rs#L3834) 一致）：

```
crates/tui/assets/scripts/
├── write_docx.py    ← include_str!("../../assets/scripts/write_docx.py")
└── write_pptx.py    ← include_str!("../../assets/scripts/write_pptx.py")
```

**部署逻辑：**

1. 编译期：Python 脚本通过 `include_str!` 嵌入二进制
2. 首次执行：脚本写入 `~/.deepseek/scripts/` + 版本戳标记文件
3. 升级时：检测版本戳 → 覆盖旧脚本
4. 参考实现：[skills/system.rs 的安装逻辑](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/skills/system.rs#L9-L51)

#### 3.3.4 执行模型

```rust
async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
    // 在 spawn_blocking 中执行同步 Command 以避免阻塞 async runtime
    tokio::task::spawn_blocking(move || {
        let mut child = Command::new(python_bin)
            .arg(script_path)
            .arg("--output").arg(&output_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // stdin 只传数据 payload（不重复传 path/format）
        serde_json::to_writer(child.stdin.take().unwrap(), &data_payload)?;

        let output = child.wait_with_timeout(timeout)?;
        // 解析输出，构建 ToolResult
    }).await?
}
```

**关键点：**

- 与 shell.rs 同为 `std::process::Command`（同步）
- `spawn_blocking` 保证不阻塞 tokio runtime
- 超时 + kill：Python 脚本是单进程执行，`child.kill()` 即可
- **stdin 协议：CLI 传 `--output`/`--format`，stdin 只传纯数据 payload**，避免 path 出现在两个通道

#### 3.3.5 路径安全

走现有 `context.resolve_path()` — 已有完整的 `..` 逃逸检测 + 信任路径支持。

### 3.4 Payload 设计原则

```jsonc
// XLSX
{
  "format": "xlsx",
  "path": "output.xlsx",
  "sheets": [
    {
      "name": "Sheet1",
      "rows": [
        ["项目名称", "金额", "日期"],        // headers
        ["开发费用", 50000.0, "2025-01-15"],
        [null, null, null]                    // null → 空单元格
      ],
      "column_widths": [20, 12, 15]          // 可选
    }
  ]
}

// DOCX
{
  "format": "docx",
  "path": "output.docx",
  "title": "项目报告",                        // 可选，文档标题
  "blocks": [
    { "type": "heading", "level": 1, "text": "第一章" },
    { "type": "paragraph", "text": "普通段落内容..." },
    { "type": "list", "style": "bullet", "items": ["项目A", "项目B"] },
    { "type": "list", "style": "number", "items": ["第一步", "第二步"] },
    { "type": "table",
      "headers": ["名称", "数量", "备注"],      // 需要 Python 引擎
      "rows": [["项目A", "10", "已完成"], ["项目B", "5", "进行中"]]
    }
  ]
}

// PPTX
{
  "format": "pptx",
  "path": "output.pptx",
  "slides": [
    {
      "title": "封面标题",
      "bullets": ["要点1", "要点2"],
      "notes": "演讲者备注"                // 可选
    },
    {
      "title": "第二页",
      "bullets": ["数据展示"],
      "notes": null
    }
  ]
}
```

### 3.5 工具注册与能力声明

**注册点：** 在 `with_agent_tools()` 中新增 `.with_office_write_tool()`，Agent / YOLO 模式可用。
**不在 `with_file_tools()` 中注册** — `with_file_tools()` 被 `with_read_only_file_tools()` 链调用，会进入 Plan 模式。`WriteOfficeTool` 写文件 + spawn Python 子进程，Plan 模式不应可见。

**延迟加载：** 默认 always-loaded（加入 `should_default_defer_tool` allowlist）。理由：`write_office` 有明确的应用场景（用户说"生成一个表格/文档"），模型应当首轮就能调用，不需要先通过 ToolSearch 发现。token 开销是一次性的（~300 tokens 的工具 schema）。

**能力声明：**

```rust
impl ToolSpec for WriteOfficeTool {
    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![
            ToolCapability::WritesFiles,        // 生成 .xlsx/.docx/.pptx
            ToolCapability::ExecutesCode,       // spawn Python 子进程
            ToolCapability::RequiresApproval,
        ]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Suggest            // 写文件触发建议性审批
    }

    fn supports_parallel(&self) -> bool {
        true                                    // 文件生成不与读取冲突
    }
}
```

| 改造点 | 文件 | 说明 |
|--------|------|------|
| `WriteOfficeTool` 实现 | `crates/tui/src/tools/office_write.rs` (新建) | ToolSpec 实现，含 capabilities 声明 |
| 模块注册 | `crates/tui/src/tools/mod.rs` | `pub mod office_write;` |
| 工具注册 | `crates/tui/src/tools/registry.rs` | `with_agent_tools()` 中注册（非 `with_file_tools()`） |
| always-loaded | `crates/tui/src/core/engine/tool_catalog.rs` | 加入 `should_default_defer_tool` allowlist |

---

## 4. 双模式适配验证

DS Pick 桌面模式和 TUI 直连模式共享同一执行路径：

```
模型 → tool call → deepseek-tui serve (同一进程)
                     → WriteOfficeTool::execute()
                       → Command::new("python3").arg(script) ✅
                       → rust_xlsxwriter::Workbook::new()  ✅
```

在这两种模式下 `deepseek-tui serve` 都是执行者，`std::process::Command` 和纯 Rust 库均能正常工作。不需要 Tauri 特殊机制。

---

## 5. Python 打包策略（产品决策）

### 5.1 推荐分阶段

| 阶段 | 策略 | 说明 |
|------|------|------|
| **Phase 1 (立即)** | **不打包**。`find_python()` + venv + 引导提示 | 先验证功能价值。XLSX 零依赖。DOCX/PPTX 需 Python |
| **Phase 2 (发布前)** | 打包 `python-build-standalone` (~15MB 压缩后增量) | 开箱即用，不依赖用户环境 |

**理由：**

- Phase 1 的 `find_python()` + venv 是无论如何都需要的基础设施（也改善 RLM）
- 用户群体是开发者，安装 Python 不是障碍
- Python 打包涉及多平台构建矩阵（Win/Mac/Linux × x64/arm64），不应阻塞核心功能

### 5.2 打包方案（Phase 2）

```
prepare-bundle.mjs 扩展:
  1. 下载 python-build-standalone 对应平台版本
  2. 解压到 binaries/python-standalone/
  3. 创建 venv + pip install 依赖包
  4. 配置 Tauri resources 打包

tauri.conf.json 新增:
  "resources": { "binaries/python-standalone/*": "python/" }
```

**代价估算：**

| 方面 | 说明 |
|------|------|
| 体积 | 解释器 + 3 个库 ≈ 30-50MB (压缩后 ~15MB) |
| 多平台 | Win/Mac/Linux × x64/arm64 各一套 |
| 更新 | 随应用版本升级 Python 及依赖 |
| 签名 | macOS 公证需处理解释器作为应用资源 |
| 许可证 | PSF License (Python) + MIT (各库)，需 NOTICE 留痕 |

---

## 6. 风险矩阵

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| XLSX 共享字符串表 OOM | 大文件读取崩溃 | 逐元素流式提取，加行数截断 |
| `python3` 不在 PATH (Windows) | 生成失败 | `find_python()` 多候选链 + 清晰安装指引 |
| venv 首次创建耗时长 | 用户体验差 | 考虑首次启动时预建 venv（非阻塞后台任务） |
| OOXML 加密文件 | 读取失败 | 捕获 ZIP 错误，返回明确提示 |
| python-docx/openpyxl API 变动 | 生成失败 | `requirements-office.txt` 版本 pin |
| DOCX Rust 兜底不足 | 用户期望落空 | 显式说明能力边界（无表格/图片等） |
| 旧格式 (.xls, .ppt) | 不支持 | 明确只支持 OOXML (.xlsx, .docx, .pptx) |

---

## 7. 完整文件改动清单

```
新建文件:
  crates/tui/src/tools/office_write.rs         WriteOfficeTool 实现 (~300行)
  crates/tui/src/python_env.rs                  find_python() + venv 管理 (~150行)
  crates/tui/assets/scripts/write_docx.py       DOCX Python 脚本 (~60行)
  crates/tui/assets/scripts/write_pptx.py       PPTX Python 脚本 (~50行)

改动文件:
  crates/tui/Cargo.toml                         新增 rust_xlsxwriter 依赖
  crates/tui/src/tools/mod.rs                   注册 office_write 模块
  crates/tui/src/tools/file.rs                  新增 is_xlsx/read_xlsx/is_pptx/read_pptx
  crates/tui/src/tools/registry.rs              with_agent_tools() 注册 WriteOfficeTool（非 with_file_tools）
  crates/tui/src/core/engine/tool_catalog.rs    should_default_defer_tool 加 write_office
  crates/tui/src/repl/runtime.rs                硬编码 python3 → find_python()
  crates/tui/src/core/engine/tool_catalog.rs    硬编码 python3 → find_python()

未改动:
  crates/desktop/                               无需改动 (sidecar 是 deepseek-tui serve 本身)

实施后需记录:
  root/CHANGELOG.md                             新增 DS Pick 行记录：read_file 扩展 xlsx/pptx 支持 + 新增 write_office 工具
```

---

## 8. 实施建议

### Phase 1a — XLSX 读取（优先）
- 无新依赖，~180-200 行
- 立即可读项目中的数据文件
- 可复用 `read_docx` 的 ZIP + 正则模式

### Phase 1b — PPTX 读取
- 无新依赖，~80-100 行
- 枚举 `ppt/slides/slideN.xml` 按数字排序，直接 ZIP 条目遍历

### Phase 2 前置基建 — `find_python()` + venv
- 先兑现基础设施（改善 RLM + code_execution）
- `python_env.rs` ~150 行

### Phase 2a — XLSX 写入
- 新增 `rust_xlsxwriter`，~200-250 行
- 零 Python，开箱即用

### Phase 2b — DOCX/PPTX 写入
- 依赖 Python 基建（`find_python` + venv）
- Python 脚本各 ~50 行，Rust 工具层 ~150 行
- DOCX Rust 兜底 ~120 行（仅标题/段落/简单列表）
