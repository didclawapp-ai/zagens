# 审计 Scratchpad

**审计 Scratchpad** 是代码模式下长程**全库/分区审计**的结构化工作记忆：把发现、证据与区域进度写入工作区磁盘，**跨多轮、多子代理**可读可验，而不只靠聊天上下文。

> **办公模式通常不需要 Scratchpad** — 用 DOCX 章节组织即可。

---

## 存储位置

每个 **run** 对应一个目录（默认 `run_id` = 当前 **thread_id**）：

```
{工作区}/.zagens/scratchpad/{run_id}/
  inventory.json    # 审计区域清单与状态
  notes.jsonl       # 逐条发现/元数据（append-only）
```

（legacy 安装可能仍在 `.deepseek/scratchpad/`。）

**会话绑定：** Scratchpad 状态按 **thread** 绑定 — **其他会话默认不继承**上一会话的 run。换 thread 需重新 **初始化** 或让 Agent 调用 `scratchpad_init`。

---

## 在哪打开

### 审计网格（推荐）

1. **代码**会话，且已有 checklist / scratchpad / 子代理活动时，标题栏出现 **显示审计网格**。
2. 打开后 **2×2 布局**，左上 **清单**、右上 **审计**（即 Scratchpad）、左下 **长程任务**、右下 **子代理**。
3. 拖拽左缘调整网格宽度；可 **隐藏审计网格** 再次收起。

侧栏 **子代理** 单独打开时看不到 Scratchpad 全文 — 审计象限在网格内。

### 空态：尚未绑定 run

显示：

- 「本会话尚未绑定审计 scratchpad」
- 说明需 `scratchpad_init` 或点击下方按钮
- 路径提示：`{工作区}/.zagens/scratchpad/{thread_id}/`
- 按钮 **「初始化 scratchpad」** — 调用 runtime HTTP init，默认 `run_id` 为当前 thread

---

## 面板读什么（与 UI 一一对应）

绑定 run 后，卡片展示：

### 进度行

`进度 accounted/total (pct%)` — **accounted** = `done + in_progress + deferred` 区域数。

附加：`done N` · `进行中 N` · `deferred N`。

### Checklist 双轨（最新 run）

若同 thread 有 LHT checklist：`Checklist completed/total`。

- 与 inventory **不一致**时琥珀警告：**「Checklist 有完成项但 inventory 未记账」** — 须用 `scratchpad_set_area`，不能只用 `checklist_write`。

### 续审区

`resume_area` 存在时显示 **续审区** `{area_id}` — Agent 应从此区域继续。

全部区域 accounted 且 total>0 时显示 **「inventory 已完成」**。

### Findings 条

`verified V · open O`，并按严重度拆分：

| 后缀 | 含义 |
|------|------|
| **HIGH** | 未关闭高严重度 |
| **MED / LOW** | 中/低 |
| **✓HIGH** | 已验证的高严重度 |

### 子代理与警告徽章

| 徽章 | 含义 |
|------|------|
| **子代理 N 活跃** | 当前有 N 个子代理在跑（最新 run） |
| **待 scratchpad_set_area（有 notes 但 inventory 未记账）** | 有 notes 但未 `set_area` — **contract 违规** |
| **Checklist 与 inventory 脱节** | 双轨不同步 |
| **口述 spawn / 子代理面板无数据** | 聊天声称 spawn 但面板无 `agent_*` 行 |

### Inventory 列表

可折叠表格，每行：

| 列 | 说明 |
|----|------|
| **状态** | `pending` · `in_progress` · `done` · `deferred`（颜色标签） |
| **area id** | 如 `workspace`、`crates-runtime` |
| **path** | 工作区内相对路径 — 点击可在工作台打开 |
| **notes 数** | 该区域在 `notes.jsonl` 中的条数 |

### 历史 run

同 thread 较早的审计以 **「历史审计」** 折叠卡片列出（`previous_runs`），避免与最新 run 混淆。

---

## Runtime 工具（Agent 侧）

| 工具 | 作用 |
|------|------|
| **`scratchpad_init`** | 创建 run；可传 `areas[]` 或 `template=workspace_audit`（按 Cargo workspace 成员生成 inventory） |
| **`scratchpad_status`** | 读进度、findings 统计、`resume_area_id`（与面板同源） |
| **`scratchpad_append`** | 向 `notes.jsonl` 追加一行（需合法 `area_id`, `kind`, …） |
| **`scratchpad_set_area`** | 更新区域状态 → `done` / `deferred` / `in_progress` |
| **`scratchpad_verify_note`** | 读代码后把 finding 标为 **verified**（`done` 前对 HIGH/BLOCKER 通常必需） |
| **`scratchpad_list_notes`** | 按 `area_id` 列出 notes（恢复上下文时用，不要只读 jsonl 尾部） |

### notes 行 `kind`（常用）

| kind | 用途 |
|------|------|
| **finding** | 审计发现（报告只认 verified finding） |
| **cleared** | 已消除的问题 |
| **meta** | 延期原因等元数据（`deferred` 时常需） |
| **todo** | 待办 |

### 区域状态流转（建议）

```text
pending → in_progress → done
                    ↘ deferred（需 meta 说明）
```

**完成区域前：** 至少 `scratchpad_append` 一条有效 note → 必要时 `scratchpad_verify_note` → **`scratchpad_set_area(done)`**。

仅改 checklist **不能**代替 inventory 记账。

---

## 与 CRAFT 的配合

| 机制 | 作用 |
|------|------|
| **Scratchpad** | 长跑审计的 **区域进度 + 证据库**（多 run、可续审） |
| **CRAFT 黑板** | 单次 `task_id` 下 Explore/Review **结构化交接** |

典型 **全库审查**（[craft-audit 预置](/zh-Hans/docs/settings/lht#①-harness-预置)）：

1. LHT harness **关**（或 Composer **LHT·关**）
2. 面板或 `scratchpad_init` 建立 inventory
3. 按区域 spawn **Explore**（只读），`scratchpad_import_agent` / append 写入发现
4. 需修代码时 spawn **Implementer**；**Review** 产出 PASS/BLOCKER
5. 最终报告只引用 **verified** findings

Explore 子代理可绑定 **`scratchpad_run_id`**，与当前 run 隔离。

---

## 与清单（Checklist）双轨

LHT 使用 `checklist_write` 跟踪工程任务；Scratchpad 使用 **inventory** 跟踪**审计覆盖**。

两者应同步：**checklist 项完成时，对应 inventory 区域应 `scratchpad_set_area(done)`**。面板会检测 `checklist_inventory_mismatch` 并警告。

---

## 推荐使用方式

1. **大仓库**：`scratchpad_init` + `template=workspace_audit`，或手动 `areas` 按 crate/目录拆分。
2. **续审**：先 `scratchpad_status` 看 `resume_area_id`，再 `scratchpad_list_notes` **仅加载该区域**。
3. **子代理并行**：大 subtree 用 Explore 子代理，结果整合进 Scratchpad，**不要静默跳过文件**（须在 notes 记录原因）。
4. **超时**：子代理 `Failed: API call timed out` **不等于**区域完成 — 缩小范围重 spawn 或提高 `step_timeout_ms`，勿仅凭超时标 `done`。
5. **写报告前**：runtime 会检查 pending 区域与 contract — 确保全部 `done|deferred`。

### 示例 prompt

> 初始化 scratchpad（workspace_audit 模板）。从 resume 区域开始，读完该 path 下源码后 scratchpad_append finding，verify 后 scratchpad_set_area(done)。全部区域完成后写 Markdown 审计报告，只含 verified findings。

---

## 常见问题

**Q：面板一直空态？**  
需在本 **thread** 内初始化；其他会话的 run 不会自动显示。点 **初始化 scratchpad** 或让 Agent 调用 `scratchpad_init`。

**Q：进度 100% 但仍有 open findings？**  
`accounted/total` 指 **区域**记账，不是 findings 清零。open HIGH 可能仍阻塞写正式报告。

**Q：能否删除 run？**  
UI 不提供删除；可手动清理 `.zagens/scratchpad/{run_id}/`（谨慎）。

**Q：办公模式能用吗？**  
工具面不注册 scratchpad；请用 [办公技能](/zh-Hans/docs/office/skills)。

---

相关：[CRAFT 多代理](/zh-Hans/docs/code/craft) · [子代理面板](/zh-Hans/docs/code/subagents) · [LHT 设置](/zh-Hans/docs/settings/lht) · [清单侧栏](/zh-Hans/docs/code/checklist)
