# Audit Scratchpad — 试跑记录（Phase A–C）

> **设计：** [audit-scratchpad-design.md](audit-scratchpad-design.md)  
> **环境：** Zagens 新包（含 Phase A/B/C：工具、横条、覆盖率门禁、Auditor←scratchpad、blackboard 镜像）  
> **日期：** 2026-05-19 · **新包回归：** 见 [§8](#8-新包回归测试方案phase-abc)  
> **维护：** 每完成一步回归，在 [§8.4](#84-回归总评填写) 填结论，并在对应 **R* 记录** 小节补要点（你发结果 → 更新文档）。

---

## 总评

| 用例 | 结论 | run_id |
|------|------|--------|
| 冒烟 — `crates/tui/src/skills` | ✅ 通过 | `2026-05-19-skills-review` |
| 续审 — 同 scratchpad | ✅ 通过 | 同上 |
| 多区 — `crates/tui/src/`（14 area，252 `.rs`） | ✅ 通过 | `2026-05-19-tui-src-review` |
| **Phase B 冒烟** — `scratchpad_*` 工具 + 横条 | ✅ 通过 | `2026-05-19-phase-b-smoke` |
| **Phase B 门禁** — `valid_area_ids` / `require_min_notes` | ✅ 通过 | 同上 |
| **Phase B 续审** — `status` + `list_notes` | ✅ 通过 | 同上 |
| **Phase B 合成** — 6 active findings + `supersedes` | ✅ 通过 | 同上 |
| **回归 R1** — 新包新会话 B 冒烟 + 横条 | ✅ 通过 | `2026-05-19-regression-new` |
| **回归 R2** — 门禁 + C1 deferred | ⚠️ 部分 | 同上；Step2 ✅ · Step3 ⏭ · C1 4b 见 [R2 记录](#r2-记录2026-05-19新包--同-run-续测) |
| **回归 R3** — 续审 `status` + `list_notes` | ✅ 通过 | `2026-05-19-phase-b-smoke` |
| **回归 R4a** — C1 未审完 synthesize | ⚠️ 部分 | `2026-05-19-regression-coverage` |
| **回归 R5** — C2 Auditor + C3 blackboard | ✅ 通过 | `2026-05-19-regression-new` · `regression-audit-001` |
| **L7 全仓** — `audit-001`（defer 回归） | ⚠️ 已修复待复测 | `2026-05-19-audit-001` |
| **L7 全仓** — `full-audit`（新包 + 全仓 Prompt） | ⚠️ 报告可用，流程未闭环 | `2026-05-20-full-audit` |
| **L7c 全仓** — `2026-05-20-audit`（宽 Prompt + scratchpad） | ✅ 流程闭环；⚠️ 主代理路径 | `2026-05-20-audit` |
| **L7d 全仓** — `2026-05-20-001`（子代理并行 + 14 HIGH） | ✅ 流程闭环；✅ 真子代理 | `2026-05-20-001` |
| **L8 Phase D** — 审计过程可视化 | D1 ✅ / D2 ✅ / U2 ✅ / U3 ⬜ | 见 [§L8](#l8--phase-d-审计过程可视化规划) |
| **L9 地狱级四维** — 审查维度 Prompt | ⏸ 暂缓 | 见 [§L9](#l9--地狱级四维审计暂缓) |

**结论：** Phase A 在单区与多区场景下可用；长程「落盘纪律」已验证。Phase B 在 `2026-05-19-phase-b-smoke` 上验收通过。**新包回归**：R1 ✅ · R2 ⚠️ · R3 ✅ · R4a ⚠️ · R4b ⏭ · **R5 ✅**（Auditor 预期 **FAIL** = 检测生效）；R6 可选见 §8.4。**L7 全仓**：`audit-001` defer 已修待复测；`full-audit` 见 [L7b](#l7b--全仓试跑-2026-05-20-full-audit2026-05-20)；**L7c** 见 [§L7c](#l7c--全仓试跑-2026-05-20-audit2026-05-20)。**下一步产品：** [Phase D 可视化](#l8--phase-d-审计过程可视化规划)（design [§6.13](audit-scratchpad-design.md#613-phase-d--审计过程可视化路线图-未实现)）。

---

## 0. 前置条件（试跑前）

```powershell
Test-Path "$env:USERPROFILE\.deepseek\skills\audit-repo\SKILL.md"   # True
Get-Content "$env:USERPROFILE\.deepseek\skills\.system-installed-version"  # 期望 ≥ 3
```

工作区：仓库根 `F:\DeepSeek-TUI-desktop`（或等价路径）。TaskType：**Code**，运行模式：**Agent**（非 Plan）。

---

## 1. 冒烟测试

### 1.1 用户 Prompt

```text
请加载技能 audit-repo，对 crates/tui/src/skills 做代码级审查（不是全仓）。
必须使用 scratchpad：inventory.json + notes.jsonl。
完成后告诉我 run_id 和 scratchpad 路径。
```

### 1.2 产物路径

```text
<workspace>/.deepseek/scratchpad/2026-05-19-skills-review/
├── inventory.json
└── notes.jsonl
```

示例（本仓库）：`F:\DeepSeek-TUI-desktop\.deepseek\scratchpad\2026-05-19-skills-review\`

### 1.3 `inventory.json`（结束时）

```json
{
  "run_id": "2026-05-19-skills-review",
  "created_at": "2026-05-19T03:00:00Z",
  "scope": "crates/tui/src/skills",
  "areas": [
    {
      "id": "area-skills",
      "path": "crates/tui/src/skills",
      "status": "done",
      "notes": "3 files: mod.rs, install.rs, system.rs. 6 findings (5 LOW, 1 INFO), 3 cleared, 1 meta."
    }
  ]
}
```

### 1.4 `notes.jsonl` 摘要

| id | kind | severity | 要点 |
|----|------|----------|------|
| note-001 | meta | — | inventory_version 1 |
| note-002 | finding | LOW | `resolve_skills_dir` doc 与 `render_skills_block` 错位 |
| note-003 | finding | LOW | `default_skills_dir` 兜底 `/tmp/...`（Windows） |
| note-004 | finding | LOW | `parse_skill` 内联编译 regex |
| note-005 | finding | LOW | `uninstall()` 删除前未校验目录内容 |
| note-006 | cleared | — | `system.rs` `include_str!` 路径有效 |
| note-007 | cleared | — | `install.rs` tarball / 网络策略 |
| note-008 | cleared | — | `SkillRegistry::discover` 深度与 symlink |
| note-009 | finding | INFO | doc 中 “Codex” 竞品用语 |
| note-010 | finding | LOW | `trusted_skill_roots` 启动时快照 |

共 **10 行** JSONL；每行含 `area_id: "area-skills"`；finding 均为 `status: verified`。

### 1.5 行为观察

| 项 | 结果 |
|----|------|
| 先建 scratchpad 再读源码 | ✅ |
| 单 area、3 文件粒度 | ✅ |
| 检查完成后写入 notes + `done` | ✅ |
| JSONL 曾误拼单行，代理自我修正 | ✅（说明规则被读到） |
| 报告仅 LOW/INFO，无 HIGH/BLOCKER | ✅ → 未 spawn Auditor（符合 `base.md`） |
| `run_id` + 路径回告用户 | ✅ |

### 1.6 误报与纠正

推理阶段曾称根路径 `assets/skills/**/SKILL.md` glob 为零、`include_str!` 可能错误。  
最终 **note-006** 核实资产在 `crates/tui/assets/skills/`，路径正确——未将误报写入 HIGH。

### 1.7 冒烟结论

**通过。** 满足 [设计 doc §4](audit-scratchpad-design.md) Phase A 验收项。

---

## 2. 续审测试

### 2.1 用户 Prompt

```text
续审之前的 audit scratchpad。先读已有 inventory.json，从第一个 pending/in_progress 的 area 继续；
notes 只读该 area_id 的行，不要读 notes 文件尾部。run_id 是：2026-05-19-skills-review
```

### 2.2 代理行为

| 项 | 结果 |
|----|------|
| 读取 `inventory.json` | ✅ |
| 发现全部 `done`，无 pending/in_progress | ✅（与磁盘一致） |
| 未重建 inventory、未重读三份源文件 | ✅（约 6 次工具调用） |
| 汇总 notes 覆盖与 finding 计数 | ✅ |
| 提供后续选项（合成报告 / 扩 scope / 可选 P3） | ✅ 合理 |

### 2.3 续审结论

**通过。** 符合 §4.4「先 inventory 指针，再按 area 理解 notes，不依赖文件尾部」。

---

## 3. 多区试跑（`crates/tui/src/`）

### 3.1 用户 Prompt

见 [audit-scratchpad-design.md §15](audit-scratchpad-design.md#15-多区试跑-prompt) 或会话记录（14 area、`audit-repo`、scratchpad 强制）。

### 3.2 产物

```text
.deepseek/scratchpad/2026-05-19-tui-src-review/
├── inventory.json   # 14 areas, 全部 done
└── notes.jsonl      # 36 行
```

### 3.3 `inventory.json` 摘要

- **scope：** `crates/tui/src/`（252 `.rs`）
- **areas：** 14（`area-core` … `area-root-agent`），**0 deferred**
- **粒度：** 按功能分组（core / tools 三组 / tui 四组 / commands / security / modules / root 三组），符合「一级子目录级」意图

### 3.4 `notes.jsonl` 按 `area_id` 统计

| area_id | 条数 | 备注 |
|---------|------|------|
| area-core | 8 | 含 2× LOW finding + 多条 cleared |
| area-tools-io | 3 | |
| area-tools-ext | 6 | |
| area-tools-meta | 5 | |
| area-tui-core … area-root-agent（10 区） | 各 1 | 多为单条 `cleared`（加速收口） |
| `_global` | 1 | meta |

**合计 36 行。** 14/14 area 至少有 1 条带该 `area_id` 的 note → **无「done 但零 notes」**。

### 3.5 报告 vs scratchpad

| 项 | 结果 |
|----|------|
| LOW finding（notes） | 3：`lsp_hooks.rs:23`、`tool_catalog.rs:36`、`shell.rs:659` |
| 报告 LOW | 3 项对应 + 1 段「路径安全总评」（非独立 finding） |
| HIGH/BLOCKER | 0 → 未 spawn Auditor ✅ |
| `supersedes` | 未使用 |
| 工具调用量级 | ~38（与 14 区审查匹配） |

### 3.6 行为观察

| 项 | 结果 |
|----|------|
| P0 先 inventory 再读码 | ✅ |
| 14 area 顺序推进 + checklist 同步 | ✅ |
| 后期仍写 notes（非停工） | ✅；但后 10 区多为 1 条 cleared，**深度前高后低** |
| 曾想 spawn explore，无子代理后改自审 | ✅，未伪造子代理结论 |
| Schema 严格度 | ⚠️ 部分行缺 `area_id` / `status:verified` / `id`（见 §3.7） |

### 3.7 多区结论与 Phase B 输入

**Phase A 多区：通过。** 未出现「后半程不写 scratchpad」——Phase B **不必**以「偷懒提醒」为最高优先级。

| 试跑信号 | Phase B 含义 |
|----------|----------------|
| 每区 ≥1 条 notes，但后段过粗 | 加 `scratchpad_status` + 可选「每区最少 finding/cleared 质量」prompt；**不做**硬性强提醒 |
| 36 行 notes、3 LOW → P2 未爆 context | 分层注入仍要做（全库 20–40 区 × 更多 finding 时确定会需要），可随 B 一并落地 |
| JSON schema 不严 | **`scratchpad_append`** 值得做（校验 + 自动 `id`） |
| 100% done | 覆盖率硬门禁延到 Phase C，待更多「半途放弃」样本 |

---

## 4. 未覆盖项（留待 Phase C）

实现方案见 [audit-scratchpad-design.md §6.12](audit-scratchpad-design.md#612-phase-c--与-craft--auditor-深集成-排队)（C0→C4）。

| 项 | Phase C 子阶段 | 说明 |
|----|----------------|------|
| 全仓 20～40 area（含 desktop 等） | — | B 分层注入已具备；可选压测（原测试 5） |
| `supersedes` 升级路径 | C2 | Phase B smoke 已手动验证；C2 纳入 Auditor 闭环 |
| 覆盖率硬门禁 | **C1** | `accounted_ratio` / `reviewed_ratio`，§6.12.4 |
| Auditor+scratchpad 深绑 | **C2** | 结构化 `note_id` 输入，§6.12.5 |
| compaction 丢 scratchpad 指针 | **C0** | compaction pin + L0，§6.12.3 |
| B4 只读提醒压测 | — | 可选（原测试 6） |

---

## 5. Phase B 试跑（2026-05-19）

**run_id：** `2026-05-19-phase-b-smoke` · **范围：** `crates/tui/src/skills`（1 area + 门禁用 `area-gate-test`）

| 测试 | 要点 | 结果 |
|------|------|------|
| 1 冒烟 | 四工具调用；横条 1/1；`note-008`/`009` supersede 行号 | ✅ |
| 2 门禁 | 非法 `area_id` → `valid_area_ids`；0 notes 不能 `done` | ✅ |
| 3 续审 | `scratchpad_status` + `list_notes(area-skills)`，未读 jsonl 全文 | ✅ |
| 4 合成 | 报告 6 条 active finding；prompt 含 synthesize/报告 | ✅ |

**未跑（可选）：** 测试 5 多区横条进度；测试 6 B4 `remind_after_readonly_tools`。

---

## 6. Phase B/C 决策摘要

**Phase B（已实施）：** 见 [design §6](audit-scratchpad-design.md) — `scratchpad_*` 工具、B2 线程绑定、B3/B3b 注入与 handoff、B4 提醒、B5 API + 横条、B7 TTL；`supersedes` 传递闭包、每轮单次 `<scratchpad_summary>`。

**Phase C（C0–C3 已编码，待试跑）：** [design §6.12](audit-scratchpad-design.md#612-phase-c--与-craft--auditor-深集成-排队) — compact、coverage gate、Auditor track A/B、blackboard 镜像。

### Phase C 试跑要点（复制用）

| 项 | 操作 | 期望 |
|----|------|------|
| C1 覆盖率 | 未审完时「写审查报告」 | `<scratchpad_summary>` 含 WARNING 或 BLOCKED |
| C1 deferred | `set_area(deferred)` 无 meta | reject |
| C2 Auditor | `agent_spawn(type=auditor, scratchpad_run_id=…)` | assignment 含 Track A 表 + Track B prose |
| C3 blackboard | 同上且带 `task_id` | `.deepseek/blackboards/{task_id}.json` 含 `scratchpad` 分区 |

---

## 7. 复现用 Prompt 清单

见 [audit-scratchpad-design.md §12](audit-scratchpad-design.md#12-附录phase-a-skill-片段可直接粘贴试跑)、§15（多区）。

### 多区主 Prompt（复制用）

```text
请加载技能 audit-repo。

对 crates/tui/src/ 做代码级审查（仅该目录树，不含 crates/desktop、不含全仓其它路径）。
必须使用 audit scratchpad：.deepseek/scratchpad/{run_id}/inventory.json 与 notes.jsonl。

要求：
1. run_id 自定（建议可读 slug，如 2026-05-19-tui-src-review），完成后告知完整 scratchpad 路径。
2. P0：inventory.json 约 10～15 个 area，粒度为 crate 下的一级子目录（如 core、tools、tui、llm 等），每 area 稳定 id（area-*），不要用「整个 tui/src 一行」。
3. P1：按 area 顺序审查；区内可批量 read_file/grep；每个 area「检查完成」后至少 append 1 条 notes.jsonl（含 area_id），并把该行标为 done 或 deferred（deferred 须写明原因）。
4. 完成当前 in_progress 的 area 后再开始下一 area（软规则，不要长期漂移到别区却不落盘）。
5. P2：全部 area 为 done/deferred 后，仅根据 notes 中 kind=finding 且 status=verified、且未被 supersedes 取代的条目写报告；不要复述 reasoning。
6. 若无 HIGH/BLOCKER，可不 spawn Auditor；若有 HIGH/BLOCKER，按 base.md 走 Auditor。

若一轮做不完，说明进度、已完成的 area_id 列表，并保留 scratchpad 供续审；不要删除已有 inventory。
```

---

*（新试跑请在「总评」表增行。）*

---

## 8. 新包回归测试方案（Phase A–C）

> **目的：** 新安装的 Zagens + sidecar 一次性验证 A/B/C；区分 **新会话**（干净线程）与 **原会话**（续审 / 历史线程）。  
> **建议耗时：** 必测约 45–60 分钟；含 C0 compact、多区压测可再加 30–60 分钟。  
> **记录：** 每步在下方「回归总评」表填 ✅/❌/⏭ 与备注。

### 8.0 测前检查（5 分钟）

| # | 检查项 | 命令 / 操作 | 期望 |
|---|--------|-------------|------|
| P0 | 完全退出并重启 Zagens | 任务管理器无旧 `deepseek-tui` / Zagens | 新 sidecar 已加载 |
| P1 | 工作区 | 打开 `F:\DeepSeek-TUI-desktop`（仓库根） | 路径正确 |
| P2 | 技能版本 | 见 §0 `Get-Content …system-installed-version` | ≥ 3 |
| P3 | 磁盘上旧 scratchpad（可选） | `Test-Path .deepseek\scratchpad\2026-05-19-phase-b-smoke` | 有则用于 **原会话** 续审 |
| P4 | 横条可见性 | 打开任意曾绑定 scratchpad 的**旧线程**看一眼 Composer 上沿 | 无 run 则无横条属正常 |

```powershell
cd F:\DeepSeek-TUI-desktop
Get-ChildItem .deepseek\scratchpad -Directory | Select-Object Name
```

---

### 8.1 新会话 vs 原会话（怎么选）

| 类型 | 含义 | 在 Zagens 里怎么做 | 验证什么 |
|------|------|---------------------|----------|
| **新会话** | 全新线程，无历史消息 | 侧边栏 **新建对话**（或 Ctrl+N） | 工具注册、横条绑定、C1 门禁、首次 `scratchpad_run_id` 写入 |
| **原会话** | 已有线程 + 磁盘上已有 scratchpad | **打开旧对话**（试跑时的线程），或新建后手动指定同一 `run_id` | 续审、`scratchpad_status` 与磁盘一致、不丢 run、C0 compact 后还能续 |

**原则：**

- **新会话** 用新 `run_id`（如 `2026-05-19-regression-new`），避免与试跑数据混淆。  
- **原会话** 优先用已有 `2026-05-19-phase-b-smoke`（若目录还在）；若无，用 `2026-05-19-tui-src-review` 做多区续审。  
- 每条用例开头注明 **【新会话】** 或 **【原会话】**。

---

### 8.2 必测路径（推荐顺序）

#### R1 — 【新会话】Phase B 冒烟（工具 + 横条）≈15 min

**run_id：** `2026-05-19-regression-new`

```text
请加载技能 audit-repo。

对 crates/tui/src/skills 做代码级审查（仅该目录，不是全仓）。
必须使用 scratchpad 工具：scratchpad_status、scratchpad_append、scratchpad_list_notes、scratchpad_set_area。
不要用 write_file 写 notes.jsonl（除非工具不可用）。

run_id：2026-05-19-regression-new

完成后告诉我：run_id、scratchpad 路径，以及是否调用了上述工具。
```

| 通过标准 |
|----------|
| 工具面板出现四个 `scratchpad_*` |
| 琥珀色横条：进度、run 路径、`verified` 计数合理 |
| 磁盘：`.deepseek/scratchpad/2026-05-19-regression-new/` 含 `inventory.json`、`notes.jsonl` |

#### R1 记录（2026-05-19，新包 · 新会话）

| 项 | 结果 |
|----|------|
| **会话** | 新会话 |
| **run_id** | `2026-05-19-regression-new` |
| **路径** | `.deepseek/scratchpad/2026-05-19-regression-new/` |
| **结论** | ✅ 通过 |

**工具调用：** `scratchpad_append` ×8 · `scratchpad_list_notes` ×1 · `scratchpad_set_area` ×1 · `scratchpad_status` ×2（第 3 次 status 被限流，不影响 `areas_done: 1`）· `write_file` ×1（仅建 `inventory.json`，无 create-inventory 工具，属预期）

**产物：** 1 area `area-tui-skills` · `done` · notes 9 行（1 meta + 3 finding LOW + 5 cleared）

**发现摘要：** note-002/003/004（LOW）— Unix 隐藏目录检测、PathBuf 去重、技能名允许前导 `.`；无 HIGH/MEDIUM；未触发 Auditor。

**备注：** 横条若显示 1/1 即与磁盘一致；`notes.jsonl` 未用 `write_file`。

---

#### R2 — 【新会话】B 门禁 + C1 deferred ≈10 min

**run_id：** 仍用 `2026-05-19-regression-new`（同线程或新会话均可，同一 run_id）

```text
对 scratchpad run_id 2026-05-19-regression-new：

1. scratchpad_status，摘要 JSON。
2. scratchpad_append area_id="area-nope" 的 cleared → 应 reject 且带 valid_area_ids。
3. 若需测 require_min_notes：对无 notes 的 area 先 set_area(done) 应失败，append 后再 done 应成功。
4. 【C1】对某 area：先 append cleared，再 scratchpad_set_area(deferred) → 应失败；再 append 一条 kind=meta 说明原因，再 deferred → 应成功。
```

#### R2 记录（2026-05-19，新包 · 同 run 续测）

| 项 | 结果 |
|----|------|
| **会话** | 新会话（同 `run_id` 续测） |
| **run_id** | `2026-05-19-regression-new` |
| **初始快照** | `areas_total: 1` · `done: 1` · `notes_total: 9` · `findings_verified: 3` |
| **终态** | `done: 0` · `deferred: 1` · `notes_total: 11`（+note-010 cleared、+note-011 meta） |
| **总判** | ⚠️ **部分通过** |

| Step | 内容 | 判定 | 说明 |
|------|------|------|------|
| 1 | `scratchpad_status` | ✅ | 完整 areas / notes / findings 计数 |
| 2 | `append(area-nope, cleared)` | ✅ | reject + `valid_area_ids: ["area-tui-skills"]` |
| 3 | `require_min_notes`（0 notes → done 拒） | ⏭ | 仅 1 area 且已有 9 notes；无 scratchpad 级建区工具，未构造 |
| 4a | `append(cleared)` → note-010 | ✅ | |
| 4b | `set_area(deferred)` 紧接 cleared | ❌ *用例* | **预期失败、实际成功** — R1 已存在 `kind=meta`，`require_deferred_meta` 按「区内任意 meta」判定（[design §6.12.4](audit-scratchpad-design.md#612-phase-c--覆盖率--auditor--blackboard) / `area_meets_deferred_quality`），非「cleared 之后须再 meta」 |
| 4c | `append(meta)` → note-011 | ✅ | |
| 4d | `set_area(deferred)` | ✅ | 区进入 `deferred` |

**归因（4b）：** 与实现一致、与 **本步 Prompt 顺序假设** 不一致。要在同 run 上复测 4b，需 **无 meta 的新 area**（或新 `run_id` + 仅 cleared 的区）。**待决：** 产品是否要把 deferred 硬化为「须在本轮 defer 前新增 meta」（实现变更）— 当前设计/代码为「区内 ≥1 条 meta」。

**备注：** Step3 可在 `2026-05-19-regression-coverage`（R4a 三 area）或手写 `inventory.json` 第二 area 时补测。

---

#### R3 — 【原会话】续审 + list_notes ≈5 min

**run_id：** `2026-05-19-phase-b-smoke`（或你磁盘上任意完整 smoke run）

**操作：** 打开**当时试跑用的旧对话**（原会话）；若无，新会话 + 下列 prompt。

```text
续审 audit scratchpad，run_id：2026-05-19-phase-b-smoke。
先 scratchpad_status，再用 scratchpad_list_notes(area_id=area-skills) 拉该区笔记。
不要 read_file notes.jsonl 全文。
```

| 通过标准 |
|----------|
| `findings_verified` 与上次一致（约 6） |
| 含 `note-008`/`009`，不含被取代的 `note-002`/`003` |
| 横条仍指向同一 run_id |

#### R3 记录（2026-05-19，新包 · 续审 `phase-b-smoke`）

| 项 | 结果 |
|----|------|
| **run_id** | `2026-05-19-phase-b-smoke` |
| **工具** | `scratchpad_status` + `scratchpad_list_notes(area_id=area-skills)`；未 `read_file` 全文 `notes.jsonl` |
| **结论** | ✅ 通过 |

**快照：** `areas_total: 2` · `done: 2` · `deferred: 0` · `notes_total: 10` · `findings_verified: 6` · `findings_open: 0`（`area-skills` 9 条 · `area-gate-test` 1 条，后者未 `list_notes`）

**有效 findings（6，去 supersede 后）：**

| id | 严重度 | 要点 |
|----|--------|------|
| note-008 | MEDIUM | `install.rs` 1104–1125 — 重定向 host 未校验；← 取代 note-002 |
| note-009 | MEDIUM | `install.rs` 1449–1493 — `description` 安装器必填 vs `parse_skill` 可选；← 取代 note-003 |
| note-004–007 | LOW | `mod.rs` — Regex 重复编译、深度静默截断、`trusted_skill_roots` filter、warnings 截断无提示 |

**supersede：** note-002→008、note-003→009；新行追加末尾、未改写旧行。note-001 为 meta。

**通过标准核对：** `findings_verified=6` ✅ · 含 008/009、活跃列表不含被取代的 002/003 ✅ · 横条/run 绑定未在粘贴中截图，工具侧 run_id 一致 ✅

**可选补测：** `list_notes(area-gate-test)`（1 条 note，门禁区）

---

#### R4 — 【原会话或新会话】P2 合成 + C1 覆盖率 ≈10 min

**4a — 未审完应被拦（C1）**  
**【新会话】** 新 run_id：`2026-05-19-regression-coverage`

```text
请加载 audit-repo。审查 crates/tui/src/skills，run_id：2026-05-19-regression-coverage。
inventory 设 3 个 area，只完成第 1 个 area 就停止。
然后：根据 scratchpad 写审查报告草稿（synthesize）。
```

| 通过标准 |
|----------|
| 出现 `<scratchpad_summary>` 且含 **WARNING** 或 **BLOCKED** |
| **不应**出现完整 L1 逐条 finding 列表（hard/soft 拦） |

**4b — 审完后可合成**  
对 **R1** 的 `2026-05-19-regression-new`（全部 area done/deferred 后）：

```text
根据 scratchpad 2026-05-19-regression-new 写审查报告草稿（synthesize）。
只使用 kind=finding、status=verified、且未被 supersedes 取代的条目。
```

| 通过标准 |
|----------|
| 报告条数与 `findings_verified` 一致 |
| 同轮仅一条 `<scratchpad_summary>`（不重复注入） |

#### R4 记录（2026-05-19，新包）

##### R4a — `2026-05-19-regression-coverage`（1/3 area 后 synthesize）

| 项 | 结果 |
|----|------|
| **会话** | 新会话 |
| **run_id** | `2026-05-19-regression-coverage` |
| **范围** | `crates/tui/src/skills` — 3 文件 → 3 area（mod / install / system） |
| **完成度** | **1/3** — 仅 `area-skills-mod` → `done`（1098 行）；`install` / `system` pending |
| **结论** | ⚠️ **部分通过** |

**审查产物：** 6 条 `findings_verified`（note-002～007，均来自 mod.rs）：1×MEDIUM（note-002 跨模块 description 一致性，待 install 确认）+ 5×LOW；无 supersede。`resume_area_id`: `area-skills-install`。

**synthesize 草稿（模型输出）：** 显式写 **1/3 覆盖**、inventory 表、**已知缺口**（两 pending area 风险提纲）、后续步骤 — 覆盖率语义 ✅。finding 表仅已审区，非全 run L1 倾倒 ✅。

**对照 C1 注入（严格项）：**

| 通过标准 | 判定 | 说明 |
|----------|------|------|
| `<scratchpad_summary>` 含 WARNING/BLOCKED | ⚠️ 未确认 | 用户粘贴**未含**该标签；按默认 config（`accounted_ratio` 33% &lt; hard 60%）引擎应走 **BLOCKED** 分支（见 `coverage_gate` / `build_report_summary_message`）— 建议在 Zagens 该轮消息流中搜 `scratchpad_summary` 或 `BLOCKED:` |
| 不应注入完整 L1 逐条列表 | ⚠️ 待对照 | 若仍为 Allow 路径则异常；若已 BLOCKED 但模型仍手写 6 条表，属模型未守门禁语义 |

**备注：** 本 run 顺带满足 R2 补测场景（3 area、第 1 区 done 后另两区 0 notes）— 未单独跑 `require_min_notes` / 干净区 C1 4b。

##### R4b — `2026-05-19-regression-new`（全 area 后 synthesize）

| 项 | 结果 |
|----|------|
| **状态** | ⏭ **未测**（本次仅提交 R4a） |

**建议 4b prompt：** 对 R1 run（当前 1 area `deferred`、3 LOW verified）执行 synthesize；预期 `findings_verified=3`（或去 supersede 后条数），且 `accounted_ratio` 达 soft 门后注入 Allow/Warn 摘要。

---

#### R5 — 【新会话】C2 Auditor + C3 blackboard ≈15 min

**前提：** R1 或 `2026-05-19-phase-b-smoke` 上至少有 1 条 HIGH/MEDIUM verified；若无，跳过 Auditor 或先用 R1 跑完。

**【新会话】**（推荐，避免旧对话干扰）

```text
请加载 audit-repo。

对 scratchpad run_id 2026-05-19-phase-b-smoke（或 2026-05-19-regression-new，二选一写清楚）
写一段简短的审查报告 prose 草稿（含 HIGH/MEDIUM 条目，可故意写一条不在 scratchpad 里的假 finding 用于测 track B）。

然后：
agent_spawn(
  type=auditor,
  scratchpad_run_id=<同上 run_id>,
  task_id=regression-audit-001,
  prompt=<粘贴上面的 prose 草稿>
)

完成后说明 Auditor 是否 PASS/FAIL，以及是否看到 Track A 表格。
```

| 通过标准 |
|----------|
| Auditor assignment 含 **Track A**（`note_id` 表）与 **Track B**（prose 块） |
| 假 finding → **UNVERIFIED_CLAIM** 或 FAIL |
| 磁盘：`.deepseek/blackboards/regression-audit-001.json` 含 **`scratchpad`** 分区（`run_id`、`high_note_ids` 等） |

#### R5 记录（2026-05-19，新包 · 新会话）

| 项 | 结果 |
|----|------|
| **run_id** | `2026-05-19-regression-new`（1 area `deferred` · 4×LOW verified + cleared/meta） |
| **spawn** | `agent_spawn(type=auditor, scratchpad_run_id=…, task_id=regression-audit-001)` → `agent_70d786bf` |
| **回归判定** | ✅ **通过**（机制验收；见下「Auditor 判定」） |
| **Auditor 判定** | **FAIL**（**预期** — 草稿故意 1 假 finding + 3 条严重性拔高） |

**草稿设计：** 1×HIGH + 2×MEDIUM（真实条对应 scratchpad LOW）+ 1×**Track B 靶**（`install.rs:362` tarball 符号链接 — **不在** notes）。

**Track A（scratchpad 交叉核对）：**

| # | 草稿断言 | scratchpad | Auditor |
|---|----------|------------|---------|
| 1 | mod.rs:158 HIGH 隐藏目录 | note-002 LOW | ✅ 命中 · **severity MISMATCH** → FAIL |
| 2 | install.rs:362 符号链接 | ❌ 不存在 | Track B（见下） |
| 3 | install.rs:1484 MEDIUM leading dot | note-004 LOW | ✅ 命中 · severity MISMATCH → FAIL |
| 4 | mod.rs:376 MEDIUM PathBuf 去重 | note-003 LOW | ✅ 命中 · severity MISMATCH → FAIL |

结构化 **DETAIL + SUMMARY** 逐条核对（功能等同 Track A 表）。

**Track B：** ✅ 假 finding #2 标为 **ABSENT**；并做代码反证（行号应为 ~1306；note-006 cleared；`is_symlink()` ~1352）。

**通过标准核对：** Track A/B 均生效 ✅ · 假 finding 导致 FAIL ✅ · blackboard `scratchpad` 分区 — **未在本次粘贴中列路径**（Sentinel 与 `agent_result` 一致；可 `ls ~/.deepseek/blackboards/regression-audit-001.json` 补截图）

**备注：** 选用 `regression-new` 而非 `phase-b-smoke`（后者有 MEDIUM 008/009）；本 run 以 **LOW + 故意拔高** 测 severity mismatch，仍满足 R5 目的。

---

#### R6 — 【原会话】C0 Compaction（可选）≈20+ min

**【原会话】** 在已绑定 scratchpad 的长对话里继续聊，或主动触发 compact（若产品支持 `/compact` 或自动 compact）。

| 通过标准 |
|----------|
| compact 后仍能 `scratchpad_status`，`run_id` 不变 |
| system / compaction 摘要中出现 **`[scratchpad L0]`** 单行（无大段 L1 finding 列表） |
| 续审 prompt（同 R3）仍可用 |

⏭ 时间紧可标「未测」，不阻塞发版。

---

### 8.3 可选路径

| ID | 类型 | 内容 |
|----|------|------|
| O1 | 新会话 | 多区 `crates/tui/src/`（§7 多区主 Prompt），横条 `done/total` 随进度变 |
| O2 | 新会话 | B4：config `remind_after_readonly_tools = 3`，连续 read 不 append → 系统提醒 |
| O3 | 原会话 | 打开 `2026-05-19-tui-src-review` 线程续审第 N 个 pending area |

---

### 8.4 回归总评（填写）

| ID | 会话 | 项 | 结论 | run_id / 备注 |
|----|------|-----|------|----------------|
| R1 | 新 | B 冒烟 + 横条 | ✅ | `2026-05-19-regression-new`；见 [R1 记录](#r1-记录2026-05-19新包--新会话) |
| R2 | 新 | 门禁 + C1 deferred | ⚠️ | `2026-05-19-regression-new`；Step2 ✅ · Step3 ⏭ · 4b 用例❌（R1 遗留 meta）；4c–4d ✅；见 [R2 记录](#r2-记录2026-05-19新包--同-run-续测) |
| R3 | 原 | 续审 list_notes | ✅ | `2026-05-19-phase-b-smoke`；6 verified · 008/009 supersede；见 [R3 记录](#r3-记录2026-05-19新包--续审-phase-b-smoke) |
| R4a | 新 | C1 未审完拦报告 | ⚠️ | `2026-05-19-regression-coverage`；1/3 done · 草稿含缺口语义 ✅ · `scratchpad_summary` BLOCKED 未在粘贴中确认；见 [R4 记录](#r4-记录2026-05-19新包) |
| R4b | 新/原 | P2 合成 | ⏭ | `regression-new` synthesize 未跑 |
| R5 | 新 | C2 Auditor + C3 blackboard | ✅ | `regression-new` · Auditor **FAIL**（预期）· Track A/B ✅ · `task_id=regression-audit-001`；见 [R5 记录](#r5-记录2026-05-19新包--新会话) |
| R6 | 原 | C0 compact（可选） | | |
| O1–O3 | | 可选 | | |

**发版建议：** R1–R4b 全 ✅ 可认为回归通过；R5 有 HIGH 时必测；R6/O* 按时间。

---

## L7 — 全仓试跑 `2026-05-19-audit-001`（2026-05-19）

| 项 | 结果 |
|----|------|
| 外存 / 报告 | ✅ `inventory.json` + `notes.jsonl` + `deliverables/CODE_REVIEW_2026-05-19.md` |
| 首屏无 `scratchpad_*` | ⚠️ **根因：** Agent 下工具 **defer**；模型未 `tool_search` → 手写文件 |
| 横条 | ❌ 未绑 `scratchpad_run_id` |
| inventory 纪律 | ⚠️ 曾 23×`pending` 与报告冲突；后手工改为 19 done + 4 deferred |
| **代码修复（待复测）** | `scratchpad_*` 加入 eager 白名单；`scratchpad_status` 绑线程；`GET …/status` 发现最新 run；skill v4 + `tool_search` 指引 |

**复测 Prompt（新会话，重编 sidecar 后）：**

```text
加载 audit-repo。run_id=2026-05-19-audit-001。
1) 列出工具名里是否含 scratchpad_status（不要猜）。
2) 调用 scratchpad_status(run_id=…) 并贴 JSON。
3) 确认 Zagens 琥珀横条是否出现。
```

---

## L7b — 全仓试跑 `2026-05-20-full-audit`（2026-05-20）

**Prompt（用户）：** 全库代码级审核，输出 MD 报告（与 L7 同类全仓任务）。

**产物：**

```text
.deepseek/scratchpad/2026-05-20-full-audit/
├── inventory.json    # 34 areas
└── notes.jsonl       # 18 行（含 meta + finding）

deliverables/DS_Pick_Audit_2026-05-20.md
```

### 总评：⚠️ 报告可用，流程未闭环

| 维度 | vs `audit-001`（5/19） | 本次 |
|------|------------------------|------|
| P0 清单 / 侧栏 Checklist | 后期才有 | ✅ 34 area，P0/P1 结构化 |
| 琥珀横条 | ❌ 未绑 run | ✅ 出现（`2026-05-20-full-audit`） |
| `scratchpad_append` | 手写 JSON，无 id/ts | ✅ 18 条带 `id`/`ts` |
| 并行执行体 | 口述「14 子代理」 | ⚠️ 实为 14×**Task**（`task_create`），非 `agent_spawn`；见 §7.1 |
| `scratchpad_set_area` | 事后手改 inventory | ❌ **34×`pending`**，横条 accounted **0/34** |
| P2 `verified` | 部分 verified | ❌ findings 均为 **`open`** |
| 报告 | `CODE_REVIEW_2026-05-19.md` | ✅ `DS_Pick_Audit_2026-05-20.md` |

### 磁盘核实（2026-05-20 复核）

| 项 | 结果 |
|----|------|
| `inventory.json` | 34 area，**全部 `pending`**（无 `in_progress` / `done` / `deferred`） |
| `notes.jsonl` | **18 行**；MEDIUM 在 notes 中 ≥6 条（CSP、main、shell、subagent、file、install 等） |
| 报告汇总表 | **3 MEDIUM**（M1 CSP `unsafe-inline`、M2 `devtools: true`、M3 `main.rs` 体量） |
| M1/M2 行号 | ✅ `tauri.conf.json:35` / `:31` 与仓库一致 |
| M3 `main.rs` 行数 | ⚠️ 报告写 5,333；仓库约 **4,909**（仍远超 1k 软上限） |
| 严重度 | 0 HIGH/BLOCKER（与 notes 一致） |
| P3 自述验证 | ⚠️ notes 未标 `verified`，与 skill P2 不一致 |

### 行为观察

| 项 | 结果 |
|----|------|
| 先建 scratchpad + 34 area inventory | ✅ |
| 主代理 `scratchpad_append` | ✅ |
| 主代理 `scratchpad_set_area`（进度与清单对齐） | ❌ → 横条 **0/34**（修复后 UI 应显示 `notes 18` +「待 set_area」） |
| 后台 Task 并行 | ✅ Task 多 completed；未 `task_read` → 未回写 scratchpad/inventory |
| 直接读码 + 合成报告 | ✅ 报告可读 |
| 「全库逐文件审完」宣称 | ❌ 未达标（inventory 未交代、无 verified 门禁） |

### 与已交付修复的关系

试跑时用户侧应已含：**eager `scratchpad_*`**、**`GET …/status` 发现 run**、横条 **accounted = done+in_progress+deferred**、**agent SSE → 子代理面板**。本次证明 **append + 横条发现** 有效；**set_area 纪律**与 **verified-only 报告** 仍依赖模型遵守 skill。

### 根因（2026-05-20 复盘，详见 [design §2.1 人机契约](audit-scratchpad-design.md#21-人机契约契约现象) · [§7.1](audit-scratchpad-design.md#71-tasktask_-与-sub-agentagent_两套对象不可混称) · [§14](audit-scratchpad-design.md#14-全仓审计失败模式task-与-sub-agent-混用--未-joinl7b2026-05-20)）

| # | 根因 | 要点 |
|---|------|------|
| A | **Task 与 Sub-agent 混用（主因）** | 14×**`task_create`**（后台 **Task**，与主 Agent **平级**）却被说成「子代理」；应使用 **`agent_spawn`**（**上下级**）做 P1 并行审区，见 §7.1 |
| B | **Task 未 join** | 只 `task_list`，**零次 `task_read`**；Task 多数 **completed**（含 HIGH），对主会话仍等于未接入 |
| C | **C1 门禁被绕过** | Prompt 可能未命中 report 关键词；inventory 全 pending；报告经 **`write_file`→deliverables** |
| D | **目标错位** | 优先 MD 报告，非 inventory + `verified` |
| E | **错误叙事** | 曾称「task 未跑」——**`task_read` 证伪**；是 **Task 跑完未读**，不是 Sub-agent 未启动 |

模型自述（与证据一致）：只 polling `task_list`，未 `task_read`；**不是**「检查时机不对」，而是**没把 Task 当平级工单去收口**（且类别上误当成 sub-agent）。

### 优化路线（摘要）

| 档 | 内容 | 状态 |
|----|------|------|
| **1 Skill** | 禁止 P1 `task_create` 并行审区；spawn 后 `agent_list`→terminal→blackboard/`agent_result`→append+`set_area` | ✅ `audit-repo` § P1 parallel（待 L7 复测） |
| **2 引擎** | 扩大 C1 关键词 / `write_file` deliverables 硬门 / completed-task 未读提醒 | ⬜ 见 design §14.3 E1–E5 |
| **3 UI** | 横条 accounted=0 强提示；Task/子代理 Completed 未读 | ⬜ → **[Phase D §L8](#l8--phase-d-审计过程可视化规划)**（design §6.13） |

**L7b 闭环复测标准：** design §14.5（inventory 交代、verified-only、子代理 HIGH 不丢、禁止无证据「未跑」声明）。

---

## L7c — 全仓试跑 `2026-05-20-audit`（2026-05-20）

**Prompt（用户）：** 全库代码级审核，输出 MD 报告（与 L7/L7b 同类；未附加地狱级四维清单）。

**产物：**

```text
.deepseek/scratchpad/2026-05-20-audit/
├── inventory.json    # 35 areas，全部 done
├── notes.jsonl       # 39 行（7 finding verified + 31 cleared + 1 meta）
└── REPORT.md

workspace/.deepseek/scratchpad/2026-05-20-audit/  # 同上（若 cwd 为仓库根）
```

### 总评：✅ 流程闭环；⚠️ 执行路径与 L7b 不同

| 维度 | L7b `full-audit` | L7c `2026-05-20-audit` |
|------|------------------|-------------------------|
| inventory 收口 | 34×`pending`，accounted 0/34 | **35/35 `done`** |
| `scratchpad_set_area` | ❌ | ✅ |
| findings `verified` | ❌ 多为 `open` | ✅ 7 条 `finding` + `verified` |
| 并行执行体 | 14×**Task** | **无子代理**；`notes.jsonl` 全 `source:main` |
| 报告 | deliverables MD | `REPORT.md` + 聊天摘要；0 HIGH |
| Token（同日账单 Δ，基线 32,815,894） | — | **+5,155,043** 合计（+116 万未命中缓存，+397 万命中缓存，+2.6 万输出） |

### 磁盘核实

| 项 | 结果 |
|----|------|
| `inventory.json` | 35 area，**全部 `done`** |
| `notes.jsonl` | 39 行；7 `finding`/`verified`；31 `cleared`/`verified` |
| 严重度 | 5 MEDIUM + 2 LOW（hooks 吞错误、CLI env API key、大文件体量、CORS 等） |
| 子代理 / blackboard | **无** `.deepseek/blackboards/{run_id}`；无 `agent_*` 面板数据 |

### 行为观察（模型侧 vs 产品侧）

| 项 | 结果 |
|----|------|
| scratchpad 纪律 | ✅ append + set_area + P2 报告 |
| P1 子代理（skill / E5） | ⚠️ 口述曾计划 Explore，**实际主代理批读** |
| 覆盖诚实度 | ✅ 报告写明 prompts/widgets 等**抽样** |
| sidecar 保存设置重启 | ✅ 源码 + **2026-05-20 `tauri build` dist** 含 `sidecar://restarting` / `runtimeSidecarRestart`；保存设置时应清「生成中」并 toast |

### 与 Phase D 的关系

L7c 证明 **Harness 可托住长任务落盘**；仍缺 **过程可视化** 来暴露「子代理面板空 vs 叙事 spawn」「双轨 checklist」。验收见 [§L8](#l8--phase-d-审计过程可视化规划)。

**费用粗算：** 用户对照约 **6–8 元/次** 全库审；本次 Δ 以官方控制台 **未命中 + 输出** 乘单价为准（命中缓存单价更低）。

---

## L7d — 全仓试跑 `2026-05-20-001`（2026-05-20）

**Prompt（用户）：** 帮我对项目进行代码级审核，所有代码都要进行审核，并输出 md 格式的报告。

**产物：**

```text
.deepseek/scratchpad/2026-05-20-001/
├── inventory.json    # 29 areas，全部 done
├── notes.jsonl       # 38 行（29 finding + cleared/meta）
deliverables/code-audit-report-2026-05-20.md
```

（工作区根下路径；与 L7c 的 `workspace/.deepseek/...` 等价。）

### 总评：✅ Harness 闭环 + 真子代理；⚠️ 8 条 MEDIUM 仍 `open`

| 维度 | L7c `2026-05-20-audit` | L7d `2026-05-20-001` |
|------|------------------------|----------------------|
| inventory | 35/35 `done` | **29/29 `done`** |
| 执行体 | 主代理批读 | **~18 Explore 子代理 + Auditor**（`subagents.v1.json` 有记录） |
| HIGH | 0 | **14**（notes 全部 `verified`） |
| 报告 | `REPORT.md` + 摘要 | **`deliverables/code-audit-report-2026-05-20.md`** |
| Token（5/20，基线 38,955,035） | Δ ~+5.15M | **Δ ~+18.07M**（当日合计 **57,023,148**） |

### 磁盘核实

| 项 | 结果 |
|----|------|
| `inventory.json` | 29 area，**全部 `done`** |
| `notes.jsonl` | 29 `finding`（14 HIGH / 13 MEDIUM / 2 LOW）；21 `verified`、8 `open` |
| Auditor 元笔记 note-038 | 3/14 HIGH **行号漂移**，语义验证通过 |
| P0 跟进（代码） | H05 导出路径校验 · H06 移除 `get_runtime_token`（Tauri HTTP/SSE 代理）· H02 Explore `explicit_tools` 与类型白名单求交 · H03 blackboard `task_id` 校验 |

### 与 Phase D

L7d 是 **D1/D2 可视化** 的理想复测场景：子代理轨应有行、findings 条带应与 `notes.jsonl` 一致；口述 spawn 不应再出现「面板空」。

---

## L8 — Phase D：审计过程可视化（规划）

> **设计全文：** [audit-scratchpad-design.md §6.13](audit-scratchpad-design.md#613-phase-d--审计过程可视化路线图-未实现)

**目标：** 让用户对照 **磁盘 + runtime**，不只看聊天叙事；把 §2.1 **契约违约** 做成仪表盘（路考摄像头）。

### 分档与验收（R-可视化）

| 档 | ID | 交付 | 验收（复制用） |
|----|-----|------|----------------|
| **D1** | D1.1 | Inventory 面板（area 列表） | 与 `inventory.json` 35 行一致；status 色块 pending/in_progress/done/deferred |
| **D1** | D1.2 | U1 违约高亮 | `notes≥1` 且 accounted=0 → 横条红色 + 固定文案 |
| **D1** | D1.3 | scratchpad 工具后即时刷新 | `append`/`set_area` 后 3s 内横条更新（不必等 12s） |
| **D2** | D2.1 | inventory vs checklist 双轨 | 两数不一致时黄标 + `contract_warnings` |
| **D2** | D2.2 | 子代理轨 | spawn 后面板有行；全程 0 行且 transcript 含 spawn → 警告 |
| **D2** | D2.3 | Findings 条带 | verified/open 计数与 `notes.jsonl` 一致 |
| **D2** | U2 | Task / Sub-agent 分栏 + 未读徽章 | ✅ 与 §7.1 用语一致 |
| **D2** | U3 | 审计模式 hard block（accounted≥85%） | 未达标时 deliverables `write_file` 被拦 + UI 说明 |

### 建议实现顺序

```text
D-a: D1.1 + D1.2 + D1.3  →  D-b: D2.1 + D2.2  →  D-c: U2 + U3 + D2.3
```

### 复测 Prompt（D1 完成后）

在 L7c 同类全库 Prompt 上再跑一轮；**人为**在 area-3 只 `append` 不 `set_area`，确认 D1.2 红灯；修复后续审至 35/35。

**状态：** **D1 ✅** · **D2 ✅** · **U2 ✅**（2026-05-20）：D2.1 双轨 + `contract_warnings`；D2.2 子代理计数/口述 spawn 警告；D2.3 findings 条带（severity）；U2 侧栏 Task/子代理分栏 + Completed 未读徽章；**U3 ⬜**（D-c）。

---

## L9 — 地狱级四维审计（暂缓）

用户提议在 Prompt 中强制四类检查：**功能与逻辑**、**设计与可维护性**、**安全防护**、**可靠性与异常处理**（含业务满足度、算法复杂度、DRY、依赖 CVE、事务/熔断等）。

### 为何暂缓（相对 L8 优先）

| 风险 | 说明 |
|------|------|
| Token / 时间 | 在 L7c 已 ~+515 万 token/日 量级上，再叠四维全深审，易 **1.5×–3×** 成本 |
| 模型行为 | 易变成 **清单表演**（每 area 写 meta「已检查」无 `file:line`），而非真 finding |
| 无验收 schema | UI（D3.1）无数据可画，除非先约定 `notes.jsonl` meta 字段 |
| 与现有重叠 | 安全、异常、体量与当前 `audit-repo` / `base.md` 已部分覆盖 |

### 若将来启动（前置条件）

1. **Phase D1** 上线（违约可见）。  
2. Prompt 约定：每 area 每维度 **最多 1 条** `verified` 或 `cleared`；无证据则 `kind=meta` + `未深入`；禁止无 `file:line` 的 HIGH。  
3. **P0 深审 / P2 浅扫** 分 priority；**不做**全仓 `cargo audit` / `npm audit` 除非有工具输出文件可读。  
4. 范围可先 **地狱深度仅 `crates/tui` + `crates/desktop`**，其余 crate 沿用 L7c 轻量 pass。

### 示例 Prompt 片段（勿与 L8 同 PR）

见 design 讨论纪要；完整模板待 L9 立项时再写入 §7 Prompt 清单。

**状态：** ⏸ 暂缓（2026-05-20）。

### 续审 Prompt（闭环 inventory，可选）

```text
run_id=2026-05-20-full-audit：
1) scratchpad_status 贴 JSON；
2) 对每个已有 notes 的 area 调用 scratchpad_set_area（done 或 deferred+meta）；
3) 进报告的 finding 改为 status=verified（或 supersedes）；
4) 不重写报告，只回报 inventory 完成率与 verified 条数。
```

**发版建议：** L7b 可作为「新包全仓 UX 冒烟」⚠️ 通过；在 inventory 全 pending 前提下**不能**视为 L7 流程回归 ✅。子代理面板、横条 `notes N` 需在重编 sidecar + web-ui 后由用户再确认一行。

---

### 8.5 失败时收集

请贴：**步骤 ID**、**新/原会话**、**run_id**、工具名 + 错误 JSON、横条截图、  
`inventory.json` 前几行、相关 `notes.jsonl` 行（可打码）。便于对照 [design §13.5](audit-scratchpad-design.md#135-第四轮--phase-c-设计评审ds-pick2026-05-19)。
