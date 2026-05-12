# write_office 工具使用指南

## 快速上手指南

### 用户要 Excel (.xlsx) → 用 xlsx 格式

最简例子（零配置,自动美化）:

```json
{
  "format": "xlsx",
  "path": "销售报告.xlsx",
  "sheets": [
    {
      "name": "月度汇总",
      "rows": [
        ["月份", "销售额", "增长率"],
        ["1月", 150000, 0.125],
        ["2月", 180000, 0.20],
        ["3月", 165000, -0.08]
      ]
    }
  ]
}
```

> 自动生效：表头加粗 + 蓝色背景 + 列宽自适应 + 边框 + 冻结首行 + 自动筛选 + 交替行色

### 用户要 PPT (.pptx) → 用 pptx 格式

```json
{
  "format": "pptx",
  "path": "项目汇报.pptx",
  "title": "Q1 项目汇报",
  "subtitle": "2024年第一季度",
  "theme": "dark",
  "slides": [
    { "title": "项目进度", "bullets": ["完成需求分析", "进入开发阶段"] },
    { "title": "关键指标",
      "chart": { "type": "bar", "categories": ["1月","2月","3月"], "series": [{"name":"完成率","values":[85,92,78]}] } }
  ]
}
```

### 用户要 Word (.docx) → 用 docx 格式

```json
{
  "format": "docx",
  "path": "合同.docx",
  "title": "项目合同",
  "blocks": [
    { "type": "heading", "level": 1, "text": "第一条 项目范围" },
    { "type": "paragraph", "text": "本合同约定..." },
    { "type": "list", "style": "number", "items": ["需求分析", "设计", "交付"] }
  ]
}
```

## 格式选择决策树

| 用户说 | 选择 | 原因 |
|--------|------|------|
| "表格" "数据" "报表" "统计" "Excel" | **xlsx** | 纯 Rust,离线可靠,秒级生成 |
| "PPT" "幻灯片" "汇报" "演示" | **pptx** | Python引擎,支持图表/表格/封面 |
| "文档" "合同" "报告" "Word" | **docx** | Python 优先，纯 Rust 兜底 |

## XLSX 完整参数速查

```
{
  "format": "xlsx",
  "path": "文件.xlsx",
  "title": "文档标题（可选,自动合并首行）",
  "author": "作者（可选）",

  "style": {                                  // 全局样式（全部可选,有智能默认值）
    "theme": "corporate",                     // corporate | tech | warm | minimal
    "header_freeze": true,                    // 冻结首行
    "border": "thin",                         // thin | none
    "banded_rows": true,                      // 交替行底色
    "print": {                                // 打印设置（全部可选）
      "orientation": "landscape",             // portrait | landscape
      "paper_size": "A4",                     // A4 | A3 | Letter
      "fit_to_width": 1,                      // 缩放适应宽度页数
      "header": "&P / &N",                    // 页眉 &P=页码 &N=总页数
      "footer": "&F"                          // 页脚 &F=文件名
    }
  },

  "sheets": [
    {
      "name": "工作表名",
      "header": true,                         // 首行是否为表头（默认true,纯数据表设false）

      "columns": [                            // 列定义（可选,用于精确控制格式）
        { "width": 30, "label": "月份", "format": "text" },
        { "width": 15, "label": "销售额", "format": "currency", "number_format": "¥#,##0.00" },
        { "width": 12, "label": "增长率", "format": "percentage", "number_format": "0.00%" },
        { "width": 18, "label": "日期", "format": "date", "number_format": "yyyy-mm-dd" },
        { "width": 15, "label": "合计", "formula": "=SUM(B{{row}}:B{{last_row}})" },
        { "width": 36, "label": "备注", "format": "text", "wrap": true }
      ],

      "rows": [                               // 数据行（必填,二维数组）
        ["1月", 150000, 0.125, "2024-01-31"],
        ["2月", 180000, 0.20,  "2024-02-29"]
      ],

      "merged_cells": [                       // 合并单元格（可选）
        { "row": 0, "col": 0, "rows": 1, "cols": 5 }
      ],

      "charts": [                             // 图表（可选）
        {
          "type": "bar",                      // bar | line | pie | stacked_bar
          "title": "月度趋势",
          "categories_range": "=月度汇总!$A$2:$A$13",
          "values_range": "=月度汇总!$B$2:$B$13",
          "position": { "row": 14, "col": 0 }
        }
      ],

      "conditional_formats": [                // 条件格式（可选）
        {
          "range": { "row": 0, "col": 1, "rows": 12, "cols": 1 },
          "type": "data_bar",
          "color": "#4472C4"
        }
      ]
    }
  ]
}
```

### 列格式 (columns[].format) 速查

| format | 用途 | 默认 number_format | 对齐 |
|--------|------|-------------------|------|
| `text` | 文本 | - | 左对齐 |
| `number` | 整数/小数 | `#,##0` | 右对齐 |
| `currency` | 金额 | `#,##0.00` | 右对齐 |
| `percentage` | 百分比 | `0.00%` | 右对齐 |
| `date` | 日期（yyyy-mm-dd 或 yyyy/mm/dd） | `yyyy-mm-dd` | - |

### 公式模板语法

| 占位符 | 替换为 | 示例 |
|--------|--------|------|
| `{{row}}` | 当前行的 1-based Excel 行号 | `=B{{row}}*C{{row}}` |
| `{{col}}` | 当前列字母 | `=SUM({{col}}2:{{col}}13)` |
| `{{last_row}}` | 数据最后一行行号 | `=SUM(B2:B{{last_row}})` |

### XLSX 主题配色

| 主题 | 表头背景 | 表头文字 | 偶数行背景 |
|------|---------|---------|-----------|
| `corporate` (默认) | 蓝 #4472C4 | 白 | 浅蓝 #F2F7FB |
| `tech` | 深灰 #2D3748 | 绿 #68D391 | 浅灰 |
| `warm` | 橙 #ED8936 | 白 | 米色 |
| `minimal` | 白 | 深灰 | 无色 |

## PPTX 完整参数速查

```json
{
  "format": "pptx",
  "path": "演示文稿.pptx",
  "title": "封面标题（可选）",
  "subtitle": "封面副标题（PPTX专用）",
  "theme": "dark",        // dark | light | warm | minimal（PPTX主题,与XLSX独立）

  "slides": [
    {
      "title": "幻灯片标题",
      "bullets": ["要点1", "要点2"],
      "table": {
        "headers": ["列A", "列B"],
        "rows": [["值1", 100], ["值2", 200]]
      },
      "chart": {
        "type": "bar",    // bar | line | pie | stacked_bar | stacked_bar_pct | area | scatter | donut
        "categories": ["A", "B", "C"],
        "series": [{"name": "系列名", "values": [10, 20, 30]}],
        "chart_title": "图表标题",
        "x_label": "X轴标签",
        "y_label": "Y轴标签"
      },
      "notes": "演讲者备注（可选）"
    }
  ]
}
```

## DOCX 完整参数速查

```json
{
  "format": "docx",
  "path": "文档.docx",
  "title": "文档标题（可选）",

  "blocks": [
    { "type": "heading",  "level": 1, "text": "一级标题" },
    { "type": "heading",  "level": 2, "text": "二级标题" },
    { "type": "paragraph", "text": "正文段落内容..." },
    { "type": "list", "style": "bullet", "items": ["项目A", "项目B"] },
    { "type": "list", "style": "number", "items": ["第一步", "第二步"] }
  ]
}
```

> DOCX 表格生成需要 Python 引擎。纯 Rust 后备引擎支持 heading/paragraph/list,不支持 table。

## 最佳实践

### ✅ 要做

1. **日期列必须设 format:"date"** — 否则日期当作文本,无法排序/筛选/运算
2. **金额列用 format:"currency"** — `number_format` 可选 `"¥#,##0.00"` 或 `"$#,##0.00"`
3. **百分比列用 format:"percentage"** — 输入小数（0.125 即 12.5%）
4. **长文本列加 "wrap": true** — 防止内容被截断
5. **纯数据表设 "header": false** — 避免第一行被误格式化为表头
6. **PPTX 优先设 title** — 自动生成封面幻灯片,更专业

### ❌ 不要做

1. **不要把 Excel 公式写成 `={{row}}`** — 不要加外层花括号
2. **不要用 merged_cells 的 A1 格式** — 必须用 `{row,col,rows,cols}` 结构化索引
3. **不要混用 XLSX 和 PPTX 的 theme** — 两个主题系统完全独立
4. **不要把 null 值写成字符串 "null"** — 直接用 JSON null

## 常见场景模板

### 模板 A：销售业绩报表

```json
{ "format": "xlsx", "path": "销售报表.xlsx", "title": "2024年度销售业绩",
  "sheets": [{ "name": "月度汇总",
    "columns": [
      { "label": "月份", "format": "text", "width": 15 },
      { "label": "销售额", "format": "currency", "number_format": "¥#,##0", "width": 18 },
      { "label": "环比增长", "format": "percentage", "width": 15 }
    ],
    "rows": [["1月", 1500000, 0.125], ["2月", 1800000, 0.20], ["3月", 1680000, -0.067]]
  }]
}
```

### 模板 B：项目进度 PPT

```json
{ "format": "pptx", "path": "项目汇报.pptx", "title": "Q1 项目汇报", "theme": "dark",
  "slides": [
    { "title": "项目概况", "bullets": ["项目目标：完成核心功能开发","当前进度：85%","预计完成：2024-04-30"] },
    { "title": "里程碑", "table": {"headers":["里程碑","状态","完成日期"],"rows":[["需求分析","✅完成","1月15日"],["架构设计","✅完成","2月1日"],["开发","🔄进行中","-"]]} },
    { "title": "关键指标", "chart":{"type":"bar","categories":["1月","2月","3月"],"series":[{"name":"代码提交","values":[45,62,38]}],"chart_title":"月度代码提交量"} }
  ]
}
```

### 模板 C：数据仪表盘

```json
{ "format": "xlsx", "path": "仪表盘.xlsx", "title": "运营数据仪表盘",
  "style": { "theme": "corporate" },
  "sheets": [
    { "name": "收入",
      "columns": [
        { "label": "月份", "format": "text", "width": 12 },
        { "label": "收入", "format": "currency", "width": 18 },
        { "label": "月环比", "format": "percentage", "width": 12 }
      ],
      "rows": [["1月", 500000, null], ["2月", 520000, 0.04], ["3月", 480000, -0.077]],
      "charts": [{"type": "bar", "title": "月度收入趋势", "categories_range": "=收入!$A$2:$A$4", "values_range": "=收入!$B$2:$B$4", "position": {"row": 6, "col": 0}}]
    },
    { "name": "用户",
      "columns": [
        { "label": "月份", "format": "text", "width": 12 },
        { "label": "新增用户", "format": "number", "width": 15 }
      ],
      "rows": [["1月", 12500], ["2月", 15000], ["3月", 13800]]
    }
  ]
}
```
