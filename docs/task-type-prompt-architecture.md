# TaskType 任务类型分层方案（办公 / 代码 二类 MVP）

**状态**: **MVP 已落地**（`crates/tui/src/task_type.rs`、prompt overlay、Office 工具裁剪、DS Pick Composer 切换）  
**日期**: 2026-05-17（初稿，四类）· **2026-05-18**（收敛为二类，对齐当前工具与 prompt）  
**基于**: [prompt-architecture.md](prompt-architecture.md)  
**相关**: [prompt-hallucination-patch.md](prompt-hallucination-patch.md)、[TOOLS_PRINCIPLES.md](tech/TOOLS_PRINCIPLES.md)、`crates/tui/src/prompts/base.md`（检索三件套）

---

## 1. 设计结论（已定稿）

| 决策 | 内容 |
|------|------|
| **类型数量** | **两类**：`Office`（办公）、`Code`（代码）。不再单独设 Chat / Architecture 枚举。 |
| **Office 范围** | 一般聊天 + 文档/表格/演示/PDF 等办公任务。 |
| **Code 范围** | 编写/修改/调试 + 架构分析/设计评审/只读调研（见 §4 只读工作流）。 |
| **与 AppMode 关系** | 正交叠加：`AppMode`（Agent / Plan / Yolo）管「怎么工作」；`TaskType` 管「干什么」。 |
| **切换类型** | **必须新开 session**。不在同一会话内改 TaskType，保证 KV Cache 前缀稳定。 |
| **维护** | 仅 2 个 task overlay 文件；`code.md` 宜极短或为空（Code = 现有全量行为）。 |

---

## 2. 背景与动机

当前几乎所有会话都走 **Code 全量** system prompt：CRAFT、LSP、符号索引、检索三件套、幻觉防控、完整 toolbox。对以下场景浪费 token 且干扰判断：

| 场景 | 问题 |
|------|------|
| 办公 / 聊天 | 注入代码审查、LSP、`edit_file` 并行策略等，模型易「过度工程化」 |
| 办公 | 不需要 `grep_files` / `exec_shell`，却出现在工具列表与说明里 |
| 代码会话内的架构评审 | 若只靠 pick-rules 软约束，仍可能误调 `edit_file` |

初稿曾拆四类（Chat / Office / Code / Architecture）。**产品收敛为二类**后：聊天并入 Office（prompt 区分「聊 vs 做文档」）；架构评审并入 Code（**只读靠 Explore 子代理 + 工作流**，见 §4）。

---

## 3. TaskType 定义

```rust
/// 任务类型——控制 prompt overlay 与工具注册裁剪。
/// Session 创建时固定；切换类型 = 新 session。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    /// 聊天 + 办公文档。精简 prompt + 办公工具面子集。
    Office,
    /// 编程与仓库内技术工作（含架构评审）。全量 prompt + 全 Agent 工具面。
    #[default]
    Code,
}

impl TaskType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskType::Office => "office",
            TaskType::Code => "code",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            TaskType::Office => "办公",
            TaskType::Code => "代码",
        }
    }

    /// 是否注册代码向写工具（edit_file / apply_patch / exec_shell 等）
    pub fn uses_code_tool_surface(&self) -> bool {
        matches!(self, TaskType::Code)
    }

    /// 是否注入完整 runtime 说明（CRAFT、LSP、符号索引、检索三件套等）
    pub fn needs_full_code_prompt(&self) -> bool {
        matches!(self, TaskType::Code)
    }
}
```

---

## 4. 两类差异总览

### 4.1 Token 预算（粗估，需实现后实测）

| TaskType | 现在（全量） | 目标 | 说明 |
|----------|-------------|------|------|
| Office | ~8500+ token | ~2000–3500 | 去掉 CRAFT/LSP/符号索引/代码 toolbox；保留人格、语言、办公说明 |
| Code | ~8500+ token | ~8500+ | **不裁剪**（与当前 Agent 一致） |

> `base.md` 已含检索三件套、Capability Claims 等，全量可能高于旧表中的 8500，以 `chars/3` 或 tokenizer 实测为准。

### 4.2 工具权限矩阵（与当前实现对齐）

| 工具 | Office | Code |
|------|:------:|:----:|
| `read_file` | ✅ | ✅ |
| `list_dir` | ✅ | ✅ |
| `file_search` | ✅（找附件/模板） | ✅ |
| `glob_files` | ✅（按路径找文件） | ✅ |
| `write_office` | ✅ | ✅ |
| `write_file` | ✅（非源码交付物时） | ✅ |
| `grep_files` | ❌ | ✅（含 `output_mode`: content / files_with_matches / count） |
| `edit_file` / `apply_patch` | ❌ | ✅ |
| `exec_shell` | ❌ | ✅ |
| `agent_spawn` | ❌（默认） | ✅ |
| `run_tests` / `diagnostics` / `git_*` | ❌ | ✅ |
| `web_search` / `fetch_url` / `web.run` | 可选 ✅ | ✅ |

**硬边界**：Office 必须在 **`ToolRegistryBuilder` 层裁剪**，不能只靠 prompt 禁止。否则模型仍可调用 `edit_file`。

**禁止 Bash 搜索**：两类均通过 `grep_files` / `glob_files`；**禁止** `exec_shell` 跑 `grep`/`rg`（见 `base.md` 检索三件套）。

### 4.3 Prompt 注入裁剪

| 块 | Office | Code |
|----|:------:|:----:|
| `compose_base_prompt_layer()`（base.md 核心） | ✅ 精简子集或完整 base 中非代码段 | ✅ 全量 |
| personality + mode + approval | ✅ | ✅ |
| `tasks/office.md` overlay | ✅ | ❌ |
| `tasks/code.md` overlay | ❌ | ✅（宜空：Code = 默认） |
| Environment / project context | ✅ 可选（办公常需路径） | ✅ |
| instructions（pick-rules 等） | ❌ 或仅办公相关摘录 | ✅ |
| Skills 目录 | ❌ | ✅ |
| Context Management（Agent/Yolo） | 缩短版 | ✅ |
| CRAFT / LSP / 符号索引 / 检索三件套 | ❌ | ✅ |
| Epistemic / Capability Claims | 简短版 | ✅ 全量 |

### 4.4 Office 内：聊天 vs 做文档（同一 TaskType）

不拆第三个枚举，在 **`prompts/tasks/office.md`** 用行为规则区分：

- **默认**：纯对话，**不调用工具**；简洁回答。
- **用户要读文件 / 生成 xlsx·docx·pptx·pdf**：再用 `read_file` / `write_office` / `list_dir`。
- **用户要改代码、跑命令、搜仓库**：提示切换到 **代码** 任务并 **新开 session**。

### 4.5 Code 内：架构评审 / 只读调研（原 Architecture 合并于此）

Code 会话**仍注册写工具**，评审类任务靠工作流降低误改风险：

1. **优先** `agent_spawn` + `type: Explore`（或 Review）：子代理只读工具面，结论回传主会话（上下文隔离，见 `subagent/mod.rs` `build_allowed_tools`）。
2. **主会话**若自行检索：`glob_files` / `grep_files`（`output_mode: files_with_matches`）→ `read_file` 分段，遵循 `base.md` **检索三件套**。
3. **未经明确授权不得** `edit_file` / `apply_patch` / `exec_shell` 改仓库；若用户要求改代码，视为实现任务而非评审。
4. 产出：Mermaid、ADR 式结论、带路径/行号的发现；修改建议以 **diff 描述** 输出，不直接落盘。

> 若未来误改率仍高，可再加 session 级 `read_only: bool`（非 MVP）。

---

## 5. 新增文件

```
crates/tui/src/
├── task_type.rs              # 枚举 + infer_task_type()
└── prompts/tasks/
    ├── office.md             # 办公 + 聊天规则（见 §5.1）
    └── code.md               # 空或极短补充（Code = 现有全量）

# 不再创建：chat.md、architecture.md（已并入二类）
```

### 5.1 `prompts/tasks/office.md` 骨架

```markdown
## Task: Office（办公）

本 session 用于一般对话与办公文档，不是编程任务。

### 聊天（默认）
- 不要调用工具，直接回答。
- 保持简洁、对话式。

### 文档与文件
- 生成 XLSX/DOCX/PPTX/PDF：使用 `write_office`。
- 读取附件或确认路径：`read_file`、`list_dir`；按名找文件：`glob_files` 或 `file_search`。
- 生成前确认路径与格式。

### 禁止
- 不要调用 `grep_files`、`edit_file`、`apply_patch`、`exec_shell`、`agent_spawn`（除非产品后续明确开放）。
- 不要用 Bash 运行 grep/rg。
- 若用户要改代码、调试、架构深挖：请切换到 **代码** 任务并 **新建会话**。
```

### 5.2 `prompts/tasks/code.md` 骨架

```markdown
## Task: Code（代码）

本 session 使用完整 Agent 工具与代码规范（与默认行为一致）。
架构/评审类任务优先 `agent_spawn` Explore；详见 base.md「代码检索三件套」。
```

---

## 6. 代码改动点（实现清单）

命名与现有源码一致：`AppMode`、`ApprovalMode`（非初稿中的 `AgentMode` / `ApprovalPolicy`）。

### T1 — `task_type.rs`

- `TaskType` 枚举（§3）
- `infer_task_type(workspace, first_message)` 规则：
  1. 首条消息含 **修复/实现/edit/bugfix** 等 → **Code**（覆盖「分析」类关键词）
  2. 工作区存在常见代码扩展名（顶层 + `src/` 浅扫）→ **Code**
  3. 首条消息含办公关键词（文档/xlsx/pptx/…）且无强 Code 信号 → **Office**
  4. 兜底 → **Office**（聊天向）或 **Code**（有仓库时倾向 Code）——**产品默认建议 `Code`**

### T2 — `prompts.rs` 嵌入 overlay

```rust
const TASK_OFFICE: &str = include_str!("prompts/tasks/office.md");
const TASK_CODE: &str = include_str!("prompts/tasks/code.md");

fn task_overlay(task_type: TaskType) -> &'static str {
    match task_type {
        TaskType::Office => TASK_OFFICE,
        TaskType::Code => TASK_CODE,
    }
}
```

在 `compose_prompt_with_approval(..., task_type, ...)` 中，于 mode overlay **之后**追加 `task_overlay`。

`system_prompt_for_mode_with_context_skills_session_and_approval(...)` 增加 `task_type: TaskType`，按 §4.3 跳过 Office 不需要的 runtime 块。

### T3 — Engine + Session

- `Engine`（或 session 元数据）持有 `task_type: TaskType`
- **创建 session** 时写入；`set_task_type` **仅用于新 session** 流程，不用于中途刷新
- UI/CLI 切换类型 → 提示「将新建会话」→ `new_session(task_type)`

### T4 — `ToolRegistryBuilder` 按 TaskType 分支

- `TaskType::Code` → 现有 `with_full_agent_surface`（不变）
- `TaskType::Office` → 新函数，例如 `with_office_surface()`：`read_file`、`list_dir`、`file_search`、`glob_files`、`write_office`、`write_file`（可选）、`note`；**无** grep/edit/shell/subagent

### T5 — Desktop / TUI UI

在 AppMode 旁增加任务类型：

| 选项 | 值 |
|------|-----|
| 自动 | 后端 `infer_task_type` |
| 办公 | `office` |
| 代码 | `code` |

切换时：**新建 session**，不就地改 prompt。

### T6 — 配置持久化（可选）

```toml
# ~/.deepseek/config.toml 或 <workspace>/.deepseek/config.toml
[task]
default_type = "code"   # office | code | auto
```

---

## 7. 实施顺序（MVP）

| 批次 | 内容 | 收益 |
|:----:|------|------|
| **M1** | T1 + 单测 `infer_task_type` | 类型与规则可测 |
| **M2** | T2 + `office.md` / `code.md` + Office prompt 裁剪 | Token 与说明分离 |
| **M3** | T4 + T3（Engine/session 固定 task_type） | 工具硬边界 |
| **M4** | T5 + T6 | 用户可见、可配置 |

每批可独立 PR；M3 前 Office 仍可能通过工具列表误调写工具，宜 M2+M3 同发或紧接。

---

## 8. KV Cache

- **有利**：Office session 前缀显著变短，同类型连续对话 cache 命中更好。
- **约束**：**禁止** session 中途切换 TaskType；切换 = 新 session（已接受）。
- **与 AppMode**：同一 session 内 `AppMode` 也宜少变；TaskType 与 mode 均在 session 创建时确定。

---

## 9. 与初稿（四类）对照

| 初稿类型 | 二类方案中的归属 |
|----------|------------------|
| Chat | `Office`（prompt：默认不调工具） |
| Office | `Office`（工具 + 文档规则） |
| Code | `Code`（全量） |
| Architecture | `Code`（§4.5 Explore + 检索三件套；无独立枚举） |

---

## 10. 注意事项

1. **Office 聊天**可不注入完整 pick-rules / AGENTS.md；若用户问项目细节，可引导切换到 **代码** 新会话。  
2. **架构评审**在 Code 下仍可能误写：MVP 依赖 Explore + prompt；监控误调 `edit_file` 再考虑 session `read_only`。  
3. **自动推断**：关键词必有歧义；**含修复/实现意图 → Code**；UI 保留手动覆盖。  
4. **Plan / Yolo**：TaskType 与 `AppMode` 叠加；Plan 会话仍以 `update_plan` 为主，Office 下是否允许 plan 工具需单独定一条规则（建议 Office 不注入 plan 长说明）。  
5. **文档与代码同步**：实现后更新 [prompt-architecture.md](prompt-architecture.md) 增加 TaskType 层；[TOOLS_PRINCIPLES.md](tech/TOOLS_PRINCIPLES.md) §1.4 与工具表按 Office/Code 分列。

---

## 11. 验收标准（实现后）

- [ ] 新建 Office session：system prompt 不含 CRAFT/LSP/符号索引/代码检索三件套长文  
- [ ] Office session：registry 无 `edit_file`、`grep_files`、`exec_shell`  
- [ ] 新建 Code session：与当前 Agent 行为一致（全工具 + 全 prompt）  
- [ ] UI 切换 Office ↔ Code：**创建新 session**，旧 session 不变  
- [ ] `infer_task_type("分析并修复 login")` → `Code`  
- [ ] `grep_files` / `glob_files` 仅在 Code（及 Explore 子代理）可用  

---

## 12. UI 安排（定稿建议）

### 12.1 控件放哪

**推荐**：与现有 **运行模式**（Plan / Agent / Yolo）并列，放在 **Composer 底栏** `Composer.tsx` 工具条内——紧挨 `runMode` 下拉左侧或右侧，使用同级 `composer-chip` 样式。

```text
[ 自动批准 ] | [ Agent ▼ ] | [ 办公 ▼ ]  ……  模型 / 更多
                  ↑ runMode      ↑ taskType（办公 | 代码 | 自动）
```

**不推荐**单独占顶栏或设置页深处：任务类型是 **session 级** 决策，应在「发消息前」可见。

**Session 列表**（侧栏）每条会话建议带小标签：`办公` / `代码`，避免打开错会话。

### 12.2 切换交互（与「新 session」一致）

| 操作 | 行为 |
|------|------|
| 新建会话 | 使用当前选中的 TaskType（或「自动」→ `infer_task_type`） |
| 在输入区改 TaskType | **弹窗**：「切换任务类型将新建会话，当前对话不会带入。」→ 确认后 `onNewSession(taskType)` |
| 会话进行中 | TaskType **只读**（chip 可点但仅展示说明，或 disabled） |

不在同一会话内改类型，避免 KV cache 与工具面不一致。

### 12.3 Office 下界面是否变化

**建议：适度收敛，不做整套换肤。**

原则：**隐藏「代码向、且 Office registry 没有」的入口**；保留文件预览、办公交付物、通用设置。

| 区域 | Code | Office |
|------|:----:|:------:|
| 主聊天区 | ✅ | ✅ |
| Composer：TaskType | ✅ | ✅ |
| Composer：Run mode（Plan/Agent/Yolo） | 三档 | **默认 Agent**；Plan/Yolo 可隐藏或灰显「代码会话专用」 |
| Composer：自动批准 | Agent 常用 | **可默认开**（`write_office` 多为 Suggest，减少打断） |
| Composer：路由 / 模型 | ✅ | ✅（可保留） |
| 右侧：**工作台 → 文件** | ✅ | ✅；**默认打开 `deliverables/`**（§13） |
| 右侧：**Checklist** | ✅ | 隐藏或仅在有数据时出现 |
| 右侧：**子代理 (agents)** | ✅ | **隐藏** |
| 右侧：**索引 (index)** | ✅ | **隐藏** |
| 右侧：**Mermaid** | ✅ | **保留**（办公也常画流程/结构图） |
| 右侧：任务与技能 / MCP / 系统 | 设置类 | 保留在「更多」或设置，不占主路径 |

**不必**删掉代码面板代码路径——用 `taskType === 'office'` 条件渲染即可；Code 会话恢复完整侧栏。

**可选增强（非 MVP）**：Office 会话顶栏浅色条文案「办公模式 · 产出在 deliverables/」；Code 无条或显示「代码模式」。

### 12.4 与 AppMode 的组合（Office session）

| 组合 | 建议 |
|------|------|
| Office + Agent | **默认推荐** |
| Office + Plan | 允许但弱化（办公很少先写 plan）；可不展示 Plan |
| Office + Yolo | 不建议；隐藏或拒绝创建 |

---

## 13. Office 模式：生成文件放哪

### 13.1 原则

- 路径相对 **Composer 工作区根**（与 `write_office` / `read_file` 的 `context.resolve_path` 一致）。
- **不要**默认写到 `.deepseek/`：仓库 `.gitignore` 已忽略 `.deepseek/`，用户不易在资源管理器/git 里看到交付物。
- 用户明确指定路径时 **以用户为准**。

### 13.2 默认目录（推荐定稿）

```text
<workspace>/
  deliverables/          ← Office 默认产出根（可配置）
    <可选子目录>/        ← 按会话或日期，见下
      报告_2026-05-18.xlsx
      方案初稿.pptx
```

| 项 | 建议值 |
|----|--------|
| 默认根目录 | `deliverables/` |
| 子目录策略（MVP） | 不强制；prompt 写「默认 `deliverables/<简短英文名>.<ext>`」 |
| 子目录策略（增强） | `deliverables/<YYYY-MM-DD>/` 或 `deliverables/<session-id 前 8 位>/` 防覆盖 |

**config.toml（可选）**：

```toml
[task]
default_type = "code"
office_output_dir = "deliverables"   # 相对 workspace
```

### 13.3 三处一致

1. **`prompts/tasks/office.md`**：`write_office` 未给 `path` 时 → `deliverables/...`  
2. **`write_office` 工具**（可选）：`path` 缺省时拼 `office_output_dir` + 由 title  slug 的文件名（实现阶段再定）  
3. **DS Pick 右栏「文件」**：Office session 打开工作台时 **默认列出 `deliverables/`**（无则提示「首次生成后会出现在此」）

### 13.4 非代码工作区

用户把 Composer 工作区指到 `~/Documents/我的项目` 时，`deliverables/` 仍在该目录下，符合直觉。

若在 **代码仓库** 内办公：建议在项目 `.gitignore` **不要**忽略 `deliverables/`（或提供模板 `.gitignore` 注释说明），是否提交由用户决定；与忽略 `.deepseek/` 区分开。

### 13.5 待你确认的两点

1. 默认目录名用 **`deliverables`** 还是 **`documents` / `output`**？（推荐 `deliverables`，与 base.md 交付物 recap 用语一致。）  
2. Office 是否要 **按日期分子文件夹**（避免同名覆盖）？MVP 可只做扁平 `deliverables/文件名.ext`。  
