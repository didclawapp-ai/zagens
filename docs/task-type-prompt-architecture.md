# TaskType 任务类型分层方案

**日期**: 2026-05-17  
**基于**: `prompt-architecture.md`（现有架构文档）  
**目标**: 区分聊天/办公/代码/架构四种任务类型，控制token成本，提升prompt准确度  
**原则**: 不动 compile-time 层，不破坏 KV Cache 分层，向后兼容  

---

## 背景与动机

现有架构的 mode 层（agent/plan/yolo）解决的是"模型怎么工作"，
TaskType 解决的是"模型做什么类型的任务"——两个维度正交，可以叠加。

现有问题：

| 问题 | 影响 |
|------|------|
| 聊天任务注入了完整工具权限说明、CRAFT流程、符号索引规范 | ~6500 token 纯浪费 |
| 办公任务注入了代码审查规则、LSP诊断说明 | 无关内容干扰模型判断 |
| 架构分析任务没有明确禁止 edit_file | 模型可能误改代码 |
| 代码任务没有按工作区语言裁剪工具说明 | 模型在 Rust 项目看到 Python 工具说明 |

---

## TaskType 定义

```rust
/// 任务类型——控制 prompt overlay 和工具权限裁剪。
/// 由用户手动选择，兜底为自动判断。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum TaskType {
    /// 纯对话，无工具调用。最精简的 prompt。
    Chat,
    /// 文档生成：XLSX/DOCX/PPTX/PDF。办公工具权限。
    Office,
    /// 代码编写/修改/调试。完整工具权限 + CRAFT + LSP + 符号索引。
    #[default]
    Code,
    /// 架构分析/设计评审。只读权限，禁止写文件。
    Architecture,
}
```

---

## 四种类型的 Prompt 差异

### Token 预算对比（粗略估算）

| 任务类型 | 现在（全量） | TaskType后 | 节省 |
|---------|------------|-----------|------|
| Chat | ~8500 token | ~1200 token | **86%** |
| Office | ~8500 token | ~2800 token | **67%** |
| Code | ~8500 token | ~8500 token | 0%（不裁剪） |
| Architecture | ~8500 token | ~3500 token | **59%** |

### 工具权限矩阵

| 工具 | Chat | Office | Code | Architecture |
|------|:----:|:------:|:----:|:------------:|
| read_file | ❌ | ✅ | ✅ | ✅ |
| write_file | ❌ | ✅ | ✅ | ❌ |
| edit_file | ❌ | ❌ | ✅ | ❌ |
| grep_files | ❌ | ❌ | ✅ | ✅ |
| list_dir | ❌ | ✅ | ✅ | ✅ |
| file_search | ❌ | ✅ | ✅ | ✅ |
| exec_shell | ❌ | ❌ | ✅ | ❌ |
| write_office | ❌ | ✅ | ❌ | ❌ |
| agent_spawn | ❌ | ❌ | ✅ | ✅（只读子agent） |
| apply_patch | ❌ | ❌ | ✅ | ❌ |

### Prompt 内容裁剪规则

**Chat**（极简）：
- ✅ base.md 核心行为规范
- ✅ personality overlay
- ✅ 语言设置
- ❌ 所有工具说明
- ❌ CRAFT 流程
- ❌ LSP 诊断说明
- ❌ 符号索引说明
- ❌ 工作区上下文（AGENTS.md/pick-rules.md）
- ❌ Skills 目录

**Office**（文档生成）：
- ✅ base.md
- ✅ personality
- ✅ 工作区上下文
- ✅ write_office / read_file 工具说明
- ✅ 办公格式规范（xlsx/docx/pptx/pdf）
- ❌ CRAFT 流程
- ❌ LSP 诊断
- ❌ 符号索引
- ❌ exec_shell / edit_file 工具说明

**Code**（完整，不裁剪）：
- ✅ 全量注入（与现在完全一致）
- ✅ CRAFT + Auditor + 符号索引 + LSP
- ✅ 全工具权限

**Architecture**（只读分析）：
- ✅ base.md
- ✅ 工作区上下文（AGENTS.md）
- ✅ read_file / grep_files / list_dir / agent_spawn（只读子agent）
- ✅ 架构分析规范（输出格式：Mermaid图/决策记录/风险点）
- ❌ edit_file / write_file / exec_shell（**硬禁止，写入系统prompt**）
- ❌ CRAFT 实施流程
- ❌ 办公工具

---

## 新增文件清单

```
crates/tui/src/prompts/tasks/
├── chat.md           # 极简 prompt，无工具引用
├── office.md         # 办公任务规范
├── architecture.md   # 架构分析规范 + 禁止写文件声明
└── code.md           # 代码任务（空文件或仅补充说明，Code = 现有行为）
```

### `prompts/tasks/chat.md` 内容骨架

```markdown
## Task Mode: Chat

You are in conversation mode. This session is for discussion, questions,
and thinking — not for file operations or code execution.

Rules:
- Do NOT call any tools. Answer directly from knowledge and reasoning.
- Keep responses concise and conversational.
- If the user asks you to edit files or run commands, explain that
  they should switch to Code mode for that.
```

### `prompts/tasks/office.md` 内容骨架

```markdown
## Task Mode: Office

You are generating office documents (XLSX, DOCX, PPTX, PDF).

Rules:
- Use write_office for all document generation.
- Always confirm file path and format before generating.
- For XLSX: apply headers, column widths, number formats by default.
- For DOCX/PPTX: use the project theme if available.
- Do NOT run exec_shell or edit source code files.
- Do NOT spawn sub-agents unless explicitly asked.
```

### `prompts/tasks/architecture.md` 内容骨架

```markdown
## Task Mode: Architecture

You are performing architecture analysis, design review, or technical
assessment. You are READ-ONLY in this mode.

Hard rules (cannot be overridden by user instruction):
- Do NOT call edit_file, write_file, apply_patch, or exec_shell.
- Do NOT modify any file under any circumstances.
- If asked to make changes, output the proposed change as a diff or
  description only — do not apply it.

Output standards:
- Use Mermaid diagrams for architecture visualization.
- Document decisions in ADR format (Context / Decision / Consequences).
- Flag risks explicitly with [RISK] prefix.
- Reference specific file paths and line numbers for all findings.
```

---

## 代码改动点

### T1：`TaskType` 枚举定义

**新建文件**：`crates/tui/src/task_type.rs`（~30行）

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Chat,
    Office,
    #[default]
    Code,
    Architecture,
}

impl TaskType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskType::Chat => "chat",
            TaskType::Office => "office",
            TaskType::Code => "code",
            TaskType::Architecture => "architecture",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            TaskType::Chat => "💬 聊天",
            TaskType::Office => "📄 办公",
            TaskType::Code => "💻 代码",
            TaskType::Architecture => "🏗 架构",
        }
    }

    /// 是否允许写文件操作
    pub fn allows_writes(&self) -> bool {
        matches!(self, TaskType::Code | TaskType::Office)
    }

    /// 是否注入完整工具说明
    pub fn needs_full_tools(&self) -> bool {
        matches!(self, TaskType::Code)
    }
}
```

---

### T2：compile-time 嵌入 task overlay

**改动文件**：`crates/tui/src/prompts.rs`

在现有的 `include_str!` 块旁边加：

```rust
// Task type overlays (compile-time embed)
const TASK_CHAT: &str = include_str!("prompts/tasks/chat.md");
const TASK_OFFICE: &str = include_str!("prompts/tasks/office.md");
const TASK_CODE: &str = include_str!("prompts/tasks/code.md");
const TASK_ARCHITECTURE: &str = include_str!("prompts/tasks/architecture.md");
```

新增辅助函数：

```rust
fn task_overlay(task_type: TaskType) -> &'static str {
    match task_type {
        TaskType::Chat => TASK_CHAT,
        TaskType::Office => TASK_OFFICE,
        TaskType::Code => TASK_CODE,
        TaskType::Architecture => TASK_ARCHITECTURE,
    }
}
```

---

### T3：`compose_prompt_with_approval()` 加 task_type 参数

**改动文件**：`crates/tui/src/prompts.rs`

```rust
// 现有签名
pub fn compose_prompt_with_approval(
    mode: AgentMode,
    approval: ApprovalPolicy,
    context: &PromptSessionContext,
) -> String

// 改为
pub fn compose_prompt_with_approval(
    mode: AgentMode,
    approval: ApprovalPolicy,
    task_type: TaskType,          // 新增
    context: &PromptSessionContext,
) -> String {
    // ...现有逻辑...

    // 在 mode overlay 之后注入 task overlay
    let task = task_overlay(task_type);
    if !task.is_empty() {
        parts.push(task);
    }

    // Chat 模式裁剪：跳过工具说明和 CRAFT 相关内容
    if task_type == TaskType::Chat {
        // 不注入 context_management_block
        // 不注入 skills_block
        // 不注入 instructions_block（含 pick-rules.md）
    }
}
```

---

### T4：`system_prompt_for_mode_with_context_skills_session_and_approval()` 传递 task_type

**改动文件**：`crates/tui/src/prompts.rs:445`

```rust
// 总入口函数签名加 task_type 参数
pub async fn system_prompt_for_mode_with_context_skills_session_and_approval(
    mode: AgentMode,
    task_type: TaskType,    // 新增
    approval: ApprovalPolicy,
    context: &PromptSessionContext,
    // ...其余参数不变
) -> SystemPrompt
```

---

### T5：Engine 持有 task_type，refresh 时传入

**改动文件**：`crates/tui/src/core/engine.rs`

```rust
pub struct Engine {
    // ...现有字段...
    task_type: TaskType,    // 新增
}

impl Engine {
    pub fn set_task_type(&mut self, task_type: TaskType) {
        self.task_type = task_type;
        // 触发 system prompt 刷新
        self.refresh_system_prompt(self.mode);
    }
}
```

`refresh_system_prompt()` 调用时透传 `self.task_type`。

---

### T6：自动判断兜底逻辑

**新建文件**：`crates/tui/src/task_type.rs`（加在 T1 的同一文件里）

```rust
/// 自动判断 TaskType，仅在用户未手动选择时使用。
/// 规则优先于模型——零幻觉风险，响应即时。
pub fn infer_task_type(workspace: &Path, first_message: Option<&str>) -> TaskType {
    // 规则1：工作区有代码文件 → Code（优先级最高）
    if workspace_has_code(workspace) {
        // 但消息关键词可以覆盖为 Architecture
        if let Some(msg) = first_message {
            if contains_architecture_keywords(msg) {
                return TaskType::Architecture;
            }
        }
        return TaskType::Code;
    }

    // 规则2：消息包含办公关键词 → Office
    if let Some(msg) = first_message {
        if contains_office_keywords(msg) {
            return TaskType::Office;
        }
        if contains_architecture_keywords(msg) {
            return TaskType::Architecture;
        }
    }

    // 兜底 → Chat
    TaskType::Chat
}

fn workspace_has_code(workspace: &Path) -> bool {
    const CODE_EXTENSIONS: &[&str] = &["rs", "ts", "tsx", "py", "go", "js", "jsx"];
    // 只扫描顶层 + src/ 目录，不全量遍历
    for ext in CODE_EXTENSIONS {
        if quick_has_extension(workspace, ext) {
            return true;
        }
    }
    false
}

fn contains_office_keywords(msg: &str) -> bool {
    let msg = msg.to_lowercase();
    ["文档", "报告", "表格", "excel", "xlsx", "pptx", "docx",
     "幻灯片", "演示", "word", "pdf", "spreadsheet"]
        .iter().any(|kw| msg.contains(kw))
}

fn contains_architecture_keywords(msg: &str) -> bool {
    let msg = msg.to_lowercase();
    ["架构", "设计", "评审", "分析", "review", "architecture",
     "diagram", "流程图", "时序", "依赖关系", "模块划分"]
        .iter().any(|kw| msg.contains(kw))
}
```

---

### T7：Desktop UI — 下拉选择器

**改动文件**：`crates/desktop/web-ui/src/components/InputBar.tsx`（或现有下拉所在组件）

在现有的 `[模式: Agent ▼]` 旁边加任务类型选择器：

```tsx
const TASK_TYPES = [
  { value: 'chat',         label: '💬 聊天' },
  { value: 'office',       label: '📄 办公' },
  { value: 'code',         label: '💻 代码' },
  { value: 'architecture', label: '🏗 架构' },
] as const;

// 状态
const [taskType, setTaskType] = useState<TaskType>('code');
const [taskTypeAuto, setTaskTypeAuto] = useState(true); // true = 自动判断

// UI
<select
  value={taskTypeAuto ? 'auto' : taskType}
  onChange={(e) => {
    if (e.target.value === 'auto') {
      setTaskTypeAuto(true);
    } else {
      setTaskTypeAuto(false);
      setTaskType(e.target.value as TaskType);
    }
  }}
>
  <option value="auto">⚡ 自动</option>
  <option value="chat">💬 聊天</option>
  <option value="office">📄 办公</option>
  <option value="code">💻 代码</option>
  <option value="architecture">🏗 架构</option>
</select>
```

用户选"自动"时，前端不传 task_type，后端走 `infer_task_type()` 判断。
用户选具体类型时，前端通过 `invoke('set_task_type', {taskType})` 传给 Rust 侧。

---

### T8：config.toml 持久化

用户的任务类型偏好可以持久化：

```toml
# ~/.deepseek/config.toml 或 .deepseek/config.toml
[task]
default_type = "code"   # chat | office | code | architecture | auto
```

项目级别的 `.deepseek/config.toml` 可以覆盖全局默认——比如一个纯文档仓库默认 `office`，一个代码仓库默认 `code`。

---

## 实施顺序

| 批次 | 项 | 改动量 | 收益 |
|:----:|-----|--------|------|
| **P** | T1（枚举定义）+ T6（自动判断规则） | ~60行 | 基础类型定义，无UI，可单元测试 |
| **Q** | T2（compile-time嵌入）+ 四个tasks/*.md文件 | ~80行+4文件 | prompt内容差异落地 |
| **R** | T3+T4（compose函数加参数）+ T5（Engine持有） | ~40行 | 完整后端逻辑 |
| **S** | T7（Desktop UI下拉）+ T8（config持久化） | ~50行 | 用户可见功能 |

---

## KV Cache 影响分析

TaskType的引入对KV Cache的影响需要认真考虑：

**有利影响**：
- Chat任务的system prompt大幅缩短，相同任务的cache命中率更高
- 同一taskType的连续session前缀完全一致，cache效果好

**潜在影响**：
- 用户在session中途切换TaskType → system prompt变化 → cache bust
- 建议：TaskType切换触发新session而非在当前session中刷新prompt
  这样每个session内TaskType固定，cache前缀稳定

**结论**：TaskType在session级别固定，不允许session中途切换，
保持现有KV Cache分层的完整性。

---

## 对比：改动前后

| 维度 | 改动前 | 改动后 |
|------|--------|--------|
| 聊天任务system prompt | ~8500 token（含全量工具说明） | ~1200 token（极简） |
| 架构分析误改代码风险 | 无约束 | 硬禁止写文件（写在system prompt里） |
| 办公任务看到CRAFT说明 | ✅（无关内容） | ❌（不注入） |
| 用户感知 | 只有Agent/Plan/YOLO模式选择 | 额外任务类型下拉，默认自动 |
| 自动判断 | 无 | 规则based，零模型调用，即时 |

---

## 注意事项

**Architecture模式的硬禁止写入**是这次最重要的安全改进——
把"不要修改文件"从pick-rules.md的软约束（模型可能忽略）升级为system prompt的硬声明。
根据之前对幻觉的研究，模型在system prompt里看到明确禁止比在rules文件里看到更可靠。

**Chat模式不注入工作区上下文**——这意味着Chat模式下模型不知道当前项目的技术栈和规则。
如果用户在Chat模式下问项目相关的问题，可能得到泛化答案而不是项目特定答案。
这是可接受的权衡——Chat模式本来就是用于通用对话，不是项目操作。
如果需要项目上下文，用户应该切换到Code或Architecture模式。

**自动判断的局限**：关键词匹配无法处理歧义场景，比如"分析这段代码并修复问题"既有"分析"（Architecture关键词）又是代码修复（Code场景）。
建议：关键词冲突时优先Code（因为Code是功能最全的兜底）。
