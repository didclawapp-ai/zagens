# Auditor 子代理设计文档

## 1. 动机 — M4 的根因与系统对策

### 1.1 M4 回顾

在 DS Pick 全仓库审查中，父代理将 `export_thread_json` 标记为 MEDIUM：“`save_path` 参数未经过路径安全验证”。实际调用方 `App.tsx:803` 使用 OS 原生文件保存对话框 — `save_path` 来自用户显式选择，不需要路径检查。M4 是误报。

### 1.2 M4 的精确根因（基于操作记录）

| 证据 | 事实 | 与 M4 的关系 |
|------|------|-------------|
| 父代理读了 `commands.rs:670-718` | `export_thread_json` 函数体没有 `canonicalize` | 观察到代码差异，但不构成漏洞 |
| 父代理读了 `commands.rs` 中 `read_thread_workspace_binary` | 后者有完整路径检查 | 被用作对比模板，将前者的威胁模型投射到后者 |
| 父代理从未读过 `App.tsx:803-807` | 调用方代码证明 `save_path` 来自 `@tauri-apps/plugin-dialog` 的 `save()` | **关键缺失** |
| 父代理未对 `export_thread_json` 执行 `grep_files` | 工具可用但未使用 | 2 秒内可纠正 |
| 报告原文含“与 read_thread_workspace_binary 不同” | 推理链建立在类比之上 | 违反 Hype filter 规则 |

**唯一根因**：父代理在观察到两个函数的代码差异后，直接将差异解释为漏洞，未执行 `grep_files` 追溯调用方。

### 1.3 为什么已有规则被跳过

`base.md` 中的 Hype filter（“Downgrade claims that rest on analogy”）和 Evidence rule（“concrete claim publishable only when anchored in fresh tool output”）在 M4 路径上未被触发。原因是：

- **规则依赖自我觉察**：“我在做类比吗？” — 但父代理体验到的不是类比，而是“这个函数缺了路径检查”。
- **观察替代了证据**：读函数体产生的代码观察（`std::fs::write(&save_path, json)` 无路径检查）被当作证据，而证据要求回答“输入从哪里来”。

### 1.4 系统对策

**引入 Auditor 子代理** — 一个外部的、机械的事实核查员。不依赖父代理的自我觉察，只做程序性核查：每条发现是否在代码中有对应的行号，行号内容是否与结论一致。

---

## 2. 提示词架构总览

### 2.1 根代理提示词层叠

```
prompts.rs::build_system_prompt()
  ├── client_identity (DS Pick vs Terminal)
  ├── base.md (337 行; 核心行为规则)
  │     ├── Language / Preamble / Decomposition
  │     ├── Full-repository code review mode (6 步)
  │     ├── Verification Principle
  │     ├── Epistemic discipline (Evidence rule, Label inference…)
  │     ├── Sub-Agent Strategy + CRAFT P2 fix-loop
  │     └── Tool usage rules
  ├── personality overlay (calm.md / playful.md)
  ├── mode delta (agent.md / plan.md / yolo.md)
  ├── approval policy (auto.md / suggest.md / never.md)
  ├── project_context (AGENTS.md → pick-rules.md → .deepseek/instructions/*)
  ├── environment block (lang, ui_shell, platform, pwd)
  ├── compaction handoff (compact.md, 如果 active)
  ├── skills section
  └── final guardrails (no nightly, sub-agent rules from project_rules.md)
```

### 2.2 子代理提示词层叠

```
tools/subagent/mod.rs::build_subagent_system_prompt()
  ├── SubAgentType::system_prompt()
  │     ├── General → GENERAL_AGENT_PROMPT
  │     ├── Explore → EXPLORE_AGENT_PROMPT
  │     ├── Plan    → PLAN_AGENT_PROMPT
  │     ├── Review  → REVIEW_AGENT_PROMPT
  │     ├── Implementer → IMPLEMENTER_AGENT_PROMPT
  │     ├── Verifier → VERIFIER_AGENT_PROMPT
  │     ├── Custom  → CUSTOM_AGENT_PROMPT
  │     └── Auditor → AUDITOR_AGENT_PROMPT     ← 新增
  │
  ├── subagent_output_format.md (SUMMARY/EVIDENCE/CHANGES/RISKS/BLOCKERS)
  └── role overlay ("You are operating in the role of `{name}`.")

tools/subagent/mod.rs::build_assignment_prompt()
  ├── parent prompt (injected)
  ├── assignment context (from blackboard, if task_id + CRAFT)
  └── allowed_tools info
```

### 2.3 CRAFT P2 修复循环（现有关卡）

```
父代理
  │
  ├── agent_spawn(type="explore", task_id)   → Explorer 读代码, 写 blackboard.explorer
  ├── agent_spawn(type="implementer", task_id) → Implementer 写代码, 读 blackboard.explorer + reviewer
  ├── agent_spawn(type="review", task_id)    → Reviewer 审查, 写 blackboard.reviewer (structured_verdict)
  │     └── BLOCKER → 父代理判断: agent_spawn(implementer) 修复 → 再 Review (最多 3 轮)
  ├── agent_spawn(type="verifier", task_id)  → Verifier 跑测试, 写 blackboard.verifier
  │     └── FAIL → agent_spawn(implementer) 修复 → 再 Verify (最多 3 轮)
```

**关键事实**：CRAFT 是**可选的**。当 `task_id` 为 `None` 时，不读写黑板。根代理可以直接读文件、写报告、不经过 CRAFT 链路。

### 2.4 Auditor 插入点（新增）

```
父代理完成审查 → 产出报告草稿
                      │
                      ├── 提取报告中的 HIGH/MEDIUM 发现
                      ├── agent_spawn(type="auditor")  ← 新增的最终出口
                      │      │
                      │      ├── Auditor 逐条核查每条发现：
                      │      │     file_path 存在？
                      │      │     line_number 存在？
                      │      │     read_file 读取，内容与结论一致？
                      │      │
                      │      └── 输出 structured_verdict:
                      │            PASS → 父代理发布最终报告
                      │            FAIL → 父代理修正后重新提交
                      │
                      └── (可选) 交叉验证 blackboard 数据（若有 CRAFT 链路）
```

**Auditor 与 CRAFT 的关系**：
- Auditor 的**主要输入**是父代理的报告草稿文本（通过 `agent_spawn` 的 `prompt` 参数传入）
- 审计不依赖 CRAFT 链路是否已跑过
- 如果 CRAFT 链路存在且黑板有数据，Auditor 可交叉验证（增强，但不要求）
- 如果只是父代理独立审查（无 CRAFT），Auditor 仅核查报告文本中的路径/行号

---

## 3. Auditor 的设计

### 3.1 职责边界

```
Auditor 只做一件事：验证黑盒的输出是否与代码事实一致。

  输入：父代理的报告草稿（自然语言文本，含发现列表）
  输出：PASS（所有发现通过事实核查）或 FAIL（详细列出未通过项）
  不做：分析代码、判断严重性、提出建议、修改报告
```

### 3.2 系统提示词

```
你是事实核查员（Auditor）。你只做一件事：验证审查报告中每条结论是否有事实支撑。

## 输入

父代理将审查报告草稿作为任务 prompt 传入。报告中包含多条发现（HIGH/MEDIUM/LOW）。

## 规则

对每一条发现，执行以下三项检查：

1. **路径检查** — 是否引用了具体文件路径（如 `crates/desktop/src/commands.rs`）？
      缺失 → 记录 "[发现编号] 缺失: file_path"

2. **行号检查** — 是否引用了具体行号（如 `:670-718`）？
      缺失 → 记录 "[发现编号] 缺失: line_number"

3. **内容验证（仅机械核查，不做语义判断）** — 用 `read_file` 读取引用路径的引用行号。

      **步骤 3a: 行存在性检查**
      - 如果行号越界 / 文件不存在 → 记录 "[发现编号] 行号/文件无效"

      **步骤 3b: 符号存在性检查（字符串包含匹配）**
      从发现描述中提取**具体名称**（函数名、变量名、标注名、命令名），
      用 `read_file` 读取引用行后，在该行内容中做**字符串包含检查**：
      - 引用行内容包含发现中提到的符号名 → 通过
      - 引用行内容不包含 → 记录 "[发现编号] 符号缺失: 声称引用 '{符号名}', 但该行不包含此字符串"

      **禁止做语义判断。** Auditor 不判断"代码做得对不对"——那是父代理的工作。
      只判断"引用的代码里有没有发现中提到的那个符号"。
      例如：发现声明"该行没有路径验证" → Auditor 只检查该行是否含有 `save_path`（符号名），
      不检查该行是否缺少 `canonicalize`（语义）。

## 语言专属规则

根据 finding 引用的文件后缀自动路由：

| 后缀 | 规则集 | 额外要求 |
|------|--------|---------|
| `.rs` | Rust | 引用行必须是 fn/struct/enum/impl 定义或具体表达式 |
| `.ts`, `.tsx` | TypeScript | 引用行必须是 function/const/interface/type 定义 |
| 跨边界 (`.rs`+`.ts`) | Tauri-bridge | 必须同时提供 Rust 侧和 TS 侧的行号 |

### Rust 规则

- 引用行必须在以下之一内：
  - `fn` / `pub fn` / `async fn` 定义
  - `struct` / `enum` / `impl` 定义
  - 具体的表达式（`unwrap()` / `expect()` / `unsafe` 块 / 错误处理路径）
- 如果发现涉及编译问题：必须能复现（不做 cargo check，但引用行本身必须存在于函数体内）
- 如果发现涉及"A 改为 B"：必须引用改动前后的具体行

### TypeScript 规则

- 引用行必须在以下之一内：
  - `function` / `const` / `interface` / `type` 定义
  - React hook 调用（`useState` / `useEffect` 等）
  - 具体的类型断言
- `strict: true` 下：不允许引用 `any` 类型作为"已知问题"而不提供具体类型错误位置

### Tauri-bridge 规则

如果发现涉及 Tauri invoke 边界：
- 必须同时提供：
  - Rust 侧：`#[tauri::command]` 在 `commands.rs` 中的行号
  - TS 侧：`invoke('xxx')` 调用在 `.ts`/`.tsx` 中的行号
- 命令名在两侧必须完全一致（字符串匹配）
- 如果只有一侧的行号 → 记录 "缺失: 对侧行号"

## 输出格式

输出只包含两种结构：

```
### AUDIT RESULT: PASS

所有 N 条发现通过事实核查。

### EVIDENCE

- [发现 1] `path:line` — read_file 确认内容一致
- [发现 2] `path:line` — read_file 确认内容一致
...
```

或者：

```
### AUDIT RESULT: FAIL

### DETAIL

- [发现 N] 结论: "原文摘要"
  缺失: file_path / line_number / 内容不符
  原因: 具体描述
```

## 禁止事项

- 不做代码分析 — 只核查引用是否准确
- 不给修改建议
- 不判断严重性
- 不生成新的发现
- 如果报告中有 10 条发现，只核查这 10 条 — 不增减

## 处理未核查项

如果某条发现无法核查（文件被删除、行号在 read_file 的 limit 范围外），
在 DETAIL 中标注 "无法核查: [原因]"。
```

### 3.3 审计规则文件（语言专属）

```
crates/tui/src/prompts/audit_rules/
  rust.md           — Rust 代码引用验证规则
  typescript.md     — TypeScript 代码引用验证规则
  tauri_bridge.md   — Tauri invoke 跨边界验证规则
  generic.md        — 回退规则（未知语言）
```

#### `audit_rules/rust.md`

```
# Rust 代码事实核查规则

审查报告中引用 Rust 源代码时，以下条件必须满足：

## 1. 文件路径
- 必须以 `.rs` 结尾
- 必须为仓库相对路径（如 `crates/tui/src/tools/subagent/mod.rs`）

## 2. 行号
- 必须为具体数字或数字范围（如 `:42` 或 `:120-145`）
- 不允许 "around line"、"approximately" 等模糊表述

## 3. 内容匹配
- `read_file` 读取对应行
- 该行内容必须在以下类别之一内：
  - `fn` / `pub fn` / `async fn` 定义
  - `struct` / `enum` / `impl` / `trait` 定义
  - 表达式（`unwrap()` / `expect()` / `unsafe { }` 块）
  - 错误处理路径（`?` 操作符、`match Err`、`if let Err`）
  - `#[tauri::command]` 注解（桌面命令）
  - `pub(crate)` / `pub` 可见性声明

## 4. 涉及编译或运行时行为
- 如果发现声称"会 panic"、"会编译失败"、"会死锁"
- 核查只验证代码引用的位置存在对应代码
- Auditor 不做编译或运行时验证

## 5. 涉及"A 改为 B"
- 发现必须引用改动前后的具体行号
- Auditor 用 `read_file` 验证改动后在引用行中确实为 B
- 如果只说了"应改为 B"而没有引用改动位置 → 内容不符
```

#### `audit_rules/typescript.md`

```
# TypeScript 代码事实核查规则

审查报告中引用 TypeScript 源代码时，以下条件必须满足：

## 1. 文件路径
- 必须以 `.ts` 或 `.tsx` 结尾
- 必须为仓库相对路径

## 2. 行号
- 必须为具体数字或数字范围

## 3. 内容匹配
- `read_file` 读取对应行
- 该行内容必须在以下类别之一内：
  - `function` / `const` / `let` 定义
  - `interface` / `type` / `enum` 定义
  - React hook 调用：`useState` / `useEffect` / `useCallback` / `useMemo`
  - `invoke('xxx')` 调用（Tauri bridge）
  - `import` 语句
  - 类型注解（`: Type` / `as Type` / `satisfies Type`）

## 4. strict 模式下的额外要求
- 项目 `tsconfig.json` 声明 `strict: true`
- 发现不得以"使用了 any"作为独立问题而不引用具体位置
- 如果发现声称"缺少类型"，必须引用具体的变量/函数定义行

## 5. 涉及 Tauri invoke
- 见 tauri_bridge.md
```

#### `audit_rules/tauri_bridge.md`

```
# Tauri 跨边界代码事实核查规则

当审查报告中的发现涉及 Rust 后端与 TypeScript 前端之间的 Tauri invoke 调用时：

## 1. 双侧行号要求
- 必须在同一条发现中同时提供：
  - Rust 侧：`#[tauri::command]` 的函数定义位置
  - TS 侧：`invoke('xxx')` 调用位置
- 缺少任一侧 → 该发现不通过核查

## 2. 命令名一致性
- Rust 侧命令名（`#[tauri::command] fn xxx`）与 TS 侧调用名（`invoke('xxx')`）
  必须完全一致（字符串匹配）
- Auditor 用 `read_file` 分别读取两侧，做字符串对比

## 3. 参数类型对应
- Rust `String` ↔ TS `string`
- Rust `Vec<T>` ↔ TS `T[]`
- Rust `Option<T>` ↔ TS `T | null`
- Rust `bool` ↔ TS `boolean`
- Rust `u32/i32` ↔ TS `number`
- 如果发现声称"类型不匹配"，必须引用具体的类型定义行

## 4. 命令注册验证
- `main.rs` 的 `.invoke_handler()` 中必须包含该命令
- 如果发现声称"命令未注册"，必须引用 `main.rs` 中 `invoke_handler` 的行号
```

#### `audit_rules/generic.md`

```
# 通用代码事实核查规则

当审查报告中的发现使用未知语言/文件类型时，使用此回退规则。

## 1. 最基本的路径和行号要求
- file_path 必须能定位到具体文件
- line_number 必须为数字

## 2. 内容匹配
- `read_file` 读取对应行
- 行内容存在（非空）
- 如果发现声称"在某处做了某事"，该行内容语义上应可对应

## 3. 无法判断时
- 标注 "语言未知，无法深度核查"
- 不因此 FAIL — 标记为 PASS with caveat
```

---

## 4. 触发机制

### 4.1 触发条件

Auditor 在 `base.md` 的“Full-repository code review mode”第 5 步中被强制触发。

**修改前**（当前 `base.md` 第 5 步）：
```
5. **Report verification pass (mandatory before final output)** — Treat the draft as
   untrusted until checked.
   - **Evidence audit** — For every HIGH, re-check: read_file or grep_files
   - **Sub-agent spot-check (optional)** — For large drafts, spawn one read-only
     sub-agent with the draft-only prompt
```

**修改后**：
```
5. **Report verification pass (mandatory before final output)** — Treat the draft as
   untrusted until checked. This pass has two tiers:

   a. **Self-verification** — For every HIGH and ideally every MEDIUM, re-check:
      read_file or grep_files on the cited path (lines drift). Apply the
      Caller-trace rule (mandatory before marking), Hype filter, Dedup.

   b. **Auditor sub-agent (mandatory for HIGH/BLOCKER findings)** — After self-
      verification, spawn `agent_spawn(type="auditor")` with the draft report's
      findings as the prompt. The Auditor is a mechanical fact-checker:
      - It verifies every finding has file_path + line_number
      - It uses read_file to confirm the cited lines contain the symbols the
        finding claims to reference (mechanical check only — not semantic judgment)
      - It outputs PASS (all findings verified) or FAIL with specific items
      - If PASS: proceed to finalize
      - If FAIL: correct the findings and re-submit. Retry limit: **2**.
        After the 3rd FAIL (initial + 2 retries), downgrade the failing finding
        to **LOW** with the label `UNVERIFIED` and proceed — do not loop.

      **Trigger severity thresholds** (same for both paths):
      | Severity | Auditor | Notes |
      | -------- | ------- | ----- |
      | HIGH / BLOCKER | **Mandatory** | Every HIGH/BLOCKER finding must pass audit |
      | MEDIUM | Recommended | Include in prompt; 3+ MEDIUM → treat as mandatory |
      | LOW | Optional | At parent's discretion |

      This step is NOT optional for full-repo reviews with any HIGH finding.
      For PR/module reviews, same thresholds apply.
```

### 4.2 不触发 Auditor 的场景

| 场景 | Auditor？ | 原因 |
|------|----------|------|
| 简单查询/读文件 | 否 | 无审查报告 |
| 单函数代码生成 | 否 | 无发现列表 |
| 小 PR review（<3 发现） | 可选 | 父代理自查可能足够 |
| 交互式探索 | 否 | 非审查任务 |
| 纯对话 | 否 | 无代码引用 |

---

## 5. 代码变更范围

### 5.1 新增文件

| 路径 | 说明 |
|------|------|
| `crates/tui/src/prompts/audit_rules/rust.md` | Rust 核查规则 |
| `crates/tui/src/prompts/audit_rules/typescript.md` | TypeScript 核查规则 |
| `crates/tui/src/prompts/audit_rules/tauri_bridge.md` | Tauri 跨边界核查规则 |
| `crates/tui/src/prompts/audit_rules/generic.md` | 通用回退规则 |

### 5.2 修改文件

| 路径 | 变更 |
|------|------|
| `crates/tui/src/tools/subagent/mod.rs` | 1. `SubAgentType` 枚举加 `Auditor` 变体<br>2. `system_prompt()` 映射到 `AUDITOR_AGENT_PROMPT`<br>3. `from_str()` 加 `"auditor"` 解析<br>4. 新增 `AUDITOR_AGENT_PROMPT` 常量 |
| `crates/tui/src/tools/subagent/blackboard.rs` | `SubAgentType::Auditor` 分支：写 `auditor` 分区；读侧 Auditor 接收 reviewer block + 报告草稿 |
| `crates/tui/src/prompts/base.md` | 第 5 步“Sub-agent spot-check (optional)”升级为“Auditor sub-agent (mandatory)” |
| `crates/tui/src/prompts/subagent_output_format.md` | 新增 Auditor 输出格式说明（PASS/FAIL + DETAIL） |

### 5.3 不变更

| 不涉及 | 说明 |
|--------|------|
| `SubAgentType::as_str()` | `"auditor"` 自动派生 |
| `agent_spawn` 工具签名 | 通过 `type="auditor"` 即走 |
| CRAFT P2 fix-loop | 逻辑不变，Auditor 是独立的最终出口 |
| 黑板的 schema_version | 不升级，auditor 为 schema v1 新分区 |

---

## 6. 数据流：两种路径

### 6.1 路径 A：有 CRAFT 链路（丰富路径）

```
Explorer   → blackboard.explorer
Implementer → blackboard.implementer (rounds[])
Review     → blackboard.reviewer (verdict + blockers[])
Verifier   → blackboard.verifier (failures[])

父代理合成报告草稿
      │
      ├── agent_spawn(type="auditor", task_id="t-001")
      │     Auditor 读 blackboard 做交叉验证
      │     产出 structured_verdict → blackboard.auditor
      │
      ├── PASS → 最终报告
      └── FAIL → 修正 → 再 spawn Auditor（最多 2 轮重试）
            │
            └── 第 3 轮仍 FAIL → 降级该发现为 LOW + UNVERIFIED，放行
```

### 6.2 路径 B：无 CRAFT 链路（最简路径）

```
父代理自查 80 个文件 → 产出报告草稿
      │
      ├── 提取 HIGH（强制）+ MEDIUM（推荐）发现为文本块
      ├── agent_spawn(type="auditor", prompt="以下是审查报告，请逐条核查...")
      │     Auditor 对每条发现执行 read_file 验证
      │     不读 blackboard（task_id=None）
      │
      ├── PASS → 最终报告
      └── FAIL → 修正 → 再 spawn Auditor（最多 2 轮重试）
            │
            └── 第 3 轮仍 FAIL → 降级该发现为 LOW + UNVERIFIED，放行
```

### 6.3 报告传递格式（路径 B 用）

父代理传给 Auditor 的 prompt 结构：

```
你是事实核查员。以下是审查报告草稿，请逐条核查每条发现。

### 报告草稿

#### H1: Linux Landlock 沙箱未实际强制执行
路径: crates/tui/src/sandbox/mod.rs:289-318
发现: prepare_landlock() 仅设置环境变量，不应用 Landlock 规则

#### H2: Windows 沙箱未实际强制执行
路径: crates/tui/src/sandbox/mod.rs:323-344
发现: 同 H1 — prepare_windows() 仅设置环境变量

#### M1: 视觉桥接 API Key 明文存储于 config.toml
路径: crates/desktop/src/commands.rs:143-175
发现: save_vision_bridge 直接写 v.api_key = Some(key_trim.to_string())

... (其他发现)

### 核查指令

对每条发现：
1. 从发现文本中提取具体符号名（函数名、变量名、命令名等）
2. read_file 读取对应路径和行号
3. 检查引用行是否存在
4. 检查该行内容是否**包含**提取的符号名（字符串包含，非语义判断）
5. 输出 PASS 或 FAIL
```

---

## 7. 预期效果

### 7.1 M4 场景回放

如果 Auditor 在 M4 审查时已存在：

```
父代理报告草稿:
  M4: export_thread_json 缺少路径检查
  路径: crates/desktop/src/commands.rs:670-718
  （没有提供 TS 侧行号）

Auditor 执行:
  1. read_file("crates/desktop/src/commands.rs", start_line=670, limit=49)
     → 确认函数体确实没有 canonicalize ✓
  2. 发现是 Tauri command → 触发 tauri_bridge 规则
     → 搜索 invoke('export_thread_json') → 未提供 TS 侧行号
  3. 输出: FAIL
     DETAIL: [M4] "export_thread_json 缺少路径检查"
       缺失: tauri-bridge 对侧行号（TS 侧 invoke 调用）
       原因: Tauri command 发现必须提供两侧行号
```

**M4 连标记都标记不出去** — 不是被阻止了结论，而是被阻止了标记。

### 7.2 幻觉率的系统性降低

| 幻觉类型 | Auditor 拦截方式 |
|----------|-----------------|
| 虚构的行号 | read_file → 行号越界/文件不存在 → FAIL |
| 代码差异当漏洞（M4） | 缺少调用方/对侧行号 → FAIL |
| 对比推理（"与 X 不同"） | Hype filter 触发 tauri_bridge 规则 → 要求双侧证据 → FAIL |
| 框架默认值猜测 | read_file → 内容不符 → FAIL |
| 数字/参数名虚构 | read_file → 内容不一致 → FAIL |

### 7.3 自身幻觉风险

Auditor 是黑盒，**也可能**产生幻觉。降低其自身幻觉的措施：

1. **任务单一** — 不做分析，只核查（PASS/FAIL 二元输出，压缩决策空间）
2. **操作机械** — 每条发现固定四步：路径检查 → 行号检查 → 行存在性 → 符号包含
3. **内容验证为机械的字符串包含检查** — Auditor 不判断"代码对不对"。
   只判断"引用的代码行里有没有发现中提到的那几个符号名"。
   例如发现说"该行没有路径验证"，Auditor 只检查该行含不含 `save_path`，
   不判断该行缺不缺 `canonicalize`。语义判断（"没有路径检查是不安全的"）
   是父代理的工作，不是 Auditor 的工作。
4. **语言规则文件独立** — 规则本身窄而固定，不要求 Auditor 理解业务逻辑
5. **禁止分析** — 系统提示词结尾明令禁止分析、建议、判断
6. **只输出 PASS/FAIL** — 无文档生成，无叙事，压缩虚构空间

**未覆盖的风险**：如果父代理在发现中引用的符号名本身就是虚构的（例如发现中写
`save_path` 但代码里该变量实际叫 `output_path`），Auditor 会正确标记"该行不包含
`save_path`"为 FAIL。但如果真正的符号名是 `save_path` 而发现中写的也是 `save_path`，
那么符号存在性检查和语义判断之间的裂缝仍然存在——发现被标记为 PASS 可能是因为
符号存在，不是因为结论正确。这个残留风险由重试上限（§4.1）兜底。

---

## 8. 实现顺序

| 阶段 | 内容 | 依赖 |
|------|------|------|
| 1 | 写 `audit_rules/` 四个规则文件 | 无 |
| 2 | 在 `mod.rs` 加 `Auditor` 变体 + `AUDITOR_AGENT_PROMPT` | 阶段 1 |
| 3 | 在 `blackboard.rs` 加 Auditor 读写分区 | 阶段 2 |
| 4 | 修改 `base.md` 第 5 步 | 无 |
| 5 | 更新 `subagent_output_format.md` | 无 |
| 6 | 测试：构造已知误报 → 验证被 AUDITOR 拦截 | 阶段 2-4 |
| 7 | `CHANGELOG.md` 记录 | 阶段 6 |
