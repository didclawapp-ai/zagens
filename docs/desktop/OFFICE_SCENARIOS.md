# Zagens 办公场景地图

> **状态：** 产品备忘（2026-06-05，落地口径同步 2026-06-05）  
> **Phase A 完成度：** L1/L2/L3 底座已落地；L4 共 **11** 个 bundled 技能 + **11** 张空态卡片 + P0 fixtures/oracle；剩余主攻读表保真稳定性与 P0 端到端跑绿。  
> **本期范围外（不计入差距）：** STT/TTS 语音（Phase C）、ERP/CRM connector（Phase B）、`inbox/`/`data/` 工作区自动初始化（用户自建或复制 fixtures 即可）。  
> **定位：** 在 LHT / CRAFT 等 **编码 harness** 之外，梳理 **Office 模式**可覆盖的真实工作场景、与现有能力对齐情况、以及跑通优先级。  
> **核心主张（本版新增）：** 这 40+ 个场景不是 40+ 个独立功能，而是 **同一条流水线 × 四个正交维度** 的不同取值组合。统一架构见 §2.3 / §3；新增场景应退化为「填一份技能契约」，而非「写新引擎」。  
> **相关：** [task-type-prompt-architecture.md](../task-type-prompt-architecture.md)、[HARNESS.md](HARNESS.md)（Office 迭代计划与 DEV_NOTES 见本地 `doc_Private/docs/`）

---

## 1. 产品一句话

Zagens 办公线不是「再做一个聊天写 Word」，而是：

> **能读表、能联网、能交文件（DOCX / XLSX / PPTX / PDF）的本地桌面办公副驾** — 先给概况，再出可交付物；后续可加语音（STT → TTS 概况 → 确认 → 文档）。

与 **Code 模式**分工：Office 裁掉 shell / patch / 子代理等代码向工具，保留 `read_office`、`write_office`、联网与技能（见 `office.md`）。

---

## 2. 场景分类（两种视角）

### 2.1 按「信息流向」

| 类型 | 含义 | 典型输出 | Zagens 主要能力 |
|------|------|----------|-----------------|
| **外察** | 看市场、竞品、政策、行情 | 调研简报 DOCX | `web_search`、`fetch_url`、`finance` |
| **内聚** | 收各部门材料，给老板/主管一份总览 | 经营日报 / 周报 DOCX | `read_office` 多文件 + 摘要 + `write_office` |
| **内业** | 车间、销售、财务用内部数据出表 | 生产简报、报价单 XLSX | `read_office` + `write_office`（XLSX 纯 Rust） |
| **创作** | 从零写方案、合同、纪要 | DOCX / PPTX | `load_skill` + `write_office` |
| **加工** | 翻译、合并、改一版 | 同格式或新文档 | `read_office` → 改 → `write_office` / `load_office_payload` |
| **交付** | 找到文件、预览、改一列、发出去 | 体验层 | `deliverables/` 默认目录 + 高亮、PDF/HTML 右栏预览、Office 系统打开；一键导出 PDF 待做 |

### 2.2 按「角色 / 职能」

见 §4 场景目录（按部门展开）。

### 2.3 按「四个正交维度」（统一抽象 · 推荐作为架构一等公民）

§2.1 / §2.2 是给人看的「目录视角」。但从**工程视角**看，§4 那一长串场景其实只是在四个**相互独立**的轴上取不同值。任何一个办公场景都能写成一组坐标 `(摄取, 处理, 输出, 交互)`：

| 轴 | 含义 | 取值空间 | 对应 §2.1 / §4 的体现 |
|----|------|----------|------------------------|
| **① 摄取源 Ingest** | 数据从哪来 | `web`（联网）· `files`（`inbox/`,`data/`）· `dictation`（口述）· `vision`（`describe_image`，扫描件）· `connector`（ERP/CRM，**远期**） | 外察 / 内聚 / 内业 |
| **② 处理意图 Transform** | 对数据做什么 | `summarize` · `aggregate` · `compare` · `compute` · `translate` · `draft` · `extract` | 汇总 / 对比 / 报价计算 / 翻译 / 起草 |
| **③ 输出契约 Render** | 交付什么 | 格式 `docx/xlsx/pptx/pdf` + `sections`/`sheets` 结构 | §5 默认输出列 |
| **④ 交互节奏 Loop** | 怎么来回 | `oneshot` · `brief_first`（先概况）· `confirm` · `iterable`（增量改）· `voice` | §3 流水线 + Phase C 语音 |

**关键结论：** 「场景」不该是架构的一等公民，**四个轴才是**。一个场景 = 在这四轴上选定的一组坐标；统一架构只需把四轴各自做成可复用积木，新增场景就退化为**声明式配置**（见 §3 技能契约）。

**用四轴重新解释文档里的几组对比：**

- **老板日报 vs 车间晨报（§4.1）**：差异只在 `ingest`（多源 vs 单源）+ `render.sections`，流水线内核完全相同。
- **P0-1 ~ P0-4 四条 demo（§6）**：恰好是四轴的四个代表性坐标（外察 / 内聚 / 内业读表 / 计算+迭代）——**跑通这 4 条 ≈ 验证四轴各自打通**，而非验证 4 个孤立功能。
- **语音（Phase C）**：只是把 `loop` 从 `brief_first(文字)` 换成 `voice(STT/TTS)`，**不触碰摄取/处理/生成**。

---

## 3. 统一架构（所有场景共用）

办公线收敛为 **4 层**：场景只在最上面一层（声明式配置）变化，下面三层固定复用。

```
┌─────────────────────────────────────────────────────────────┐
│ L4  场景层（声明式）= 一份 SKILL.md「技能契约」               │
│     只声明四轴坐标：ingest + transform + render + loop + verify │
│     ↑ 新增场景在这一层，零引擎改动（见 §3.2）                  │
├─────────────────────────────────────────────────────────────┤
│ L3  流水线内核（固定 6 段，见 §3.1）                          │
│     触发 → 摄取 → 概况 → 确认 → 生成 → 交付/迭代               │
├─────────────────────────────────────────────────────────────┤
│ L2  能力原语（正交工具，已实现/在建）                          │
│   摄取 read_office · web_search · fetch_url · finance          │
│   视觉 describe_image（视觉桥接，扫描件 OCR 路径）              │
│   生成 write_office(source 直喂) · load_office_payload(增量改) │
│   交付 deliverables/ · 预览 · open_with_system_app             │
│   P0 工程缺口 → office-mode-iteration-plan §三 能力差距矩阵    │
├─────────────────────────────────────────────────────────────┤
│ L1  基座 TaskType=Office 隔离 · Python venv · 网络策略门控     │
└─────────────────────────────────────────────────────────────┘
```

> **现状提示：** L1/L2/L3 已落地（见 [office-mode-iteration-plan.md](../office-mode-iteration-plan.md) 推荐实施顺序 1–8）。**剩余工作量集中在 L4 场景补齐与验收**，以及 §8 所列 L2 细项（读表保真 golden、迭代修改产品化、企业模板）；扫描件 OCR **不走内置引擎**，统一经视觉桥接（§4.6）。

### 3.1 流水线内核（L3，6 段固定）

**逻辑顺序（用户故事）：** 摄取 → 处理（模型推理）→ 概况 → 确认 → 生成 → 交付/迭代。  
下列 6 段是**产品验收分段**，实现时不必做成硬状态机；模型可在单轮内交织「读 + 想 + 写」。

```
触发（任务卡片 / 打字 / 未来：语音 STT）
  → Office 模式 session（独立 TaskType，与 Code 会话隔离）
  → load_skill（可选，高频任务推荐）
  → ① 摄取：read_office / web_search / fetch_url / 用户口述 / describe_image（扫描件）
  → ② 处理：摘要 / 汇总 / 对比 / 计算 / 翻译 / 起草（prompt + 模型，非独立 runtime 算子）
  → ④ 概况（可选）：对话摘要（未来：TTS 口播 30～60s）     ← Loop.brief_first
  → ④ 确认（可选）：「生成正式文档？」                     ← Loop.confirm
  → ③ 生成：write_office → deliverables/<title>.<ext>      ← Render
  → ④ 交付/迭代：预览 / 外发 / load_office_payload         ← Loop.iterable
```

### 3.2 技能契约（L4，让新增场景 = 填表）

把 §5 已有的 **11** 个 bundled 技能与附录 A 其余待建技能，统一成同一个声明 schema。每个 `office-*/SKILL.md` 只需按四轴 + 校验填写，**无需改引擎**。示例（P0-2 样板「老板经营日报」）：

```yaml
id: office-executive-daily-brief
ingest:                       # 轴① 摄取源
  - kind: files
    from: inbox/              # 多部门附件
    formats: [docx, xlsx, pdf]
transform:                    # 轴② 处理意图（组合 L2 + 模型推理）
  - summarize_per_source
  - aggregate
  - extract: pending_decisions
render:                       # 轴③ 输出契约
  format: docx
  sections: [概况, 各部门要点, 风险, 待决事项, 附录]
  out: deliverables/          # path 可省，默认即此
loop:                         # 轴④ 交互节奏
  brief_first: true           # 先文字/口播概况
  confirm_before_render: true
  iterable: true              # 支持 load_office_payload 增量改
verify:                       # 验收配置（见下「契约落地」）
  - sources_cited
  - has_section: 待决事项
```

**契约字段说明：**

| 块 | 含义 | 运行时 |
|----|------|--------|
| `ingest` / `render` / `loop` | 四轴坐标 + 目录约定 | 由 SKILL 正文步骤 + 模型执行；引擎不解析 YAML |
| `transform` | **技能指令语义**（汇总、对比、计算…） | **非**独立 runtime 算子；写在 SKILL 步骤里 |
| `verify` | 演示 / 回归 oracle | **非**自动 gate；供人工验收或未来 headless 脚本 |

**契约落地（三阶段，均不要求改引擎）：**

| 阶段 | 动作 |
|------|------|
| **1 — 约定层** | ✅ **11/11** — 全部 bundled `office-*/SKILL.md` 含 `## 技能契约` + YAML + 编号步骤（样板：`office-executive-daily-brief`） |
| **2 — 校验层** | ❌ 待建 — 可选 `scripts/office-skill-lint.mjs`：检查契约字段、§6 验收项是否齐全 |
| **3 — 回归层** | ⚠️ 部分 — `docs/harness/fixtures/office-demo/` + `scripts/office-demo-oracle.ps1`（P0-2/3/4；P0-1 无 headless oracle） |

> **与现有技能对齐：** 全部 11 个 bundled 技能已是「确认 → 摄取 → 处理 → 生成 → 增量改」结构（见 `office-weekly-report`）。契约把隐式约定**显式化**；P0 三条新技能与卡片已落地，见 §5 / §10。

**目录约定（建议 demo / 企业工作区统一）：**

| 路径 | 用途 |
|------|------|
| `inbox/` | 各部门扔进来的原始附件（日报、表、纪要）；**用户自建**或从 `office-demo` fixtures 复制 |
| `data/` | 结构化数据源（价目表、生产日报、主数据）；同上，**不自动初始化** |
| `deliverables/` | Agent 输出（默认，技能可不填 `path`）；工作区创建时自动确保存在 |
| `templates/` | 企业母版 / 价目表模板（未来） |

**语音扩展（Phase C，本期范围外）：** 同一流水线，仅把「触发 + 概况」换成 STT / TTS；执行仍走 Office 工具面。见 `doc_Private/docs/desktop/DEV_NOTES.md` §入座 briefing。

**与 LHT / CRAFT：** 办公单次任务通常 **不需要** LHT checklist；多文件长调研、跨天跟进可考虑轻量 checklist，但不作为办公线默认。

---

## 4. 场景目录

图例：**成熟度** — ✅ 技能/卡片可试 · ⚠️ 技能已有但端到端或读表保真待验证 · ❌ 技能待建或依赖企业模板 · 🔮 远期（本期范围外：语音、ERP/CRM connector）

**四轴缩写（§2.3）：** `摄取|处理|输出|交互` — 例：`files,aggregate,docx,brief+confirm`

### 4.1 管理层 / 决策

| 场景 | 四轴（缩写） | 角色 | 典型说法 / 触发 | 输入 | 输出 | 技能 / 卡片 | 成熟度 |
|------|--------------|------|-----------------|------|------|-------------|--------|
| **经营日报汇总** | `files,aggregate,docx,brief+confirm` | 老板、高管 | 「汇总一下昨天的日报」 | `inbox/` 各部门 DOCX/摘要 | Executive brief DOCX | `office-executive-daily-brief` ✅ | ⚠️ |
| **周报 / 月报** | `files+dictation,summarize,docx,iterable` | 主管 | 「写本周周报」 | 附件 + 口述 | DOCX | `office-weekly-report` ✅ | ✅ |
| **项目汇报 PPT** | `files+dictation,draft,pptx,oneshot` | 项目负责人 | 「做一份项目汇报 PPT」 | 要点 + 材料 | PPTX | `office-project-report` ✅ | ✅ |
| **月经营分析** | `files,compute+compare,xlsx+docx,iterable` | 财务+管理层 | 「根据上月销售表做经营分析」 | XLSX | DOCX + 图表 XLSX | `office-data-report` + 定制 | ⚠️ |
| **决策备忘录** | `files+web,compare,docx,confirm` | 高管 | 「整理 A/B 方案供决策」 | 笔记 + 调研 | DOCX（选项对比） | 待建 `office-decision-memo` | ❌ |
| **董事会 / 投资人简报** | `files,summarize+extract,pptx,brief` | CEO | 「压缩成 5 页投资人要点」 | 长材料 | PPTX / DOCX | 待建 | ❌ |

**老板日报 vs 车间晨报：** 老板场景是 **多源汇总 + 待决事项**；车间场景是 **单一主题 + 结构化指标**（见 §4.4）。

---

### 4.2 市场 / 销售 / 商务

| 场景 | 四轴（缩写） | 角色 | 典型说法 | 输入 | 输出 | 技能 | 成熟度 |
|------|--------------|------|----------|------|------|------|--------|
| **竞品 / 市场动态** | `web,summarize+compare,docx,oneshot` | 市场 | 「调研竞品 A/B 最近动态」 | 联网 | DOCX + 来源 | `office-competitive-analysis` ✅ | ✅ |
| **市场日报 / 周报** | `web+files,summarize,docx,oneshot` | 市场 | 「今天行业有什么动静」 | 联网 + 可选内部笔记 | DOCX | `office-market-watch`（待建） | ⚠️ |
| **活动 / 战役简报** | `web+dictation,draft,docx,confirm` | 市场 | 「写 Q3 推广方案大纲」 | 口述 + 调研 | DOCX | 待建 | ❌ |
| **客户报价单** | `files,compute,xlsx,iterable` | 销售（小王） | 「按客户需求整理报价」 | 价目表 XLSX + 需求 | 报价 XLSX | `office-customer-quote` ✅ | ⚠️ |
| **商务提案 / Proposal** | `files+dictation,draft,docx+pptx,confirm` | 销售 | 「给客户写方案书」 | 需求 + 模板 | DOCX / PPTX | 待建 | ❌ |
| **销售日报** | `files,aggregate,docx+xlsx,oneshot` | 销售主管 | 「汇总今日销售跟进」 | CRM 导出 / 表 | DOCX / XLSX | 待建 | ❌ |
| **合同初稿** | `dictation,draft,docx,iterable` | 商务 / 法务协助 | 「起草采购合同初稿」 | 条款要点 | DOCX | `office-contract-draft` ✅ | ✅ |
| **招标应答提纲** | `files,extract+draft,docx,confirm` | 售前 | 「按招标文件列应答目录」 | PDF/ DOCX 招标 | DOCX 提纲 | 待建 | ❌ |

---

### 4.3 生产 / 品质 / 供应链 / 运营

| 场景 | 四轴（缩写） | 角色 | 典型说法 | 输入 | 输出 | 技能 | 成熟度 |
|------|--------------|------|----------|------|------|------|--------|
| **生产 + 品质晨报** | `files,summarize+aggregate,docx+xlsx,brief+confirm` | 生产/品质（小李） | 「汇报昨天生产现况和品质现况」 | 昨日 MES/Excel 导出 | 先概况 → DOCX/XLSX | `office-production-daily-report` ✅ | ⚠️ |
| **异常 / 8D 报告** | `files,draft+extract,docx,confirm` | 品质 | 「整理这批不良品异常说明」 | 检验记录 | DOCX | 待建 | ❌ |
| **排产 / 工单摘要** | `files,summarize,docx,oneshot` | 计划 | 「总结本周工单完成情况」 | XLSX | DOCX | 待建 | ❌ |
| **供应商评估** | `files,compare,xlsx,iterable` | 采购 | 「对比三家供应商报价与交期」 | 多 XLSX | 对比表 XLSX | 待建 | ⚠️ |
| **库存 / 周转简报** | `files,compute+compare,xlsx,iterable` | 仓储 | 「上周库存异动说明」 | 库存表 | XLSX 报表 | `office-data-report` 改 | ⚠️ |
| **SOP / 作业指导书** | `files+dictation,draft,docx,iterable` | 工艺 | 「把这段流程写成 SOP」 | 口述 + 旧版 | DOCX | 待建 | ❌ |

---

### 4.4 财务 / 行政 / HR

| 场景 | 角色 | 典型说法 | 输入 | 输出 | 技能 | 成熟度 |
|------|------|----------|------|------|------|--------|
| **费用 / 报销汇总** | 财务 | 「汇总本月报销分类」 | XLSX | XLSX + 摘要 DOCX | 待建 | ⚠️ |
| **预算执行差异** | 财务 | 「实际 vs 预算差异说明」 | 两表 XLSX | DOCX + 表 | 待建 | ⚠️ |
| **发票 / 对账清单** | 财务 | 「整理待付款清单」 | CSV/XLSX | XLSX | 待建 | ⚠️ |
| **会议纪要** | 行政 | 「整理今天会议决议」 | 录音转写 / 笔记 | DOCX | `office-meeting-minutes` ✅ | ✅ |
| **通知 / 公告** | 行政 | 「写全员放假通知」 | 口述 | DOCX | 通用 office | ✅ |
| **招聘 JD** | HR | 「写 Java 工程师 JD」 | 岗位要点 | DOCX | 待建 | ❌ |
| **面试纪要** | HR | 「整理候选人面试评价」 | 笔记 | DOCX | 待建 | ❌ |
| **简历 / 求职信** | 个人 / HR | 「按岗位改简历」 | 旧简历 | DOCX | `office-resume` ✅ | ✅ |

---

### 4.5 产品 / 研发 / 项目（偏办公，非 Code 模式）

| 场景 | 角色 | 典型说法 | 输入 | 输出 | 技能 | 成熟度 |
|------|------|----------|------|------|------|--------|
| **发布说明** | 产品 | 「写版本发布说明」 | changelog | DOCX | `office-release-notes` ✅ | ✅ |
| **PRD 提纲** | 产品 | 「把需求整理成 PRD 结构」 | 笔记 | DOCX | 待建 | ❌ |
| **用户调研摘要** | 产品 | 「总结 5 份访谈」 | 多 DOCX | DOCX | 待建 | ⚠️ |
| **竞品功能矩阵** | 产品 | 「做功能对比表」 | 联网 + 内部 | XLSX / DOCX | `office-competitive-analysis` 扩展 | ⚠️ |

> **边界：** 改代码、跑测试、长程重构 → **Code 模式 + LHT/CRAFT**；Office 只交付 **文档 / 表 / 汇报**。

---

### 4.6 通用 / 跨职能

| 场景 | 说明 | 成熟度 |
|------|------|--------|
| **多文档合并** | 三份周报 → 一份月报 | ⚠️ 读表保真 + 多文件 |
| **翻译 / 本地化** | 合同 / PPT 章节翻译 | ⚠️ |
| **格式转换叙事** | 「把要点做成 PPT」 | ✅ `write_office` |
| **邮件 / 消息草稿** | 对外回复、跟进邮件 | ✅ 对话即可，可选 DOCX |
| **政策 / 法规摘要** | 联网 + 引用 | ⚠️ 来源规范 |
| **数据可视化** | CSV → 图表 XLSX / PPTX | ✅ `write_office` `source` 直喂 CSV/TSV/XLSX |
| **扫描件 OCR** | PDF 图片页 | ✅ **视觉桥接** — `read_office` 文本层为空时走 `describe_image`（不在 `read_office` 内置 OCR；见 [office-read-tool-plan.md](../office-read-tool-plan.md)） |

---

## 5. 已落地技能与 UI 卡片（对照）

共 **11** 个 bundled 技能，空态 **11** 张任务卡片（P0 三条置顶）。路径：`crates/runtime-server/assets/skills/office-*/SKILL.md`；卡片与 prefill：`crates/desktop/web-ui/src/components/OfficeEmptyState.tsx` + `i18n/locales/*.ts` → `officeEmpty`。

| 技能名 | UI 卡片（zh-Hans） | 默认输出 | 批次 |
|--------|-------------------|----------|------|
| `office-executive-daily-brief` | 经营日报汇总 | DOCX | P0 |
| `office-customer-quote` | 客户报价单 | XLSX | P0 |
| `office-production-daily-report` | 生产品质晨报 | DOCX | P0 |
| `office-weekly-report` | 周报 | DOCX | 首批 |
| `office-meeting-minutes` | 会议纪要 | DOCX | 首批 |
| `office-project-report` | 项目汇报 PPT | PPTX | 首批 |
| `office-data-report` | 数据报表 | XLSX | 首批 |
| `office-competitive-analysis` | 竞品分析 | DOCX | 首批 |
| `office-contract-draft` | 合同初稿 | DOCX | 首批 |
| `office-resume` | 简历 / 求职信 | DOCX | 首批 |
| `office-release-notes` | 发布说明 | DOCX | 首批 |

---

## 6. 优先跑通的「示范场景」（建议 P0）

结合业务讨论，建议 **先跑通 4 条端到端 demo**（文字版，不依赖语音），证明 Office 线商业价值。这 4 条**刻意各取四轴的一个代表性坐标**——跑通 = 四轴各自打通，而非 4 个孤立功能（见 §2.3）：

| 优先级 | 场景 | 主验证轴 | 技能 / 卡片 | 验收标准 | 落地状态 |
|--------|------|----------|-------------|----------|----------|
| **P0-1** | 市场竞品 / 行业动态 | ① `ingest=web` + 来源约束 | `office-competitive-analysis` ✅（`office-market-watch` 未建） | 联网 + 来源列表 + DOCX 进 `deliverables/` | ⚠️ 可试；无 headless oracle |
| **P0-2** | 老板经营日报汇总 | ① `ingest=files(多源)` + ② `aggregate` | `office-executive-daily-brief` ✅ | `inbox/` 多附件 → 5 段结构 + 待决事项 | ✅ 技能+卡片+fixtures+oracle |
| **P0-3** | 生产/品质晨报（小李） | ① `ingest=files` + ④ `brief_first` | `office-production-daily-report` ✅ | 读昨日 XLSX → 文字概况 → DOCX/XLSX | ⚠️ 技能链就绪；读表保真影响稳定性 |
| **P0-4** | 客户报价单（小王） | ② `compute` + ④ `loop=iterable` | `office-customer-quote` ✅ | 价目表 + 需求 → 含税合计 XLSX，可增量改价 | ⚠️ 技能+fixtures+oracle；迭代改价 UX 待产品化 |

**共用体验 P0（工程，已落地）：** 见 [office-mode-iteration-plan.md](../office-mode-iteration-plan.md) 实施顺序 1–8 — `read_office`（calamine）、默认 `deliverables/` + 高亮、PDF/HTML 右栏预览、Office 系统打开、`load_office_payload`、`write_office` `source` 直喂。

---

## 7. 分阶段路线图

### Phase A — 文字闭环（当前，~90%）

- ✅ 11 技能 + 11 卡片 + P0 fixtures + oracle（P0-2/3/4）  
- ✅ 不依赖 STT/TTS  
- 目标：任意场景 **一句话 → 可下载文件**  
- **剩余：** P0 端到端跑绿常态化、`office-skill-lint`、附录 A 余 5 技能、读表保真 golden

### Phase B — 数据与迭代（远期，本期范围外部分见下）

- `inbox/`、`data/` 目录约定 — 用户自建或复制 fixtures（**不自动初始化**）  
- 🔮 MCP 接 ERP / CRM / 公告（可选，**本期不做**）  
- ⚠️ `load_office_payload` 增量改报价 / 改报表 — 工具有，流程产品化待做  
- 🔮 定时任务（background automation）做「每日竞品摘要」

### Phase C — 语音（入座 briefing，本期范围外）

- STT / TTS 触发与口播概况 — **本期不做**；架构上仅替换 `loop`，不触碰摄取/处理/生成  
- 见 `doc_Private/docs/desktop/DEV_NOTES.md` §入座 briefing

---

## 8. 能力差距（办公线横切）

> **架构含义：** 下列缺口落在 **L2 能力原语**（§3）或 **L4 验收**，与具体场景解耦。补一次原语，所有用到该轴的场景同时受益。  
> **本期不计入差距：** STT/TTS（Phase C）、ERP/CRM connector、`inbox/`/`data/` 自动初始化。

| 缺口 | 影响场景 | 状态 | 参考 |
|------|----------|------|------|
| XLSX 读取保真（numFmt golden 等） | 生产、报价、财务、老板汇总 | ⚠️ `read_office`+calamine 已上线，稳定性待验证 | office-mode-iteration-plan §P0 R1 |
| 迭代式修改产品化 | 报价改价、报表改列 | ⚠️ `load_office_payload` 工具有，UX/技能步骤待定型 | office-mode-iteration-plan §P0-4 |
| 企业模板 | 报价、合同、简报 | ❌ `templates/` 仅约定 | §G |
| 一键导出 PDF | 外发 | ❌ backlog | office-mode-iteration-plan §15 |
| round-trip 手改文件 | 用户改过的 docx/xlsx 再改 | ❌ 仅 payload 缓存路径 | office-mode-iteration-plan §P0-4 进阶 |
| 来源 / 幻觉约束 | 市场、竞品、政策 | ⚠️ `office-competitive-analysis` 技能已要求来源 | §11 |
| 扫描件 OCR | 扫描 PDF、发票图片 | ✅ **视觉桥接** — `describe_image`；`read_office` 不内置 | office-read-tool-plan §OCR |
| 生成后预览 / 高亮 | 全部 | ✅ 默认 `deliverables/` + 高亮；PDF/HTML 右栏；Office 系统打开 | office-mode-iteration-plan §F |
| P0 端到端 oracle 常态化 | 四轴验证 | ⚠️ `office-demo-oracle.ps1` 有 P0-2/3/4 | §6 |

---

## 9. 与商业化 / 潜力的关系

- **编码 harness（LHT/CRAFT）** → 开发者口碑、长程可靠性  
- **办公场景地图（本文）** → 非开发者可理解、可演示、可行业化（制造业、商贸、市场团队）  
- 商业化可沿 **「Skill 模板包 + 数据连接器 + 语音简报」** 展开，与 BYOK 不冲突  

---

## 10. 后续文档与实现入口

| 动作 | 落点 |
|------|------|
| 新建技能 | `crates/runtime-server/assets/skills/office-<name>/SKILL.md`（含 `## 技能契约`） |
| **P0 样板技能** | [`office-executive-daily-brief`](../../crates/runtime-server/assets/skills/office-executive-daily-brief/SKILL.md)（契约 schema 参考） |
| 任务卡片（11 张） | `OfficeEmptyState.tsx` + `web-ui/src/i18n/locales/*.ts` → `officeEmpty` |
| Office 能力迭代 | [office-mode-iteration-plan.md](../office-mode-iteration-plan.md) |
| Demo fixtures | [`docs/harness/fixtures/office-demo/`](../harness/fixtures/office-demo/README.md) |
| P0 oracle | [`scripts/office-demo-oracle.ps1`](../../scripts/office-demo-oracle.ps1) |
| 契约 lint（待建） | `scripts/office-skill-lint.mjs`（可选） |
| 语音（Phase C，范围外） | `doc_Private/docs/desktop/DEV_NOTES.md` §2026-05-18 入座 briefing |

---

## 附录 A：技能命名建议（含 P0 已建）

| 技能 ID | 场景 | 状态 |
|---------|------|------|
| `office-executive-daily-brief` | 老板 / 高管经营日报 | ✅ P0-2 |
| `office-production-daily-report` | 生产 + 品质晨报 | ✅ P0-3 |
| `office-customer-quote` | 客户报价单 | ✅ P0-4 |
| `office-market-watch` | 市场日报 / 行业动态 | ❌ 待建（P0-1 暂用 `office-competitive-analysis`） |
| `office-sales-daily` | 销售日报 | ❌ 待建 |
| `office-decision-memo` | 决策备忘录 | ❌ 待建 |
| `office-incident-report` | 品质 / 运营异常报告 | ❌ 待建 |
| `office-proposal` | 商务提案书 | ❌ 待建 |

命名与现有 `office-*` 保持一致；描述行注明格式与默认 `deliverables/`。

**编写方式：** 每个新技能按 §3.2 的**技能契约**填四轴 + `verify` 即可，不写新引擎逻辑。以 `office-executive-daily-brief` 为样板；P0 三条已按同结构落地，余 5 个按附录补齐。
