# Audit Scratchpad — 试跑记录（Phase A + B）

> **设计：** [audit-scratchpad-design.md](audit-scratchpad-design.md)  
> **环境：** DS Pick 桌面端（Phase A：`pick-rules` §7、`base.md`、`audit-repo`；Phase B：`scratchpad_*` 工具 + `AuditScratchpadBar`）  
> **日期：** 2026-05-19

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

**结论：** Phase A 在单区与多区场景下可用；长程「落盘纪律」已验证。Phase B（工具、门禁、续审、P2 合成、桌面横条）在 `2026-05-19-phase-b-smoke` 上验收通过；测试 5/6（多区压测、B4 只读提醒）未跑，可后续补。

---

## 0. 前置条件（试跑前）

```powershell
Test-Path "$env:USERPROFILE\.deepseek\skills\audit-repo\SKILL.md"   # True
Get-Content "$env:USERPROFILE\.deepseek\skills\.system-installed-version"  # 2
```

工作区：仓库根 `F:\DeepSeek-TUI-desktop`（或等价路径）。TaskType：**Code**。

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

| 项 | 说明 |
|----|------|
| 全仓 20～40 area（含 desktop 等） | 规模更大；依赖 B 的分层注入 |
| `supersedes` 升级路径 | 未触发 |
| 覆盖率硬门禁 / Auditor+scratchpad 深绑 | Phase C |
| compaction 丢 scratchpad 指针 | 长会话 + compact 时再观察 |

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

**延后 Phase C：** 覆盖率硬拦、blackboard 分区、Auditor 绑定。

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
