# Audit Scratchpad — 试跑记录（Phase A–C）

> **设计：** [audit-scratchpad-design.md](audit-scratchpad-design.md)  
> **环境：** DS Pick 新包（含 Phase A/B/C：工具、横条、覆盖率门禁、Auditor←scratchpad、blackboard 镜像）  
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

**结论：** Phase A 在单区与多区场景下可用；长程「落盘纪律」已验证。Phase B 在 `2026-05-19-phase-b-smoke` 上验收通过。**新包回归**：R1 ✅ · R2 ⚠️ · R3 ✅ · R4a ⚠️ · R4b ⏭ · **R5 ✅**（Auditor 预期 **FAIL** = 检测生效）；R6 可选见 §8.4。

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

> **目的：** 新安装的 DS Pick + sidecar 一次性验证 A/B/C；区分 **新会话**（干净线程）与 **原会话**（续审 / 历史线程）。  
> **建议耗时：** 必测约 45–60 分钟；含 C0 compact、多区压测可再加 30–60 分钟。  
> **记录：** 每步在下方「回归总评」表填 ✅/❌/⏭ 与备注。

### 8.0 测前检查（5 分钟）

| # | 检查项 | 命令 / 操作 | 期望 |
|---|--------|-------------|------|
| P0 | 完全退出并重启 DS Pick | 任务管理器无旧 `deepseek-tui` / DS Pick | 新 sidecar 已加载 |
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

| 类型 | 含义 | 在 DS Pick 里怎么做 | 验证什么 |
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
| `<scratchpad_summary>` 含 WARNING/BLOCKED | ⚠️ 未确认 | 用户粘贴**未含**该标签；按默认 config（`accounted_ratio` 33% &lt; hard 60%）引擎应走 **BLOCKED** 分支（见 `coverage_gate` / `build_report_summary_message`）— 建议在 DS Pick 该轮消息流中搜 `scratchpad_summary` 或 `BLOCKED:` |
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

### 8.5 失败时收集

请贴：**步骤 ID**、**新/原会话**、**run_id**、工具名 + 错误 JSON、横条截图、  
`inventory.json` 前几行、相关 `notes.jsonl` 行（可打码）。便于对照 [design §13.5](audit-scratchpad-design.md#135-第四轮--phase-c-设计评审ds-pick2026-05-19)。
