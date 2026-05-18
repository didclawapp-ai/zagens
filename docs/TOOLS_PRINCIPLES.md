# DS Pick 工具系统原理

> 版本: 0.2.1 | 最后更新: 2026-05-18 | 基于仓库实际代码

---

## 1. 架构总览

DS Pick 的工具系统采用 **统一抽象 + 分层注册** 的架构。所有工具实现同一个 `ToolSpec` trait，统一注册到 `ToolRegistry` 中，由引擎在 Agent 循环中调度执行。

```
┌─────────────────────────────────────────────────────────┐
│                    Agent 引擎 (core)                     │
│                                                         │
│  ┌──────────┐   ┌──────────────┐   ┌────────────────┐  │
│  │ LLM 返回  │──▶│ ToolRegistry │──▶│ ToolSpec::     │  │
│  │ tool_call │   │ .execute()   │   │   execute()    │  │
│  └──────────┘   └──────────────┘   └───────┬────────┘  │
│                                             │           │
│  ┌──────────────────────────────────────────┘           │
│  │  ToolContext { workspace, sandbox, lsp, … }          │
│  │  ToolResult { content, success, metadata }           │
│  └──────────────────────────────────────────────────────┤
│                         │                               │
│  ┌──────────────────────▼─────────────────────────────┐ │
│  │  LargeOutputRouter (大结果走 V4-Flash 压缩)        │ │
│  │  LSP Diagnostics (编辑后自动注入编译诊断)          │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

### 1.1 核心 Trait: `ToolSpec`

定义在 `crates/tui/src/tools/spec.rs`（基于 `crates/tools/src/lib.rs` 的公共类型），每个工具实现以下方法：

| 方法 | 用途 |
|------|------|
| `name()` | 工具的唯一标识符（如 `"read_file"`） |
| `description()` | 发送给 LLM 的工具描述 |
| `input_schema()` | JSON Schema，定义工具接受的参数 |
| `capabilities()` | 能力标签：`ReadOnly`、`WritesFiles`、`ExecutesCode`、`Network`、`Sandboxable`、`RequiresApproval` |
| `approval_requirement()` | 审批级别：`Auto`（自动通过）、`Suggest`（建议审批）、`Required`（必须审批） |
| `supports_parallel()` | 是否可与其他工具并行执行 |
| `defer_loading()` | 是否延迟暴露给 LLM（MCP 工具默认延迟） |
| `execute(input, context)` | 核心执行逻辑 |

### 1.2 执行上下文: `ToolContext`

```rust
pub struct ToolContext {
    pub workspace: PathBuf,             // 工作区根目录
    pub shell_manager: SharedShellManager, // Shell 进程管理器
    pub trust_mode: bool,               // 是否信任外部路径
    pub auto_approve: bool,             // YOLO 模式
    pub sandbox_policy: SandboxPolicy,   // 沙箱策略
    pub features: Features,             // 功能开关
    pub network_policy: Option<NetworkPolicyDecider>, // 网络策略
    pub lsp_manager: Option<Arc<LspManager>>,  // LSP 管理器
    pub large_output_router: Option<LargeOutputRouter>, // 大输出路由
    pub cancel_token: Option<CancellationToken>, // 取消信号
    pub tool_progress: Option<Arc<dyn ToolProgressEmit>>, // 流式进度
    // ...
}
```

### 1.3 工具注册: `ToolRegistry` / `ToolRegistryBuilder`

```rust
// 典型的 Agent 模式注册
ToolRegistryBuilder::new()
    .with_full_agent_surface(client, model, manager, runtime, allow_shell, todo, plan)
    .build()
```

`with_full_agent_surface` 展开为所有工具家族：文件、搜索、Shell、Git、Web、Patch、任务、子代理、Todo、Plan、Review、RLM、RecallArchive 等。

---

## 2. 工具执行流程

```
① LLM 返回 tool_calls ──▶ ② Engine 分发到 ToolRegistry
                                  │
                    ┌─────────────┴─────────────┐
                    ▼                           ▼
           ③ 审批检查                    ④ 执行前校验
         (approval_requirement)         (路径安全、沙箱策略)
                    │                           │
                    └───────────┬───────────────┘
                                ▼
                    ⑤ ToolSpec::execute(input, context)
                                │
                    ┌───────────┴───────────┐
                    ▼                       ▼
            ⑥ ToolResult            ⑦ 后处理管道
           { content,                 ├─ LSP 诊断注入
             success }                ├─ 统一 diff 输出
                                      └─ 大输出路由 (#548)
```

---

## 3. 工具详解

### 3.1 文件操作工具

源文件: `crates/tui/src/tools/file.rs` (2249 行)

#### `read_file` — 读取文件

| 属性 | 值 |
|------|-----|
| 能力 | `ReadOnly`, `Sandboxable` |
| 审批 | `Auto` |
| 并行 | ✅ 是 |

**原理**:

1. **路径解析**: `context.resolve_path(path_str)` — 将相对路径规范化为绝对路径，限制在工作区范围内（除非 `trust_mode` 或路径在 `trusted_external_paths` 中）
2. **格式检测**: 根据文件扩展名分发：
   - `.pdf` → `pdftotext` 或 `pdf-extract` 外部命令提取文本
   - `.docx` / `.xlsx` / `.pptx` → OOXML ZIP 解压后提取纯文本
   - 常规文本 → 流式 newline 解码（低内存）
3. **换行处理**: 自动检测 UTF-16/UTF-32 BOM 并转换为 UTF-8
4. **分页读取**: `start_line`（1-based）+ `limit`（默认 2000，最大 5000）支持分页加载大文件
5. **文件大小限制**: 超过 100MB 拒绝，超过 10MB 不进行行数统计

#### `write_file` — 写入/创建文件

| 属性 | 值 |
|------|-----|
| 能力 | `WritesFiles`, `Sandboxable`, `RequiresApproval` |
| 审批 | `Suggest` |

**原理**:

1. 自动创建父目录 (`create_dir_all`)
2. 写入前快照已有内容，写入后生成 unified diff (`make_unified_diff`)
3. 调用 `lsp_diagnostics_for_paths()` 注入编辑后的编译诊断
4. 输出: `[diff] + Created/Wrote N bytes to path + [LSP diag]`

#### `edit_file` — 精确文本替换

| 属性 | 值 |
|------|-----|
| 能力 | `WritesFiles`, `Sandboxable`, `RequiresApproval` |
| 审批 | `Suggest` |

**V0 → V4 演进**（详见 `docs/edit-file-v*.md`）：

**支持的四种操作模式**:

| 模式 | 必需参数 | 说明 |
|------|----------|------|
| `search_replace` | `path, search, replace` | 搜索-替换。多重匹配时需指定 `replace_mode: "first"|"all"` |
| `insert_after` | `path, text, after_line` | 在指定行号后插入。`after_line=0`=文件开头 |
| `delete_lines` | `path, start_line, end_line` | 删除行范围。`dry_run: true` 预览 |
| `replace_line` | `path, line, text` | 替换指定行（绕过搜索匹配） |

**关键安全机制**:

1. **E1 换行规范化**: Windows CRLF 自动检测，搜索字符串自动适配
2. **E2 行范围限定**: `start_line`/`end_line` 限定搜索区域，避免误匹配
3. **E3 诊断错误**: 搜索未命中时返回详细提示（"文件使用 CRLF"、"多行搜索请确认缩进"、"用 `grep_files` 定位"）
4. **E4 歧义保护**: 多重匹配（>1）时拒绝执行，要求明确 `replace_mode`
5. **V1-4 紧凑输出**: ≤5 行的搜索/替换自动附加 compact before/after 对比
6. **JSX 平衡检查**: 对 `.tsx`/`.jsx` 文件检查花括号/括号平衡

**输出格式**:
```
[diff unified]
Replaced N occurrence(s) in path (line X, line Y) — file now Z lines
--- compact ---
  - old line
  + new line
<diagnostics file="path"> … </diagnostics>
```

#### `list_dir` — 列目录

| 能力 | `ReadOnly`, `Sandboxable` |
| 审批 | `Auto` |

返回 `{ entries: [{ name, kind: "file"|"dir", size? }] }`，按名排序，目录在前。

#### `file_info` — 文件元数据

| 能力 | `ReadOnly`, `Sandboxable` |

读取大小、mtime、文本/二进制嗅探、PDF 检测、行数（≤10MB）、编码猜测。

---

### 3.2 搜索工具

#### `grep_files` — 正则代码搜索

源文件: `crates/tui/src/tools/search.rs` (872 行)

| 能力 | `ReadOnly`, `Sandboxable` |
| 审批 | `Auto` |
| 并行 | ✅ |

**原理**:

1. 使用 Rust `regex` crate 编译用户提供的正则
2. 遍历工作区文件系统，跳过 >10MB 的文件（视为二进制）
3. 支持 `include`/`exclude` glob 过滤
4. 每个匹配返回 `{ file, line_number, line, context_before, context_after }`
5. 默认最多 100 条结果，可配置 `max_results`
6. 支持 `case_insensitive` 和 `context_lines`
7. **符号索引集成** (`symbol_index`: true)：同时查询预建的符号索引，返回定义位置（`symbol_index_hits`）。符号索引由 `syn` 解析器生成，包含 `fn`/`struct`/`enum`/`trait`/`impl` 等定义

#### `file_search` — 模糊文件名搜索

源文件: `crates/tui/src/tools/file_search.rs` (346 行)

| 能力 | `ReadOnly`, `Sandboxable` |
| 审批 | `Auto` |

**原理**:

1. 使用 `ignore` crate 的 `WalkBuilder` 遍历工作区（自动跳过 `.gitignore` 条目）
2. 对文件名/路径片段进行**模糊评分**（基于字符连续匹配度的计分算法）
3. 支持 `extensions` 过滤（如 `["rs", "md"]`）
4. 默认返回 20 条，最多 200 条
5. 返回 `{ path, name, score }`

---

### 3.3 Shell 执行工具

源文件: `crates/tui/src/tools/shell.rs` (2560 行)

#### `exec_shell` — 命令执行

| 能力 | `ExecutesCode`, `Sandboxable`, `RequiresApproval` |
| 审批 | `Required` |

**两种执行模式**:

| 模式 | 触发方式 | 行为 |
|------|----------|------|
| **前台** | `background: false`（默认） | 阻塞等待完成，超时后 kill。默认 120s，最大 600s |
| **后台** | `background: true` 或 `tty: true` | 立即返回 `task_id`，通过 `exec_shell_wait` 轮询 |

**关键参数**:

| 参数 | 说明 |
|------|------|
| `command` | 必需，Shell 命令 |
| `timeout_ms` | 超时（默认 120000，最大 600000） |
| `background` | 后台模式 |
| `interactive` | 交互模式（分配 PTY） |
| `stdin` | 预发送到 stdin 的数据 |
| `cwd` | 工作目录（默认：工作区根） |
| `tty` | PTY 分配（隐含后台） |

**安全层级**:

1. **ExecPolicy 引擎**: 可选的命令安全分析（`crates/execpolicy/`），加载用户策略文件进行预检
2. **沙箱**: macOS Seatbelt `/` Windows Job Object 隔离（`crates/tui/src/sandbox/`）
3. **超时强制**: `wait_timeout` crate 超时 kill
4. **输出截断**: `shell_output::truncate_with_meta` — stdout/stderr 各截断到安全长度，保留原始字节数

**返回结构** (`ShellResult`):

```json
{
  "task_id": "uuid",
  "status": "Completed",
  "exit_code": 0,
  "stdout": "...",
  "stderr": "...",
  "duration_ms": 1234,
  "stdout_len": 50000,
  "stdout_omitted": 0,
  "stdout_truncated": false,
  "sandboxed": false
}
```

#### Shell 管理家族

| 工具 | 用途 |
|------|------|
| `exec_shell_wait` / `exec_wait` | 轮询后台任务输出（阻塞直到完成或超时） |
| `exec_shell_interact` / `exec_interact` | 向运行中的后台任务发送 stdin 输入 |
| `exec_shell_cancel` | 终止指定的后台任务 |
| `task_shell_start` / `task_shell_wait` | 持久化 Shell 任务（关联到 durable task） |

**`ShellManager`** (`SharedShellManager`) 是这些工具的共享后端：

- 维护 `HashMap<String, BackgroundShell>` — 所有活跃 Shell 作业
- 支持 **检查点/恢复**（进程快照）
- **Stale job 记忆**：超时或被 kill 的作业结果保留一段时间供后续查询
- **沙箱策略**注入

---

### 3.4 Git 工具

源文件: `crates/tui/src/tools/git.rs`, `crates/tui/src/tools/git_history.rs`

| 工具 | 能力 | 说明 |
|------|------|------|
| `git_status` | `ReadOnly` | `git status --porcelain` 解析，返回 `{ staged, unstaged, untracked }` |
| `git_diff` | `ReadOnly` | `git diff`（支持 `--staged`、文件过滤、上下文行数） |
| `git_log` | `ReadOnly` | `git log --oneline`，支持 `max_count` |
| `git_show` | `ReadOnly` | `git show <commit>` |
| `git_blame` | `ReadOnly` | `git blame <file>`，支持行范围 |

全部为 `ReadOnly` + `Auto` 审批。

---

### 3.5 Patch 工具

源文件: `crates/tui/src/tools/apply_patch.rs` (1489 行)

#### `apply_patch` — Unified Diff 应用

| 能力 | `WritesFiles`, `Sandboxable`, `RequiresApproval` |
| 审批 | `Suggest` |

**原理**:

1. **解析 unified diff**: 支持多文件、多 hunk
2. **模糊匹配**: 当精确行号不匹配时，在 ±50 行范围内搜索上下文（`MAX_FUZZ`）
3. **新建/删除文件**: 识别 `--- /dev/null` 和 `+++ /dev/null`
4. **逐 hunk 报告**: `{ hunks_applied, hunks_total, fuzz_used, hunks_with_fuzz }`
5. **原子性**: 所有 hunk 成功才写入，否则回滚
6. 输出包含 `touched_files` 列表，触发 LSP 诊断

**输出**:
```json
{
  "success": true,
  "files_applied": 2,
  "files_total": 3,
  "hunks_applied": 5,
  "hunks_total": 5,
  "fuzz_used": 1,
  "touched_files": ["src/main.rs", "src/lib.rs"],
  "message": "..."
}
```

---

### 3.6 Web 工具

源文件: `crates/tui/src/tools/web_search.rs` (706 行), `crates/tui/src/tools/fetch_url.rs` (509 行), `crates/tui/src/tools/web_run.rs`

#### `web_search` — 网页搜索

| 能力 | `ReadOnly`, `Network`, `Sandboxable` |
| 审批 | `Auto` |

**原理**:

1. **优先 DuckDuckGo** (`html.duckduckgo.com`)：发送 HTML 请求，用正则解析 `<a class="result__a">` + `<div class="result__snippet">`
2. **Bing 回退**：当 DDG 不可达时（网络/TLS/DNS/超时/非 200），自动尝试 `www.bing.com`
3. **结果爬取**: 每页最多 5 条（`DEFAULT_MAX_RESULTS`），可调至 10 条（`MAX_RESULTS`）
4. **HTML 清洗**: 正则去除标签、脚本、样式，保留纯文本
5. **网络策略**: 受 `NetworkPolicyDecider` 控制（`Deny` / `Prompt` / `Allow`）
6. **用户代理**: 模拟 Safari 浏览器

**输出**: `{ query, source, count, results: [{ title, url, snippet }] }`

#### `fetch_url` — 直接 HTTP 获取

| 能力 | `ReadOnly`, `Network`, `Sandboxable` |
| 审批 | `Auto` |

**原理**:

1. HTTP GET，支持重定向（最多 5 次）
2. **格式选择**: `markdown`（默认，HTML→文本）、`text`（去标签纯文本）、`raw`（原始字节）
3. **大小限制**: 默认 1MB，硬上限 10MB，超限则返回 `truncated: true`
4. 超时默认 15s，最大 60s
5. HTML 清洗: 正则去除 `<script>`、`<style>`、HTML 标签，压缩空白

#### `web_run` — 浏览器自动化

调用 Playwright/headless 浏览器执行页面操作（打开、点击、截图）。

#### `finance` — 市场行情

获取股票/加密货币实时报价。

---

### 3.7 子代理 (Sub-Agent) 工具

源文件: `crates/tui/src/tools/subagent/mod.rs` (4348 行)

子代理系统允许模型**递归派遣子任务**到独立的 Agent 实例。每个子代理运行在受限的工具集和上下文窗口中。

| 工具 | 功能 |
|------|------|
| `agent_spawn` / `spawn_agent` / `delegate_to_agent` | 创建并启动子代理（指定 `type` 和 `prompt`） |
| `agent_result` | 获取已完成的子代理结果 |
| `agent_wait` / `wait` | 阻塞等待子代理完成 |
| `agent_list` | 列出所有子代理状态 |
| `agent_cancel` / `close_agent` | 取消运行中的子代理 |
| `agent_send_input` / `send_input` | 向运行中的子代理发送输入 |
| `agent_assign` / `assign_agent` | 将子代理绑定到特定任务 ID |
| `agent_resume` / `resume_agent` | 恢复暂停的子代理 |

**子代理类型** (`SubAgentType`):

| 类型 | 工具集 | 用途 |
|------|--------|------|
| `general` | 完整 Agent 工具面 | 通用多步任务（默认） |
| `explore` | 只读文件 + 搜索 | 快速代码库探索 |
| `plan` | 分析工具 | 架构规划 |
| `review` | 只读文件 + 搜索 + Git | 代码审查、评分 |
| `implementer` | 文件 + Shell + Patch + Git | 聚焦代码实现 |
| `verifier` | Shell（`cargo test` 等） | 运行测试、报告 pass/fail |
| `auditor` | 只读文件 + 搜索 | 机械事实核对（验证 file_path + line_number + symbol） |
| `custom` | 由 spawn 时指定 | 自定义工具集 |

**关键机制**:

- **并发上限**: `config.toml` 中 `[subagents].max_concurrent`（默认 10，硬上限 20）
- **递归深度**: `DEFAULT_MAX_SPAWN_DEPTH` 限制嵌套层级
- **文件租约** (`RESIDENT_LEASES`): 每个文件同一时间只能有一个 resident 子代理持有写租约
- **取消传播**: 父级 `CancellationToken` 传递给所有子代理
- **结构化裁决**: Reviewer 子代理返回 `structured_verdict { PASS, BLOCKER, MAJOR, FAIL }`，触发自动 fix-loop

---

### 3.8 任务 (Task) 工具

源文件: `crates/tui/src/tools/tasks.rs` + `automation.rs` + `github.rs`

| 工具 | 功能 |
|------|------|
| `task_create` | 创建持久化后台任务（可跨会话恢复） |
| `task_list` | 列出近期任务 |
| `task_read` | 读取任务详情（含 timeline、checklist、gate 证据、artifact） |
| `task_cancel` | 取消任务 |
| `task_gate_run` | 运行验证门禁（`fmt` / `check` / `clippy` / `test` / `custom`） |
| `task_shell_start` / `task_shell_wait` | Shell 任务关联到 durable task |
| `github_issue_context` / `github_pr_context` | 只读拉取 GitHub issue/PR 内容 |
| `github_comment` / `github_close_issue` | 写操作（需审批 + 证据） |
| `pr_attempt_record` / `pr_attempt_list` / `pr_attempt_read` / `pr_attempt_preflight` | PR 尝试管理 |
| 自动化系列 | `automation_*` — 创建/列出/运行/暂停/恢复自动化规则 |

---

### 3.9 计划与 Todo 工具

| 工具 | 源文件 | 功能 |
|------|--------|------|
| `update_plan` | `plan.rs` | 更新高层策略计划（3-6 阶段，每阶段有状态） |
| `checklist_write` / `checklist_add` / `checklist_update` / `checklist_list` | `todo.rs` | 细粒度任务清单管理 |
| `todo_write` / `todo_add` / `todo_update` / `todo_list` | `todo.rs` | 别名兼容 |

**设计原则**: `update_plan` 用于战略层面（高层阶段），`checklist_write` 用于战术层面（可验证的叶子任务）。引擎自动同步到 UI 侧边栏。

---

### 3.10 分析与审查工具

| 工具 | 功能 |
|------|------|
| `review` | 调用独立 LLM 对代码变更进行审查 |
| `rlm` (Recursive Language Model) | 在 Python REPL 沙箱中对超长输入进行递归分块处理。内部提供 `llm_query` / `llm_query_batched` / `rlm_query` 辅助函数 |
| `diagnostics` | 收集工作区诊断信息：Git 状态、Rust 工具链版本、沙箱可用性 |
| `project_map` | 生成项目结构映射 |

#### `rlm` 工作原理

1. 加载输入到 Python REPL 沙箱作为 `PROMPT` 变量
2. 子 LLM 编写 Python 脚本：将输入分块 → 调用 REPL 内 `llm_query()` 逐块处理 → 汇总
3. 支持 `llm_query_batched` 并行处理多个分块
4. 输入的原始文本**不进入**调用方的上下文窗口，节省 token

---

### 3.11 其他工具

| 工具 | 功能 |
|------|------|
| `write_office` | 生成 `.xlsx` / `.docx` / `.pptx`（XLSX 纯 Rust，DOCX/PPTX 用 Python） |
| `load_skill` | 从 `SKILL.md` 加载技能定义到上下文 |
| `describe_image` | 通过视觉模型提取图片文字描述（调用 SiliconFlow Qwen3-VL） |
| `validate_data` | 验证 JSON/TOML 数据格式 |
| `note` | 追加持久化笔记到 `.deepseek/notes.md` |
| `remember` | 写入用户记忆文件（#489） |
| `recall_archive` | 搜索 checkpoint-restart 归档（#127） |
| `revert_turn` | 撤销上一轮编辑（通过 workspace 快照 side-repo） |
| `run_tests` | 运行 cargo test |
| `request_user_input` | 向用户提问 1-3 个选择题 |
| `fim_edit` | Fill-in-the-Middle 编辑（需 DeepSeekClient） |

---

## 4. 安全机制

### 4.1 路径安全: `context.resolve_path()`

```rust
pub fn resolve_path(&self, path_str: &str) -> Result<PathBuf, ToolError>;
```

1. **规范化**: `Path::new(path).canonicalize()` 消除 `..` 和符号链接
2. **工作区限制**: 检查规范化路径是否以 `workspace` 为前缀
3. **信任豁免**: `trust_mode` 或 `trusted_external_paths` 中的路径可以突破工作区限制
4. **拒绝逃逸**: 路径逃逸一律返回 `ToolError::PathEscape`

### 4.2 审批系统

| 级别 | 触发条件 | 行为 |
|------|----------|------|
| `Auto` | `ReadOnly` 工具 | 直接执行，无需用户干预 |
| `Suggest` | `WritesFiles` 工具 | 显示确认提示，用户可跳过 |
| `Required` | `ExecutesCode` 工具 | 必须用户手动批准 |

**审批流程**: Engine 在执行前暂停 → 通过 SSE 发送 `event=approval` → 用户通过 `POST /v1/threads/{id}/turns/{turn_id}/resolve-approval` 响应 → Engine 继续或拒绝。

**YOLO 模式**: `ToolContext::auto_approve = true` 时跳过所有审批，但仍受 ExecPolicy 和沙箱约束。

### 4.3 ExecPolicy 引擎

`crates/execpolicy/` 提供命令安全分析：

- 用户可定义策略文件，声明允许/禁止的命令模式
- `exec_shell` 在沙箱前先经过策略引擎评估
- 可配置为阻止高危命令（`rm -rf /`、`curl | sh` 等）

### 4.4 沙箱

`crates/tui/src/sandbox/` 支持：

- **macOS**: Seatbelt (`sandbox-exec`) — 限制文件系统访问、网络、进程派生
- **Windows**: Job Object — 限制进程资源
- **Linux**: 计划中的 seccomp/landlock 支持

### 4.5 网络策略

`NetworkPolicyDecider` 按域名控制 `web_search` / `fetch_url` 的访问：

- `Allow`: 放行
- `Deny`: 拒绝
- `Prompt`: 需用户批准

---

## 5. 后处理管道

### 5.1 LSP 诊断自动注入

`lsp_diagnostics_for_paths()` 在每次 `write_file` / `edit_file` / `apply_patch` 之后运行：

```
编辑完成 → 异步启动 LSP server → 对已编辑文件运行诊断
→ 将诊断结果以 <diagnostics> 块形式附加到 ToolResult.content
```

**格式**:
```
<diagnostics file="crates/tui/src/foo.rs">
  ERROR [12:8] missing semicolon
  WARNING [45:1] unused variable `x`
</diagnostics>
```

Agent 在下一个 turn 收到诊断作为合成用户消息，可立即修复编译错误，无需手动运行 `cargo check`。

当前支持的 LSP:
- **Rust**: rust-analyzer
- **TypeScript/TSX**: typescript-language-server
- **Python**: pyright
- **Go**: gopls
- **C/C++**: clangd

### 5.2 大输出路由 (#548)

`LargeOutputRouter` 拦截超过 4096 token 的 ToolResult：

1. **阈值估算**: 按 3 字符/token 的保守比率估算
2. **V4-Flash 合成**: 启动轻量子代理将原始输出压缩为要点摘要
3. **原始存储**: 完整输出存入 `workshop_vars["last_tool_result"]`
4. **升级**: Agent 可通过 `promote_to_context` 将完整输出提升到上下文

**旁路**: 工具调用时传 `raw=true` 可跳过路由，直接返回完整输出。

### 5.3 Unified Diff 输出

`make_unified_diff()` 使用 `similar` crate 的 `TextDiff` 生成 git 风格 diff。Agent 看到行级别的 `+`/`-` 变更而非单行摘要。

---

## 6. 工具清单 (共 ~80 个)

### 文件操作 (5)
`read_file` `write_file` `edit_file` `list_dir` `file_info`

### 搜索 (2)
`grep_files` `file_search`

### Shell (8)
`exec_shell` `exec_shell_wait` `exec_shell_interact` `exec_wait` `exec_interact`
`task_shell_start` `task_shell_wait` `exec_shell_cancel`

### Git (5)
`git_status` `git_diff` `git_log` `git_show` `git_blame`

### Patch (1)
`apply_patch`

### Web (4)
`web_search` `fetch_url` `web_run` `finance`

### 子代理 (8)
`agent_spawn` `agent_result` `agent_wait` `agent_list` `agent_cancel`
`agent_send_input` `agent_assign` `agent_resume`

### 任务 & 自动化 (15+)
`task_create` `task_list` `task_read` `task_cancel` `task_gate_run`
`github_issue_context` `github_pr_context` `github_comment` `github_close_issue`
`pr_attempt_record` `pr_attempt_list` `pr_attempt_read` `pr_attempt_preflight`
`automation_*`（7 个：list/create/read/update/delete/run/pause/resume）

### 计划 & Todo (8)
`update_plan` `checklist_write` `checklist_add` `checklist_update` `checklist_list`
`todo_write` `todo_add` `todo_update` `todo_list`

### 分析 & 审查 (4)
`review` `rlm` `diagnostics` `project_map`

### 其他 (10+)
`write_office` `load_skill` `describe_image` `validate_data` `note`
`remember` `recall_archive` `revert_turn` `run_tests` `request_user_input`
`fim_edit`

### 延迟加载 (MCP 工具)
MCP 工具名由 `mcp.json` 中的连接服务器动态生成，注册时标记 `defer_loading=true`。

---

## 7. 源文件索引

| 组件 | 文件 | 行数 |
|------|------|------|
| 工具抽象层 | `crates/tools/src/lib.rs` | 469 |
| ToolSpec trait + ToolContext | `crates/tui/src/tools/spec.rs` | 791 |
| 工具注册 | `crates/tui/src/tools/registry.rs` | 1337 |
| 文件工具 | `crates/tui/src/tools/file.rs` | 2249 |
| 搜索工具 | `crates/tui/src/tools/search.rs` | 872 |
| Shell 工具 | `crates/tui/src/tools/shell.rs` | 2560 |
| Patch 工具 | `crates/tui/src/tools/apply_patch.rs` | 1489 |
| Web 搜索 | `crates/tui/src/tools/web_search.rs` | 706 |
| URL 获取 | `crates/tui/src/tools/fetch_url.rs` | 509 |
| 子代理 | `crates/tui/src/tools/subagent/mod.rs` | 4348 |
| Diff 生成 | `crates/tui/src/tools/diff_format.rs` | 77 |
| 大输出路由 | `crates/tui/src/tools/large_output_router.rs` | 320 |
| 诊断工具 | `crates/tui/src/tools/diagnostics.rs` | 251 |
| 文件名搜索 | `crates/tui/src/tools/file_search.rs` | 346 |
