# PPTX 通用专业生成引擎方案

> 状态：规划中（与已实现脚本对齐的基线与路线）  
> 更新：2026-05-11；修订稿纳入 blocks 优先级、模板与 matplotlib 安全边界、字体链、Phase 1 模块化与验收细化  
> 关联：[文档读写能力增强方案](./office-doc-capability-plan.md)（`write_office` / venv / 脚本投递）  
> 脚本基线：`crates/tui/assets/scripts/write_pptx.py`

---

## 1. 目标与范围

### 1.1 目标

将当前「单文件 JSON → PPTX」桥接，演进为 **面向常用职场/报告场景的通用生成引擎**：在 **`python-pptx` + JSON 驱动** 前提下，支持版式、多图、嵌入图、段落级富文本，并为 8D、质量报告、项目周报等场景预留模板与复杂图（matplotlib → PNG）。

### 1.2 非目标（当前阶段不强求）

| 条目 | 说明 |
|------|------|
| 原生 SmartArt | 手写 OOXML 成本高；可用示意图 PNG 或简单形状替代 |
| 复杂动画 / 幻灯片切换 | ROI 低，发布会类需求延后 |
| 纯 Rust PPTX 引擎 | 与总体方案一致，PPTX 仍以 Python 为唯一路径 |

### 1.3 约束

- **对外形态**：仍为 LLM 友好的 **单一 JSON**，渐进增强字段，尽量不破坏现有 `slides[]` / `chart` / `table` / `bullets` 语义。
- **运行时**：继续使用专用 venv 与嵌入式脚本投递（参见 `office-doc-capability-plan` 中的生成与 Python 小节）。
- **复杂图**：鱼骨、甘特、雷达等以 **matplotlib（Agg）渲染 PNG 再嵌入** 为主路径，降低 chart XML 组合成本。

---

## 2. 基线盘点（与仓库脚本一致）

以下描述与 `write_pptx.py` 当前实现对齐。

### 2.1 文档级

| 能力 | 支持 |
|------|------|
| `title` / `subtitle` | 有则生成封面，无则无封面 |
| 全局 `theme` | 预设 `dark \| light \| warm \| minimal`，或自定义颜色 + 字体 dict |
| `slides[]` | 必填；每项对应一页「内容幻灯片」 |

### 2.2 单页内容（固定垂直流水线）

单页可同时包含的子块及 **排版顺序**：**表格（若存在）→ 图表（若存在）→ 要点列表（若存在）**。位置与宽高大量硬编码（英寸），不支持双栏或「左图右文」等分区。

### 2.3 主题

| 能力 | 支持 |
|------|------|
| 单页 `theme` 覆盖 | 支持（解析失败时**静默退回**全局主题） |
| 自定义主题继承 | 字典路径完整替换颜色；预设名路径从 `THEMES` 读取，自定义 table 调色与字典路径不对称（可接受但宜文档化） |

### 2.4 图表

| 类型 | `type` 取值 |
|------|-------------|
| 柱状 / 折线 / 饼 / 堆积柱 / **100% 堆积柱** / 面积 / 散点 / 环形 | `bar`, `line`, `pie`, `stacked_bar`, **`stacked_bar_pct`**, `area`, `scatter`, `donut` |

可选：`chart_title`, `x_label`, `y_label`, `data_labels`。  
**约束：每页仅一个 `chart`。**

### 2.5 表格

- `headers` + `rows`；表头粗体；表体斑马纹；列宽均分。
- **无合并单元格**；单元格内为纯文本单行段落级样式。
- 行数 **上限 18**（含表头），超出静默截断。

### 2.6 文本

- `bullets`：字符串数组；每条为 **整段同色、同字号**（无双 font 粒度混排）。
- 页标题：可选 `title`。

### 2.7 备注与页码

| 能力 | 支持 |
|------|------|
| 演讲者备注 | 字段 `notes`（在 `slide.has_notes_slide` 为真时写入） |
| 页脚序号 | 右下角 **`当前页码/总页数`** 文本（总页逻辑含致谢页及封面计数规则固定） |

> 说明：`write_pptx.py` 文件头 docstring **未写明** `notes` 与 `stacked_bar_pct`，与实现不一致，建议在后续文档与注释中补齐。

### 2.8 已知实现细节债

- `_add_table` 返回的高度与 `content_top` 的累加使用了 **混合单位换算**（`Pt` 与除以 `914400` 并用），后续引入布局引擎时应 **统一到 EMU 或英寸**，避免版面漂移。
- `resolve_theme(slide_theme, fallback=global_t)` 在 `slide_theme` 为 **字符串**时不会用到 `fallback` 的逐项合并（行为可接受但应在规范中写明，避免模型误组合）。

---

## 3. 能力缺口（按优先级）

### 3.1 高频（专业幻灯片常备）

| 缺口 | 影响场景 |
|------|----------|
| 图片嵌入（PNG/JPG；SVG 需转栅格或单列支持） | 产品图、现场照、Logo、架构图 |
| 段落级富文本（同段多字号/颜色/加粗） | 强调数字、红黄绿状态、条款引用 |
| 多图表同页 | 柏拉图、对比看板、一页多 KPI |
| 灵活布局（双栏 / 栅格 / 图配文） | 大部分非「标题+要点」页 |

### 3.2 中频

| 缺口 | 影响场景 |
|------|----------|
| matplotlib 复杂图（鱼骨、FTA、甘特、雷达、漏斗等）→ PNG | 8D、质量报告、项目管理 |
| 基于企业 `.pptx` 模板占位填充 | VI 固定、页眉页脚 Logo |
| 表格合并单元格、条件底色 | FMEA、控制计划、评审矩阵 |

**Phase 2 模板占位（设计约束，`python-pptx` 现实边界）**

- `python-pptx` **没有**「按占位符 ID 一键替换」的高层 API；须在 **模板加载后遍历** `slide.shapes`（及 `shape.text_frame` 文本）匹配约定串或 shape 属性。
- **Phase 2 先做窄切口**，避免一上来做「表格内置占位」等粗粒度很痛苦的路径：

| 能力 | Phase 2 首期 | 明确延后 |
|------|----------------|----------|
| 文本框占位 | 文本中包含 `{{title}}`、`{{date}}`、`{{author}}` 等 **固定 key**，引擎做纯字符串替换 | — |
| 图片占位 | 按 **shape 名称** 匹配（如 `LOGO_PLACEHOLDER`），替换为传入路径的图片 | — |
| 版式起点 | **克隆模板中已有幻灯片**（duplicate）再填空，不从空白 `slide_layouts[6]` 从零画 VI | — |
| 模板内表格「按格填数据」 | — | **先不做**：表格操作粒度粗、行列与合并状态难与 JSON 对齐，留到表格模块增强后再论证 |

详见第 7 节：模板相关 **不得** 执行 LLM 提供的任意 Python。

### 3.3 低频但仍可规划

| 缺口 | 说明 |
|------|------|
| SmartArt 等效 | 形状 + PNG 示意图优先 |
| 正式页眉页脚「域」 | 模板占位 + 占位符比在空白版式上纯手工画更稳 |
| 动画与转场 | 明确延后 |

---

## 4. 架构方针

不建议在单一脚本上无限堆特性。推荐在 **`crates/tui/assets/scripts/`** 下拆出包目录，入口薄封装。

### 4.1 Phase 1 目录（控制在 5 个模块内）

初期 **避免拆分过细**（易 import 循环、跨模块协调成本高于代码量）。可合并为：

```
crates/tui/assets/scripts/
├── write_pptx.py           # CLI：stdin / --input → argparse --output；极薄，转调引擎
└── pptx_engine/
    ├── __init__.py         # build_presentation() 唯一对外入口（供 write_pptx 调用）
    ├── theme.py            # resolve_theme、预设、`THEMES`（从现行脚本迁入）
    ├── layout.py           # 栅格 / 分栏 / 区域盒；EMU‑Inches 统一；封面与内容页拼装入口
    ├── blocks.py           # 块渲染：richtext（runs）、image、table、bullet 段落等（旧 _add_* 迁入）
    └── charts.py           # 原生 OOXML chart；多图同页占位、combo 策略（柏拉图等）
```

**Phase 2 再切分**：当 `matplotlib`、企业模板、表格合并体量上来后，再从 `charts.py` 拆出 **`mpl.py`（声明式 matplotlib）**、`blocks.py` 拆出 **`template.py`**、表格增强 **`tables.py`** 等——与第 7 节安全边界对齐。

### 4.2 原则

1. **JSON 驱动**：一种 schema，多种 block；模型只填语义块，少用裸坐标。
2. **渐进增强**：`blocks` 与旧字段 **优先级固定**（见第 5.0 节）；须同步写入 **WriteOfficeTool 描述**与模型提示，避免临场发挥。
3. **布局先于坐标**：栅格 / 分栏 **`weights` + `gap` + `padding`**，由引擎计算几何。
4. **复杂 OOXML**：柏拉图类可走双图并排或后续 combo；**matplotlib 仅走声明式 JSON**（第 7.1 节）。

---

## 5. JSON 设计草案（Phase 1 方向）

以下仅为约定方向，正式实施时在 PR 中与模型提示词、**工具描述**同步收口。

### 5.0 `blocks` 与旧字段：单页优先级与错误回退

**单页输入优先级（须写进契约，LLM 与工具字段说明一致）：**

| 条件 | 行为 |
|------|------|
| 该页 **`blocks` 存在且为非空数组** | 走 **layout + blocks** 管线；**忽略** 同页的 `chart`、`table`、`bullets`（避免双轨叠床架屋）。页级 `title` / `notes` / `theme` 等元字段仍按需生效（具体以 PR 收口为准）。 |
| **`blocks` 不存在、null、省略或空数组 `[]`** | 走 **现行旧流水线**：`chart` → `table` → `bullets` 垂直堆叠（与当前 `write_pptx.py` 兼容）。空数组不占新管线，避免无意中「挂了 blocks 键却没有块」。 |

**错误处理（提高鲁棒性，避免一页 schema 小问题拖死整份 deck）：**

- 若检测到 **`blocks` 管线解析或布局失败**（未知 `block.type`、缺必填键、几何不可解等）：**当前页回退至旧流水线**（使用同页上的 `chart` / `table` / `bullets`，若亦无则生成仅标题页或占位说明），并向 **stderr 输出可解析告警**（含 `slide_index`）。
- **禁止**静默吞掉整块 deck；回退路径须在开发阶段用 fixture 覆盖。

**与并行字段：** 建议在规范中约定：选择 `blocks` 的页面 **不要再填** `chart`/`table`/`bullets`；若填满，仅以优先级为准——便于人类审 JSON 时也一眼看出意图。

### 5.1 布局块（示意）

```json
{
  "layout": { "kind": "grid", "cols": [0.42, 0.58], "gap": "0.3in", "padding": "0.55in" },
  "blocks": [
    { "type": "richtext", "runs": [{"t": "良品率 ","size":14},{"t":"96.2%","bold":true,"color":"#00AA66","size":18}] },
    { "type": "chart", "chart": { "type": "bar", "...": "..." } }
  ]
}
```

### 5.2 富文本

- **推荐**：`runs[]`，每项 `{ "t", "bold?", "italic?", "size?", "color?" }`，颜色 hex 与现有一致。
- **可选**：受控 Markdown 子集——需单独词法与 SSRF/路径注入无关，但以 **runs 更简单可测**。

### 5.3 图片

- `{"type":"image","path":"...","fit":"contain"|"cover","max_height":"4.5in"}`
- **路径安全**：与工具层一致，必须先经已有 **canonicalize、禁止 `..` 逃逸** 规则；仅存档路径或占位符需在 Rust 工具侧校验后再传入脚本。

### 5.4 多图表

- `"charts": [ { ... }, { ... } ]` 或由 `blocks` 中多个 `type: chart` 表达；引擎负责垂直或栅格占位，禁止模型直接写重叠坐标。

---

## 6. 分阶段路线

| 阶段 | 交付内容 | 验收建议 |
|------|----------|----------|
| **Phase 1** | 第 4.1 节模块拆分 + 栅格 + 嵌入图 + runs 富文本 + 单页多图；`blocks`/旧流水线优先级与回退（第 5.0 节）；JSON 兼容 | **两个端到端 JSON fixture**（见下表）；生成 PPTX **目视合格**即过线 |
| **Phase 2** | **声明式** matplotlib（引擎内翻译成 `matplotlib` API，禁止 `exec` LLM 代码）、企业模板占位（第 3.2 节）、从 `pptx_engine` 再拆 `template.py` / `mpl.py` 等、`tables.py` 合并格与条件色起步 | 8D / 周报补全matplotlib；企业阉割版 `.pptx` 跑通文本 + Logo 占位 |
| **Phase 3** | 场景模板库（8D、周报、竞品、年度总结等）——**预设 JSON + 文档**，减少临场结构发散 | 用户仅填数据字段即可生成骨架 |

### 6.1 Phase 1 端到端验收（固定 fixture）

建议在仓库 **`crates/tui/assets/scripts/fixtures/pptx/`**（或 `tests/fixtures/`）落两份 **金样 JSON**，CI 或本地脚本对每个 fixture 跑一次 `write_pptx.py --output *.pptx`，发布前目视或截图回归。

| 验收用例 | 页数 / 形态 | 须覆盖的能力 |
|----------|-------------|----------------|
| **项目周报** | 5 页 | 封面；**KPI 双图**同页（或双 `chart` 块）；**风险登记表**（`table` 或 table 块）；**里程碑甘特**（见下注）；团队 / 致谢或收尾 |
| **竞品分析** | 单页 | **双栏**：左 **richtext**（关键数字高亮）；右 **图 / Logo** 嵌入 |

**甘特页与 Phase 边界**：周报第 4 页若需 **真正的甘特**，依赖声明式 matplotlib（通常为 **Phase 2**）。两种方式二选一写进路线图，避免口径悬空：

1. **推荐**：Phase 1 fixture 该页暂用 **声明式占位**——例如 `blocks` 里 `type: "image"` 指向仓库内极小 **占位 PNG**，或单列 **条形图** 近似里程碑；字段名与 Phase 2 的 `mpl_gantt`（示例名）对齐，后端升级时 **不换 JSON 形状**。  
2. **折中**：Phase 1 末尾专门排一小 Sprint，把「仅甘特一类的声明式 mpl」划入 Phase 1，则周报五页全开。

选型在开工 Phase 1 的 PR 里 **拍板**，本方案两处引用保持一致即可。

---

## 7. matplotlib、模板与安全边界

### 7.1 信任模型：不向 LLM 暴露 `exec()`

| 层级 | 谁执行 | 内容 |
|------|--------|------|
| LLM（经工具 payload） | 仅 **声明式 JSON** | 图表类型（如 `gantt`、`fishbone`）、数据结构、调色板、字号、标题等 **允许键白名单** |
| 引擎（`pptx_engine` 内，`mpl.py` / `charts.py` 扩展） | **固定翻译层** | 将 JSON **映射到受控 matplotlib 调用**（或预置函数分支），产出 PNG → `blocks`/`image` 嵌入 |
| 禁止 | — | LLM 提供 **matplotlib / 任意 Python 源码**由引擎 **`exec` / `eval`** 运行（与 **RLM 沙箱** 问题同类；本工具栈 **不采纳**）。 |

若在 Phase 2 引入「模板占位填充」脚本：**同样**只允许配置驱动的查找替换与图片替换，不接任意代码字符串。

### 7.2 运行环境（venv、后端）

- 后端强制 **`matplotlib.use("Agg")`**；无 DISPLAY 依赖。
- **依赖**：`matplotlib` 为 office venv **可选依赖**或 extra；导入失败时 **stderr** 明示「未安装 matplotlib / 不可用」，并可走「仅原生 chart」或占位图路径。
- 调用方：**仅引擎代码**在用户 venv 中 import matplotlib；用户对 **matplotlib API 无刻写面**，只有 JSON。

### 7.3 中文字体降级链（具体策略）

引擎初始化 matplotlib 字体时做一次 **`matplotlib.font_manager` 探测**（可缓存探测结果于一进程生命周期），并按序选用 **第一份已安装**的字体作为主要 CJK sans：

1. **`font`／`mpl_font`**：payload 顶层或块级传入的用户指定字体名（若在下列链中不可用则忽略该项继续向下）。  
2. **`Microsoft YaHei`**（Windows）  
3. **`PingFang SC`** / **`Heiti SC`**（macOS，依序尝试）  
4. **`Noto Sans CJK SC`**（Linux / 常见 Docker 镜像）  
5. **`SimHei`**（Windows 备选）  
6. **全链不可用**：图表内 **类别轴与标题降级为英文标签**（或由 payload 提供 **纯 ASCII** 缩写）；**stderr** 发出一次 **warning**；若该页有 `notes` 或可写 footer 元信息，**append 一条简短说明**（告知「CJK 字体未检测到，标签已降级」），避免用户只见「口口」却不知原因。

PPTX 正文框内字体与 matplotlib 图内字体 **可分离配置**；正文仍跟 `theme.font`，图内跟上述链。

---

## 8. 嵌入式脚本与 Cargo 集成

- 继续使用 `include_str!` / 版本戳落盘逻辑；若改为包目录，需更新 **嵌入 glob 或与 build 脚本同步**，保证升级时 **`pptx_engine/` 全量写入**。
- `WriteOfficeTool` 描述须显式包含：**第 5.0 节**（`blocks` 与旧字段优先级、单页失败回退）、**第 7.1 节**（仅声明式 JSON，无 `exec`）、以及 Phase 1 / 2 字段差异（在具体 PR 里改）。

---

## 9. 小结

当前 `write_pptx.py` 已覆盖 **主题、多类图表、基础表、要点、致谢、备注、百分比堆积柱**，但 **缺少版式自由度、嵌入图、富文本与多图**，与企业模板能力。本修订稿补充了 **`blocks` 契约、鲁棒回退、matplotlib/模板安全边界、字体探测链、Phase 1 五模块切分与双 fixture 验收**，便于与「全靠 skill 现场写脚本」路线形成稳定对比：高频文档能力以 **版本化引擎 + 白名单 JSON** 交付，而不是把执行面交给不可审计的即兴代码。
