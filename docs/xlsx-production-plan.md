# XLSX 文件生产级生成改进方案

> **日期**: 2025-05-11
> **涉及文件**: `crates/tui/src/tools/office_write.rs`
> **依赖**: `rust_xlsxwriter = "0.81"`（已有）

---

## 1. 现状分析

### 1.1 架构

```
write_office Tool (office_write.rs)
├── format = "xlsx"  →  generate_xlsx()  ← 纯 Rust，无 Python 依赖 ✅
├── format = "docx"  →  generate_docx()  → Python 优先 + Rust 兜底
└── format = "pptx"  →  generate_pptx()  → Python 必须
```

XLSX 是唯一**零 Python 依赖**的格式，由 [`rust_xlsxwriter`](https://crates.io/crates/rust_xlsxwriter/0.81) crate 在 `spawn_blocking` 中同步生成。

### 1.2 当前代码

[`generate_xlsx`](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/tools/office_write.rs#L165-L227) — 64 行：

```rust
fn generate_xlsx(input: &Value, path: &PathBuf) -> Result<String, String> {
    use rust_xlsxwriter::*;
    let mut workbook = Workbook::new();

    for sheet_val in sheets {
        let worksheet = workbook.add_worksheet();
        worksheet.set_name(&name)?;
        for (row_idx, row_val) in rows.iter().enumerate() {
            for (col_idx, cell) in row.iter().enumerate() {
                match cell {
                    Value::Null => {}
                    Value::Number(n) => worksheet.write(row, col, i/f/...),
                    Value::String(s) => worksheet.write(row, col, s.as_str()),
                    Value::Bool(b)   => worksheet.write(row, col, *b),
                    _ => worksheet.write(row, col, cell.to_string()),
                }
            }
        }
    }
    workbook.save(path)?;
}
```

### 1.3 当前输入 schema（简化）

```json
{
  "format": "xlsx",
  "path": "output.xlsx",
  "sheets": [
    {
      "name": "Sheet1",
      "rows": [
        ["Name", "Age", "Score"],
        ["Alice", 30, 95.5],
        ["Bob", 25, null]
      ]
    }
  ]
}
```

---

## 2. 生产级 vs 现状 差距分析

| 功能维度 | 现状 | 生产级要求 | 差距 |
|----------|------|-----------|------|
| **表头行样式** | ❌ 裸数据 | 加粗 + 背景色 + 冻结 | 🔴 |
| **列宽自适应** | ❌ 无 | 根据内容自动计算（含 CJK 宽度） | 🔴 |
| **数字格式** | ❌ 裸值 | 货币/百分比/日期/千分位 | 🔴 |
| **边框线** | ❌ 无 | 数据区全 thin 边框 | 🔴 |
| **自动筛选** | ❌ 无 | autofilter 表头范围 | 🔴 |
| **交替行底色** | ❌ 无 | 偶数行浅色背景 | 🟡 |
| **文本换行 + 垂直对齐** | ❌ 无 | `wrap: true` 列自动换行 | 🟡 |
| **合并单元格** | ❌ 无 | 文档标题行跨列合并 | 🟡 |
| **公式** | ❌ 无 | SUM / AVERAGE / IF 等 + `{{row}}` 模板 | 🟡 |
| **图表** | ❌ 无 | 柱状/折线/饼图 | 🟡 |
| **条件格式** | ❌ 无 | 数据条/色阶/图标集 | 🟢 |
| **打印设置** | ❌ 无 | A4/横向/缩放到页/页眉页脚 | 🟢 |
| **数据验证** | ❌ 无 | 下拉列表/数值范围 | 🟢 |
| **多类型单元格** | 3 种 (数/字/布尔) | 日期/百分比/超链接 | 🟡 |
| **工作表保护** | ❌ 无 | 只读密码保护 | 🟢 |

🔴 = 核心缺失，严重影响生产可用性
🟡 = 常见需求，显著提升质量
🟢 = 锦上添花，按需迭代

---

## 3. 设计决策

### 3.1 核心理念：默认智能 + 显式覆盖

| 层级 | 描述 |
|------|------|
| **零配置** (当前) | `rows: [[...]]` → 自动美化（表头加粗+背景色+边框+列宽自适应） |
| **显式配置** (新增) | `columns: [{...}]` 控制列宽/格式/公式；`style: {...}` 控制主题/边框/冻结 |
| **图表/条件格式** (新增) | `charts: [{...}]` / `conditional_formats: [{...}]` |

**向后兼容**：现有 `sheets: [{ name, rows: [[value...]] }]` 输入无需任何修改即可工作，并自动获得表头行美化。

### 3.2 为什么纯 Rust（不用 Python）

1. **离线可靠性** — 无 Python 环境也能生成
2. **性能** — 原生性能，秒级生成 10 万行级文件
3. **rust_xlsxwriter v0.81 功能完备** — 覆盖上述所有需求
4. **无跨平台兼容性问题** — 避免 `python -c "import openpyxl"` 失败

### 3.3 不改动 `rust_xlsxwriter` crate（仅扩展用法）

当前 `rust_xlsxwriter = "0.81"` 已支持 Production Plan 所需的全部 API。改进是**纯应用层代码扩展**，零新依赖。

---

## 4. 新 Input Schema 设计

### 4.1 完整 schema（所有字段可选，向后兼容）

```json
{
  "format": "xlsx",
  "path": "output.xlsx",

  // ── 文档级 ──
  "title": "2024 年度销售报告",
  "author": "DS Pick",
  "language": "zh-CN",

  // ── 全局样式（可选，有智能默认值）──
  "style": {
    "theme": "corporate",
    "header_freeze": true,
    "border": "thin",
    "banded_rows": true,
    "print": {
      "orientation": "landscape",
      "paper_size": "A4",
      "fit_to_width": 1,
      "header": "&P / &N",
      "footer": "&F"
    }
  },

  // ── 工作表 ──
  "sheets": [
    {
      "name": "月度汇总",

      // 是否为第一行加表头格式（默认 true；纯数据表设为 false）
      "header": true,

      // 列定义（可选，用于精确控制列宽/格式）
      "columns": [
        { "width": 30, "label": "月份", "format": "text" },
        { "width": 15, "label": "销售额", "format": "currency", "number_format": "¥#,##0.00" },
        { "width": 12, "label": "增长率", "format": "percentage", "number_format": "0.00%" },
        { "width": 18, "label": "日期", "format": "date", "number_format": "yyyy-mm-dd" },
        { "width": 15, "label": "合计", "formula": "=SUM(B{{row}}:{{col}}{{last_row}})" },
        { "width": 36, "label": "备注", "format": "text", "wrap": true }
      ],

      // 数据行（二维数组，向后兼容现有格式）
      "rows": [
        ["一月", 150000, 0.125, "2024-01-31"],
        ["二月", 180000, 0.20,  "2024-02-29"],
        ["三月", 165000, -0.08,  "2024-03-31"]
      ],

      // 图表（可选）
      "charts": [
        {
          "type": "bar",
          "title": "月度销售趋势",
          "categories_range": "=月度汇总!$A$2:$A$13",
          "values_range": "=月度汇总!$B$2:$B$13",
          "position": { "row": 14, "col": 0 },
          "size": { "width": 12, "height": 8 }
        }
      ],

      // 条件格式（可选）
      "conditional_formats": [
        {
          "range": { "row": 0, "col": 1, "rows": 12, "cols": 1 },
          "type": "data_bar",
          "color": "#4472C4"
        }
      ],

      // 合并单元格（可选，结构化索引）
      "merged_cells": [
        { "row": 0, "col": 0, "rows": 1, "cols": 5 }
      ]
    }
  ]
}
```

### 4.2 字段映射表

| 层级 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| 根 | `format` | `"xlsx"` | ✅ | 固定值 |
| 根 | `path` | string | ✅ | 输出路径 |
| 根 | `title` | string | ❌ | 文档标题（自动写入首行合并单元格） |
| 根 | `author` | string | ❌ | 文档属性作者 |
| 根 | `style.theme` | string | ❌ | XLSX 主题：`corporate` / `tech` / `warm` / `minimal`（与 PPTX 的 `theme` 字段独立，互不影响） |
| 根 | `style.header_freeze` | bool | ❌ | 冻结首行（默认 true） |
| 根 | `style.border` | string | ❌ | `thin` / `none`（默认 thin） |
| 根 | `style.banded_rows` | bool | ❌ | 交替行底色（默认 true） |
| 根 | `style.print.orientation` | string | ❌ | `portrait` / `landscape`（默认 portrait） |
| 根 | `style.print.paper_size` | string | ❌ | `A4` / `A3` / `Letter`（默认 A4） |
| 根 | `style.print.fit_to_width` | number | ❌ | 缩放适应宽度页数（默认 1） |
| 根 | `style.print.header` | string | ❌ | 页眉（`&P`=页码, `&N`=总页数, `&F`=文件名） |
| 根 | `style.print.footer` | string | ❌ | 页脚 |
| 工作表 | `sheets[].name` | string | ✅ | 工作表名 |
| 工作表 | `sheets[].header` | bool | ❌ | 第一行是否为表头（默认 true） |
| 工作表 | `sheets[].columns` | array | ❌ | 列定义数组 |
| 工作表 | `sheets[].columns[].width` | number | ❌ | 列宽（字符数），未指定则自动计算 |
| 工作表 | `sheets[].columns[].label` | string | ❌ | 列标题（同时作为首行表头，优先级高于 `rows[0]`） |
| 工作表 | `sheets[].columns[].format` | string | ❌ | `text` / `number` / `currency` / `percentage` / `date` |
| 工作表 | `sheets[].columns[].number_format` | string | ❌ | Excel 自定义数字格式字符串 |
| 工作表 | `sheets[].columns[].formula` | string | ❌ | 公式模板：`{{row}}` 替换为 **1-based Excel 行号**（即 `row_idx + 1`），`{{col}}` 替换为列字母，`{{last_row}}` 替换为末行号 |
| 工作表 | `sheets[].columns[].wrap` | bool | ❌ | 自动换行（默认 false） |
| 工作表 | `sheets[].rows` | array | ✅ | 数据行，格式为 `[[value...]]` 二维数组 |
| 工作表 | `sheets[].charts` | array | ❌ | 图表定义 |
| 工作表 | `sheets[].conditional_formats` | array | ❌ | 条件格式定义 |
| 工作表 | `sheets[].merged_cells` | array | ❌ | 合并单元格：`{ row, col, rows, cols }` 结构化索引（0-based 行列号） |

---

## 5. 内置主题预设

| 主题 | 表头背景 | 表头文字 | 边框色 | 偶数行背景 |
|------|---------|---------|--------|-----------|
| `corporate` (默认) | `#4472C4` | `#FFFFFF` | `#D9E2F3` | `#F2F7FB` |
| `tech` | `#2D3748` | `#68D391` | `#4A5568` | `#EDF2F7` |
| `warm` | `#ED8936` | `#FFFFFF` | `#FBD38D` | `#FFFBEB` |
| `minimal` | `#FFFFFF` | `#1A202C` | `#CBD5E0` | 无色 |

---

## 6. 智能默认策略（零配置行为）

当用户仅提供 `sheets: [{ name, rows: [[...]] }]` 时：

1. **表头识别**（按优先级）:
   - 如果 `columns[].label` 存在 → `columns[].label` 数组作为表头行写入，`rows[0]` 作为第一行数据开始
   - 如果 `columns[].label` 不存在且 `sheets[].header != false` → `rows[0]` 视为表头行
   - 如果 `sheets[].header == false` → 不设表头行
   - 表头格式：加粗 + 主题背景色 + 白色文字 + 水平居中 + 垂直居中
2. **列宽自适应**: 扫描每列所有单元格，取最长内容 + 2 字符 margin（最多扫描前 1000 行）
3. **自动筛选**: 行数 > 1 时自动启用以表头为范围的 `set_autofilter()`
4. **边框**: 数据区域全 thin 边框
5. **冻结**: 冻结首行（`header_freeze` 默认 true）
6. **交替行底色**: 偶数行浅色背景（`banded_rows` 默认 true）
7. **数字对齐**: 数字右对齐，文本左对齐
8. **空值处理**: `null` → 空白单元格
9. **自动换行**: `columns[].wrap == true` 时启用 `set_text_wrap()`

---

## 7. 实施计划

### 7.1 Phase 1 — 核心格式化（P0）

> **目标**: 表头行样式 + 列宽自适应 + 数字格式 + 边框 + autofilter + 交替行色 + 冻结 + 对齐 + text_wrap
> **预计**: 350-450 行 Rust 代码
> **破坏性**: 零（纯新增功能）

| 功能 | rust_xlsxwriter API |
|------|-------------------|
| 表头加粗+背景 | `Format::new().set_bold().set_background_color().set_font_color()` |
| 列宽自适应 | 扫描 `rows` 前 1000 行取最大字符数（含 CJK 宽度估算），`worksheet.set_column_width()` |
| 数字格式 | `Format::new().set_num_format("¥#,##0.00")` |
| 边框 | `Format::new().set_border(Border::Thin)` |
| 交替行底色 | `Format::new().set_background_color()` 偶数行 |
| 冻结首行 + 自动筛选 | `worksheet.set_freeze_panes(1, 0)` + `worksheet.set_autofilter()` |
| 数字右对齐 | `Format::new().set_align(FormatAlign::Right)` |
| 文本自动换行 | `Format::new().set_text_wrap()`（`columns[].wrap == true`） |
| 垂直居中 | `Format::new().set_align(FormatAlign::VerticalCenter)` |
| 行高 | `worksheet.set_row_height(0, 24)` 表头行 |

**实现步骤**:
1. 新增 `Theme` struct + 四个预设
2. 新增 `apply_theme_styles()` — 生成 `header_fmt`/`data_fmt`/`banded_fmt`/`num_fmt`/`date_fmt` 等 `Format` 对象
3. 扩展 `generate_xlsx()` — 解析 `style` / `print` / `columns` 新字段，按格式写入
4. 新增 `auto_column_widths()` — 扫描前 1000 行数据计算列宽（含 CJK 字符宽度估算）
5. 新增 `write_header_row()` — 按优先级判断表头来源并写入
6. 新增 `auto_autofilter()` — 行数 > 1 且表头存在时自动设置

### 7.2 Phase 2 — 图表 + 合并单元格 + 标题行（P1）

> **目标**: 柱状/折线/饼图 + 标题合并单元格 + columns 列定义完整支持
> **预计**: 150-200 行

| 功能 | rust_xlsxwriter API |
|------|-------------------|
| 图表 | `Chart::new(ChartType::Bar).add_series()...` — 将 JSON `charts[].position/size/title` 映射到 Series 对象（需验证 v0.81 的 `add_series()` 参数签名） |
| 合并单元格 | `worksheet.merge_range(row0, col0, row1, col1, "标题", format)` 使用结构化索引 |
| 文档标题行 | 读取 `title`，在工作表第 0 行跨所有列 merge 后写入 |

### 7.3 Phase 3 — 公式 + 条件格式 + 打印设置（P2）

> **预计**: 120-150 行

| 功能 | rust_xlsxwriter API |
|------|-------------------|
| 公式 | `worksheet.write_formula(row, col, "=SUM(B2:B13)")` |
| 模板公式 | `{{row}}` → 1-based Excel 行号，`{{col}}` → 列字母，`{{last_row}}` → 末行号 |
| 数据条 | `ConditionalFormat::new().set_data_bar()` |
| 页面设置 | `worksheet.set_print_orientation()` / `set_paper_size()` / `set_fit_to_pages()` |
| 页眉页脚 | `worksheet.set_header("&P / &N")` / `worksheet.set_footer("&F")` |
| 页边距 | `worksheet.set_margins()` |

### 7.4 Phase 4 — 数据验证 + 工作表保护（P3）

> **预计**: 50-80 行

| 功能 | rust_xlsxwriter API |
|------|-------------------|
| 下拉列表 | `DataValidation::new().set_list()` → `worksheet.add_data_validation()` |
| 数值范围 | `DataValidation::new().set_allow_number_range()` |
| 工作表保护 | `worksheet.protect()` 或 `worksheet.protect_with_password()` |
| 大文件性能 | 当前 10 万行秒级生成已满足需求；如 `Workbook::new_with_options()` 提供 `constant_memory` 模式则适配 |

---

## 8. 风险评估

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| 列宽自适应扫描 10 万行耗时 | 低 | 仅扫描前 1000 行估算，超过用默认值 |
| 主题颜色与用户数据冲突 | 低 | 用户可通过 `style.theme` 选择/关闭 |
| Schema 过于复杂，AI 模型难以生成正确 JSON | 中 | 保留零配置路径；`columns` 等 expando 字段全部 optional |
| `rust_xlsxwriter` API 变更 (0.81 → future) | 低 | 锁定 `"0.81"` 版本 |
| 首行非表头（纯数据表）被误格式化 | 低 | `sheets[].header: false` 关闭表头格式化 |

---

## 9. 验收标准

- [ ] 现有 `rows: [[...]]` 输入生成带表头样式的 xlsx → Excel/WPS/LibreOffice 正常打开
- [ ] 中文列名/中日韩统一表意文字不乱码
- [ ] 数字列自动右对齐，日期保留格式
- [ ] 10 列 × 1000 行文件 <1s 生成
- [ ] `cargo clippy -- -D warnings` 零新增警告
- [ ] 现有 `cargo test --workspace` 全部通过
- [ ] 空值 `null` 输出为空白单元格
- [ ] 图表在 Excel/WPS 中正常渲染
