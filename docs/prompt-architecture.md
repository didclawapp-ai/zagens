# Prompt System Architecture

本文档用 Mermaid 图描述 **Zagens 桌面端**（经 sidecar 启动的 `deepseek-runtime-server`）的 prompt 系统完整架构。函数名、模块名与源码对应；**行号仅作近似锚点**，以符号搜索为准。

**源码根（D6 Phase B 之后）：** `crates/runtime-server/src/` — 主入口 `prompts.rs`、分层 Markdown 在 `prompts/`、子 Agent 在 `tools/subagent/mod.rs`。历史路径 `crates/tui/src/prompts/` 已移除。

**最后对齐：** 2026-05-26（`DEEPSEEK_CLIENT_SURFACE=zagens` + 遗留 `ds-pick`）。

---

## 1. 系统 Prompt 组装层次

```mermaid
flowchart TD
    subgraph COMPILE["compile-time 嵌入 (include_str!)"]
        direction LR
        F0a["prompts/base.md · base-office.md<br/>Code / Office 核心契约"]
        F0b["prompts/tasks/code.md · office.md<br/>task overlay"]
        F2["prompts/personalities/calm.md<br/>voice overlay"]
        F3["prompts/modes/agent.md<br/>mode delta"]
        F4["prompts/approvals/suggest.md<br/>approval policy"]
        F5["prompts/compact.md<br/>handoff template"]
        F6["prompts/modes/plan.md"]
        F7["prompts/modes/yolo.md"]
        F8["prompts/approvals/auto.md"]
        F9["prompts/approvals/never.md"]
    end

    subgraph ASSEMBLY["prompts.rs"]
        direction TB
        A0["compose_base_prompt_layer_for_task()<br/>client_identity + base.md | base-office.md"]
        A1["compose_prompt_with_approval()<br/>base + personality + mode + task + approval"]
        A2["compose_mode_prompt_with_approval()<br/>Calm personality wrapper"]
    end

    subgraph RUNTIME["运行时 IO 层"]
        direction TB
        B1["load_project_context_with_parents()<br/>AGENTS.md → .claude/... → CLAUDE.md"]
        B2["render_environment_block()<br/>lang / ui_shell / platform / shell / pwd"]
        B3["merge_instruction_paths_with_pick_rules()<br/>.deepseek/pick-rules.md + config.toml"]
        B4["render_instructions_block()<br/>&lt;instructions&gt; blocks"]
        B5["memory::compose_block()<br/>&lt;user_memory&gt; block"]
        B5b["topic_memory::compose_block()<br/>B2 topic graph (cadence)"]
        B6["render_available_skills_context_for_workspace()<br/>skills catalog"]
        B7["Context Management<br/>inline const (Agent/YOLO only)"]
        B8["COMPACT_TEMPLATE<br/>compile-time"]
        B9["load_handoff_block()<br/>.deepseek/handoff.md"]
    end

    ENTRY["system_prompt_for_mode_with_context_skills_session_and_approval()<br/>prompts.rs"]
    FINAL["SystemPrompt::Text(full_prompt)"]

    F0a --> A0
    F0b --> A1
    A0 --> A1
    F2 --> A1
    F3 --> A1
    F6 --> A1
    F7 --> A1
    F4 --> A1
    F8 --> A1
    F9 --> A1
    A1 --> A2

    A2 --> ENTRY
    B1 --> ENTRY
    B2 --> ENTRY
    B3 --> B4
    B4 --> ENTRY
    B5 --> ENTRY
    B5b --> ENTRY
    B6 --> ENTRY
    B7 --> ENTRY
    B8 --> ENTRY
    B9 --> ENTRY

    ENTRY --> FINAL

    style COMPILE fill:#1a1a2e,stroke:#16213e,color:#e0e0e0
    style ASSEMBLY fill:#0f3460,stroke:#1a1a2e,color:#e0e0e0
    style RUNTIME fill:#114d1a,stroke:#0a3a10,color:#e0e0e0
    style FINAL fill:#7c3a00,stroke:#5c2a00,color:#ffe0b0
```

## 2. KV Cache 分层 (静态 → 易变)

```
┌── compile-time constants ──────────────────────────────────────┐  ← cache hit
│  client_identity  →  base.md  →  personality  →  mode  →  task  →  approval  │
├── workspace-static ────────────────────────────────────────────┤  ← cache hit
│  project_context  →  environment  →  instructions  →  memory   │
│  →  session_goal  →  skills  →  context_mgmt  →  compact_tmpl │
├── VOLATILE BOUNDARY ───────────────────────────────────────────┤  ← cache BUST
│  handoff_block  (.deepseek/handoff.md — rewritten on /compact) │
│  working_set    (injected into user message, NOT system prompt)│
└────────────────────────────────────────────────────────────────┘
```

---

## 3. 运行时整体流向

```mermaid
flowchart TD
    subgraph INIT["启动"]
        E1["Engine::new()<br/>core/engine/build.rs"]
        E2["refresh_system_prompt()<br/>core/engine/cycle_hooks.rs"]
        E3["merge_system_prompts()<br/>compaction/prompt.rs"]
        E4["system_prompt_hash()<br/>cycle_hooks · hash 去重"]
    end

    subgraph TURN["Turn Loop"]
        T0["每轮开始"]
        T1["refresh_system_prompt()<br/>turn_loop/host_impl · capacity_flow"]
        T2{"compaction.enabled<br/>&& should_compact()?"}
        T3["compact_messages_safe()<br/>compaction/ · 点击压缩旧消息"]
        T4["merge_compaction_summary()<br/>summary 合并入 system prompt"]
        T5["messages_with_turn_metadata()<br/>core/engine/turn_loop/mod.rs"]
        T6["<turn_meta> 注入最新 user msg<br/>working set + today date"]
        T7["MessageRequest 组装<br/>model + messages + system + tools"]
        T8["build_chat_messages_for_request()<br/>client/chat.rs"]
        T9["build_chat_messages_with_reasoning()<br/>system → role: system JSON"]
        T10["POST /v1/chat/completions<br/>DeepSeek API"]
        T11["parse SSE stream → ContentBlock"]
        T12["下一轮"]
    end

    E1 --> E2
    E2 --> E3
    E3 --> E4

    T0 --> T1
    T1 --> T2
    T2 -->|yes| T3
    T3 --> T4
    T2 -->|no| T5
    T4 --> T5
    T5 --> T6
    T6 --> T7
    T7 --> T8
    T8 --> T9
    T9 --> T10
    T10 --> T11
    T11 --> T12
    T12 --> T0

    style INIT fill:#0f3460,stroke:#1a1a2e,color:#e0e0e0
    style TURN fill:#114d1a,stroke:#0a3a10,color:#e0e0e0
```

---

## 4. Compaction 详细流程

```mermaid
flowchart TD
    TRIGGER["estimated tokens > threshold<br/>compaction/tokens.rs · should_compact()"]
    CHECK{"compaction.enabled?"}
    COMPACT["compact_messages_safe()<br/>compaction/"]
    SUMMARIZE["LLM 总结旧消息<br/>CompactionResult { messages, summary_prompt }"]
    MERGE_SESSION["Engine::merge_compaction_summary()<br/>core/engine/compaction_ops · capacity_flow"]
    MERGE_FN["merge_system_prompts()<br/>compaction/prompt.rs → SystemPrompt::Blocks"]
    UPDATE_HASH["system_prompt_hash() → 更新 session"]
    WRITE_HANDOFF["写入 .deepseek/handoff.md<br/>compact.md 格式"]
    FMT["Goal / Constraints / Progress<br/>Key Decisions / Next step"]
    SKIP["跳过"]

    TRIGGER --> CHECK
    CHECK -->|no| SKIP
    CHECK -->|yes| COMPACT
    COMPACT --> SUMMARIZE
    SUMMARIZE --> MERGE_SESSION
    MERGE_SESSION --> MERGE_FN
    MERGE_FN --> UPDATE_HASH
    MERGE_SESSION --> WRITE_HANDOFF
    WRITE_HANDOFF --> FMT

    style TRIGGER fill:#7c3a00,stroke:#5c2a00,color:#ffe0b0
    style MERGE_FN fill:#0f3460,stroke:#1a1a2e,color:#e0e0e0
```

---

## 5. Sub-Agent Prompt 系统

```mermaid
flowchart TD
    PARENT["父 Agent 调用 agent_spawn()"]
    SPAWN["AgentSpawnTool::execute()<br/>subagent/mod.rs"]

    subgraph TYPES["subagent_system_prompt()<br/>tools/subagent/mod.rs"]
        direction LR
        G["General → GENERAL_AGENT_PROMPT"]
        X["Explore → EXPLORE_AGENT_PROMPT"]
        P["Plan → PLAN_AGENT_PROMPT"]
        R["Review → REVIEW_AGENT_PROMPT"]
        I["Implementer → IMPLEMENTER_AGENT_PROMPT"]
        V["Verifier → VERIFIER_AGENT_PROMPT"]
        A["Auditor → AUDITOR_AGENT_PROMPT"]
        C["Custom → CUSTOM_AGENT_PROMPT"]
    end

    ROLE["build_subagent_system_prompt()<br/>+ role overlay when set"]
    ASSIGN["build_assignment_prompt()<br/>objective + blackboard (CRAFT P1)"]
    ENGINE["run_subagent()<br/>child Engine, 独立 session"]
    RULES["prompts/audit_rules/*.md<br/>Auditor 语言规则（未 include；靠 read_file 或内联摘要）"]
    LOOP["for _step in 0..max_steps:<br/>stream → tool calls → results"]
    DONE["<deepseek:subagent.done> sentinel<br/>→ parent 的 message stream"]

    PARENT --> SPAWN
    SPAWN --> TYPES
    TYPES --> ROLE
    A -.-> RULES
    ROLE --> ASSIGN
    ASSIGN --> ENGINE
    ENGINE --> LOOP
    LOOP --> DONE

    style TYPES fill:#0f3460,stroke:#1a1a2e,color:#e0e0e0
    style ENGINE fill:#114d1a,stroke:#0a3a10,color:#e0e0e0
```

### Sub-Agent 类型与工具权限

| Agent Type | 系统 Prompt 常量 | 权限 |
|------------|------------------|------|
| **General** | `GENERAL_AGENT_PROMPT` | 全工具 (继承父级) |
| **Explore** | `EXPLORE_AGENT_PROMPT` | read-only: read_file, grep_files, list_dir, file_search |
| **Plan** | `PLAN_AGENT_PROMPT` | 分析工具: read_file, grep_files, file_search |
| **Review** | `REVIEW_AGENT_PROMPT` | 代码审查: read + analysis |
| **Implementer** | `IMPLEMENTER_AGENT_PROMPT` | 写代码: write_file, edit_file, apply_patch, + CRAFT git stash |
| **Verifier** | `VERIFIER_AGENT_PROMPT` | 运行测试: exec_shell, task_gate_run |
| **Auditor** | `AUDITOR_AGENT_PROMPT` | 机械事实核查（默认继承全工具面；规则见 `prompts/audit_rules/`） |
| **Custom** | `CUSTOM_AGENT_PROMPT` | spawn 时指定 |

---

## 6. RLM (Recursive Language Model) 独立路径

```mermaid
flowchart LR
    TOOL["rlm tool 被调用<br/>tools/rlm.rs"]
    PROMPT["rlm_system_prompt()<br/>rlm/prompt.rs:10"]
    REPL["Python REPL sandbox<br/>llm_query / rlm_query / FINAL"]
    RESULT["返回合成结果到父 Agent"]

    TOOL --> PROMPT
    PROMPT --> REPL
    REPL --> RESULT

    style PROMPT fill:#7c3a00,stroke:#5c2a00,color:#ffe0b0
    style REPL fill:#0f3460,stroke:#1a1a2e,color:#e0e0e0
```

> RLM 使用**完全独立**的 prompt，不继承父级 system prompt。模型只能通过 `llm_query` / `rlm_query` / `FINAL` 与 REPL 交互。

---

## 7. Zagens (Desktop) 身份切换

```mermaid
flowchart TD
    subgraph DESKTOP["Zagens Tauri App"]
        SIDECAR["spawn_sidecar()<br/>crates/desktop/src/sidecar.rs"]
        ENV["env: DEEPSEEK_CLIENT_SURFACE=zagens"]
        SERVE["deepseek-runtime-server<br/>HTTP sidecar"]
    end

    subgraph RUNTIME["Runtime (deepseek-runtime-server)"]
        DETECT["client_identity_line_from_env()<br/>prompts.rs"]
        SWITCH{"surface == zagens<br/>or ds-pick (legacy)?"}
        ID_ZAGENS["CLIENT_IDENTITY_DS_PICK 文案<br/>'assisting inside Zagens...'"]
        ID_TUI["CLIENT_IDENTITY_TERMINAL<br/>'You are DeepSeek TUI...'"]
        ENV_BLOCK["render_environment_block()<br/>+ ui_shell: Zagens (desktop)"]
    end

    subgraph WEB["Desktop Web UI"]
        PANEL["get_system_settings / save_system_settings<br/>commands.rs"]
        SYNC["双轨写入 config.toml<br/>model / effort / policy / memory / lsp ..."]
    end

    SIDECAR --> ENV
    ENV --> SERVE
    SERVE --> DETECT
    DETECT --> SWITCH
    SWITCH -->|yes| ID_ZAGENS
    SWITCH -->|no| ID_TUI
    ID_ZAGENS --> ENV_BLOCK
    ID_TUI --> ENV_BLOCK

    PANEL --> SYNC
    SYNC -.-> SIDECAR

    style DESKTOP fill:#114d1a,stroke:#0a3a10,color:#e0e0e0
    style RUNTIME fill:#0f3460,stroke:#1a1a2e,color:#e0e0e0
    style WEB fill:#7c3a00,stroke:#5c2a00,color:#ffe0b0
```

---

## 8. 核心文件与职责

```mermaid
flowchart LR
    subgraph PROMPT["crates/runtime-server · prompt 组装"]
        P1["src/prompts.rs<br/>主入口 · 分层 compose"]
        P2["src/prompts/base.md · base-office.md"]
        P2b["src/prompts/tasks/<br/>code · office overlay"]
        P3["src/prompts/personalities/<br/>calm / playful"]
        P4["src/prompts/modes/<br/>agent / plan / yolo"]
        P5["src/prompts/approvals/<br/>auto / suggest / never"]
        P6["src/prompts/compact.md<br/>handoff 模板"]
        P7["src/prompts/audit_rules/<br/>Auditor 语言规则（源文件）"]
    end

    subgraph CTX["上下文注入"]
        C1["project_context.rs<br/>AGENTS.md 加载"]
        C2["memory.rs<br/>user_memory block"]
        C3["skills/mod.rs<br/>skills catalogue"]
        C4b["topic_memory.rs<br/>topic graph block"]
    end

    subgraph ENGINE["core/engine/"]
        E1["build.rs · cycle_hooks.rs<br/>refresh / merge / hash"]
        E2["turn_loop/<br/>turn_meta · API 请求"]
        E3["compaction/ + compaction_ops<br/>compact / merge / should"]
    end

    subgraph API["API 序列化"]
        A1["client/chat.rs<br/>build_chat_messages → JSON"]
        A2["models.rs<br/>SystemPrompt / Message / ContentBlock"]
    end

    subgraph CHILD["子 Agent"]
        C4["tools/subagent/mod.rs<br/>8 种 type + subagent_output_format.md"]
        C5["rlm/prompt.rs<br/>RLM 独立 prompt"]
    end

    subgraph DESK["crates/desktop"]
        D1["src/sidecar.rs<br/>DEEPSEEK_CLIENT_SURFACE=zagens"]
        D2["src/commands.rs<br/>SystemSettings 双轨"]
    end

    PROMPT --> CTX
    CTX --> ENGINE
    ENGINE --> API
    ENGINE --> CHILD
    DESK -.->|env| PROMPT

    style PROMPT fill:#0f3460,stroke:#1a1a2e,color:#e0e0e0
    style CTX fill:#114d1a,stroke:#0a3a10,color:#e0e0e0
    style ENGINE fill:#7c3a00,stroke:#5c2a00,color:#ffe0b0
    style API fill:#0f3460,stroke:#1a1a2e,color:#e0e0e0
    style CHILD fill:#114d1a,stroke:#0a3a10,color:#e0e0e0
    style DESK fill:#7c3a00,stroke:#5c2a00,color:#ffe0b0
```

---

## 9. 关键设计决策

1. **KV Cache 优化** — 系统 prompt 按"compile-time → workspace-static → session-volatile"分层排列，最大化 DeepSeek prefix cache 命中率
2. **Working Set 外置** — 工作集摘要注入 user message (`<turn_meta>`) 而非 system prompt，避免 per-turn 写入破坏 prefix cache
3. **Hash 去重** — `system_prompt_hash()` 避免相同 prompt 重复设置（减少不必要的 `SessionUpdated` 事件）
4. **Compile-time 嵌入** — base / personality / mode / approval / compact 在编译时 `include_str!`，启动零 IO
5. **配置驱动的分层注入** — Project context、instructions、user memory、skills 各自独立模块，通过 `PromptSessionContext` 传入
6. **Client identity env-driven** — `DEEPSEEK_CLIENT_SURFACE`（Zagens：`zagens`；遗留别名 `ds-pick`）控制终端 vs 桌面身份与 `ui_shell`，无需重新编译
7. **Sub-agent 独立 session** — 子 agent 获得全新 Engine session，带独立 system prompt 和工具集，通过 `<deepseek:subagent.done>` sentinel 报告结果
8. **RLM 完全独立** — RLM 有自己的 prompt (REPL 模式)，不使用父级 system prompt
