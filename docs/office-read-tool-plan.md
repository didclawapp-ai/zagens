# read_office 工具设计方案

> 状态：已实施（`read_office` + calamine XLSX 首版，2026-06-03）
> 涉及 crate：`crates/runtime-server`（新增 `read_office` 工具 + `calamine` 依赖）
> 关联：`docs/office-mode-iteration-plan.md`（P0 读取与摄取保真）、`docs/office-doc-capability-plan.md`

---

## 1. 背景与动机

办公任务的起点几乎都是“先读懂已有文件”——“分析这个 Excel”“总结这份合同”
“把这个 PDF 整理成纪要”。读不准，后续分析全错。

当前办公文档读取**寄生在 `read_file` 里**（`crates/runtime-server/src/tools/file/read.rs`），
用正则解析 OOXML，保真度与健壮性不足：

- **XLSX**：日期显示为序列号（`45678`）、货币/百分比丢格式、公式丢失、空单元格错位、
  **整表读入内存无分页**（大文件撑爆上下文/OOM，绕过了纯文本路径的 `MAX_FILE_SIZE` + 分页）。
- **DOCX**：表格被压成纯文本，行列结构丢失。
- **PPTX**：不读演讲者备注（`notesSlide`）、不读表格/图表数据。
- **PDF**：扫描件抽取为空（已有 `describe_image` 视觉工具可作 OCR 路径）。
- **完全不支持**：CSV/TSV 结构化、`.xls`/`.ods` 旧/开放格式。

`read_file` 的职责本应聚焦 **code / 纯文本**（分页、编码嗅探、符号摘要等都是为代码场景设计的）。
因此新建一个**专门读办公文档的工具 `read_office`**，把办公读取从 `read_file` 解耦出来。

---

## 1.5 真实测试验证（2026-06 实机）

在含 6 个真实办公文件的工作区实测 `read_file`，结果如下：

| # | 文件 | 格式 | 状态 | 问题 |
|---|------|------|------|------|
| 1 | 排母_良率统计.xlsx | XLSX | ❌ | 只读到工作表名，单元格全空 |
| 2 | 功能檢測報告.docx | DOCX | ❌ | 输出原始 OOXML 标签，未清洗为纯文本 |
| 3 | 良率分析报告.pptx | PPTX | ✅ | 6 页正文/数据/结论完整 |
| 4 | LAPEROS SDS(日文).pdf | PDF | ✅ | 5 页完整 |
| 5 | PBT 4830 SDS(中文).pdf | PDF | ✅ | 完整 |
| 6 | 膜厚复测.pdf | PDF | ⚠️ | 扫描/图片型，仅两行文字层，需 OCR |

**结论与现行代码一致**：问题集中在 **XLSX**、**DOCX**、**扫描型 PDF（OCR）** 三处；PPTX 与文本型 PDF 路径正常。

### XLSX 失败的根因（已在代码中定位）

`read_xlsx` 解析单元格的正则（`crates/runtime-server/src/tools/file/read.rs`）：

```
<c r="([A-Z]+)(\d+)"(?:\s+t="([^"]*)")?>(?:<v>([^<]*)</v>)?</c>
```

它要求 `<c r="A1"` 后**紧跟** `t="..."` 再 `>`。但真实 Excel 单元格几乎都带样式属性：
`<c r="A1" s="3" t="s"><v>0</v></c>`——`r` 与 `t` 之间夹了 `s="3"`，**正则整体匹配失败**，
导致**所有带样式（即绝大多数）单元格读不到**。`<c>` 内含 `<f>` 公式时同样不匹配。
sheet 名来自 `workbook.xml` 另一条正则，所以表现为“只剩工作表名”。

> 这是**确定性 bug**，不是偶发——正是用 `calamine` 替换正则、按类型读单元格的最有力证据。

### DOCX 输出原始标签

实测 DOCX 吐出 OOXML 标签而非纯文本，需在 `read_office` 中走稳健的 OOXML 解析
（剥标签 + 保留 `<w:tbl>` 表格结构 + 标题层级）。
（注：现行 `read_docx` 含 `<w:t>` 抽取逻辑，实测异常需在实现时复现确认是否边界场景或构建差异；
新工具无论如何都应稳健处理。）

### 技能（skill）不能替代

实测加载的 `xlsx` 技能提供 pandas/openpyxl 方案，但**依赖 `exec_shell`**——
Office 会话已裁掉 shell，技能在办公会话内无法执行。
**因此必须在 Zagens 侧用纯 Rust 把读取能力补齐**，而非依赖 Python 技能。

---

## 2. 已确认的设计决策

| 决策 | 结论 | 理由 |
|------|------|------|
| `read_file` 现有办公处理 | **保持不动，作为兜底** | 零回归；旧调用仍可用，新工具为高保真首选 |
| 新工具命名 | `read_office`（文件 `office_read.rs`） | 与 `office_write` / `write_office` 对称 |
| XLSX 引擎 | 引入 **`calamine`**（纯 Rust） | 类型化单元格 + 日期/格式还原 + 分页 + 顺带支持 .xls/.ods |
| 首版范围 | **XLSX 高保真优先**，docx/pptx/pdf/csv 先接基础逻辑再增强 | 收益最大处先做 |
| OCR | 复用现有 `describe_image` | 不在 `read_office` 内置 OCR |

### calamine 体积评估（0.35.0，MIT，MSRV 1.83）

- 源码包 ~133 KB；编译进二进制增量约**几百 KB ~ 1MB**，无系统库、无运行时负担。
- 共享依赖：`encoding_rs`（已用）、`serde`/`log`（基本已在）。
- 新增传递依赖：`quick-xml` 0.39（主要体积）、`byteorder`/`codepage`/`atoi_simd`/`fast-float2`（极小）。
- **`zip` 版本差异**：项目用 `zip = "2.2"`，calamine 要 `zip = "7.0"`。
  - 默认方案：**放任两个 zip 版本共存**（增量小、最省事）。
  - 可选清理：统一升 `zip` 到 7.0 去重，但会动到现有 `office_write` / `read_file`，需回归测试。
- 需启用 calamine 的 `dates`（= `chrono`）特性以拿到日期类型。

---

## 3. 工具接口设计

### 3.1 名称与能力
- 工具名：`read_office`
- 能力：`ReadOnly` + `Sandboxable`，`supports_parallel = true`（与 `read_file` 一致）。
- 注册：仅进 **office surface**（`with_office_surface`，见
  `crates/runtime-server/src/tools/registry.rs`），不改 `read_file` 的注册。

### 3.2 入参

```jsonc
{
  "path": "报表.xlsx",        // 必填，相对工作区或绝对路径
  "sheet": "月度汇总",         // 可选，XLSX：指定 sheet（名或 0-based 索引）；缺省读全部/第一个
  "pages": "1-5",             // 可选，PDF：页范围（沿用 read_file 语义）
  "start_row": 1,             // 可选，XLSX/CSV：起始数据行（1-based，分页）
  "limit": 2000               // 可选，最大行数（默认 2000，超限给“还有 N 行”提示）
}
```

> 路径解析仍走 `context.resolve_path()`（既有 `..` 逃逸防护 + 信任路径）。

### 3.3 格式分派

```
read_office(path)
  ├── .xlsx / .xls / .xlsb / .ods → calamine（高保真）
  ├── .docx                        → ZIP+XML，保留表格/标题层级（增强自现有 read_docx）
  ├── .pptx                        → ZIP+XML，正文 + notesSlide + 表格（增强自现有 read_pptx）
  ├── .pdf                         → 复用 read_pdf（pdftotext / pdf-extract）；扫描件提示 describe_image
  ├── .csv / .tsv                  → 结构化为表（表头 + 行列数）
  └── 其它 / .doc / .ppt(旧二进制)  → 明确错误 + “请另存为 .docx/.pptx 或用 read_file”
```

---

## 4. 输出与保真要点

### 4.1 XLSX（calamine，头号收益）
- 按类型读单元格：`Int / Float / String / Bool / DateTime / Error / Empty`。
- **日期还原**：序列号 → `2025-01-15`（启用 `dates`/`chrono` 特性）。
- 货币/百分比：保留可读值（必要时结合 numFmt 信息，calamine 提供格式串）。
- **列对齐**：空单元格占位，宽表行列对齐，便于模型判断“哪列对哪列”。
- **分页**：`start_row` + `limit`，超限提示续读；规避大表 OOM。
- metadata：sheet 列表、每 sheet 行列数、当前 sheet、是否截断。

输出示例：

```
=== Sheet: 月度汇总 (12 行 × 3 列) ===
| 月份    | 销售额      | 增长率  |
| 2025-01 | ¥150,000   | 12.5%  |
| 2025-02 | ¥180,000   | 20.0%  |
...（共 12 行，已显示 1-10；start_row=11 续读）
```

### 4.2 DOCX
- 识别 `<w:tbl>` → 输出为表格（行列保留）。
- 保留标题层级（`<w:pStyle>`）与列表编号，便于“按章节总结/改写”。
- 分页/大小上限，避免长文档撑爆上下文。

### 4.3 PPTX
- 正文 `<a:t>` + **演讲者备注 `notesSlide`**（备注常是真正内容）+ 表格单元格。
- 标注 slide 归属与序号。

### 4.4 PDF
- 复用现有 `read_pdf`；扫描件（抽取为空）给明确提示：改用 `describe_image` 做 OCR。

### 4.5 CSV/TSV
- 按分隔符解析为表，给表头 + 行列数；分页同 XLSX。

---

## 5. 大文件防护（跨格式）
- 统一接入大小上限（对齐 `read_file` 的 `MAX_FILE_SIZE`）+ 行/页分页。
- 超限不崩溃：返回结构摘要 + “用 start_row/limit 接续”提示。
- 这是修复当前 office 读取路径**完全绕过防护**的关键。

---

## 6. 文件改动清单

```
新建:
  crates/runtime-server/src/tools/office_read.rs   read_office 工具实现

改动:
  crates/runtime-server/Cargo.toml                 新增 calamine = { version = "0.35", features = ["dates"] }
  crates/runtime-server/src/tools/mod.rs           pub mod office_read;
  crates/runtime-server/src/tools/registry.rs      with_office_surface() 注册 ReadOfficeTool
  crates/runtime-server/src/prompts/tasks/office.md 引导优先用 read_office（read_file 仍可兜底）

不动:
  crates/runtime-server/src/tools/file/read.rs     现有办公格式处理保留为兜底

实施后记录:
  CHANGELOG.md                                     新增 read_office 工具（XLSX 高保真读取）
```

---

## 7. 实施顺序

1. 加 `calamine` 依赖（默认两个 zip 版本共存），`cargo build` 验证依赖树。
2. 新建 `office_read.rs`：先实现 **XLSX 高保真**（calamine + 日期/格式还原 + sheet 选择 + 分页）。
3. 接入 DOCX/PPTX/PDF/CSV 基础逻辑（可复用/迁移 `read.rs` 的提取函数 + 增强表格/备注）。
4. 注册到 office surface；更新 `office.md` 提示词。
5. 加 smoke 测试：XLSX 日期/数字/多 sheet/大文件分页；DOCX 表格；PPTX 备注。
6. 记 CHANGELOG。

---

## 8. 验收标准

- “分析这个 Excel”：日期是日期、百分比是百分比、宽表列对齐、公式可见、大文件能分页读完不崩溃。
- “总结这份合同”：DOCX 表格行列与章节层级正确。
- “总结这个 PPT”：能读出演讲者备注与表格数据。
- 扫描件 PDF / 旧版 `.doc`/`.ppt`：给明确的下一步提示，而非空结果或裸错误。
- `read_file` 行为不变，旧调用零回归。
