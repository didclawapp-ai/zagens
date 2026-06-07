# Office 场景演示 fixtures

> 配合 [OFFICE_SCENARIOS.md](../../../docs/desktop/OFFICE_SCENARIOS.md) §6 P0 与 bundled `office-*` 技能。

## 用法

1. 在 Office 工作区根目录创建 `inbox/`、`data/`、`deliverables/`（或复制本目录结构）。
2. 复制 fixtures：
   - `inbox/*` → 工作区 `inbox/`
   - `data/*` → 工作区 `data/`
3. Composer 选 **办公模式**，点击任务卡片或输入对应 prompt。

### P0 场景速查

| P0 | 卡片 / 技能 | 输入 | 验收 |
|----|-------------|------|------|
| **P0-2** | 经营日报汇总 · `office-executive-daily-brief` | `inbox/` 三份简报 | DOCX 含 **待决事项**；数字与 inbox 一致 |
| **P0-4** | 客户报价单 · `office-customer-quote` | `data/价目表.csv` + `data/客户需求.md` | XLSX 含报价明细与含税合计 |
| **P0-3** | 生产品质晨报 · `office-production-daily-report` | `data/生产日报_昨日.xlsx` | 先文字概况 → DOCX 含 **概况** 与 OEE |
| **P0-1** | 竞品分析 · `office-competitive-analysis` | 联网 | DOCX 含来源列表 |

### Oracle（可选）

```powershell
.\scripts\office-demo-oracle.ps1 -WorkspaceRoot C:\path\to\workspace -Scenario p0-2
.\scripts\office-demo-oracle.ps1 -WorkspaceRoot C:\path\to\workspace -Scenario p0-3
.\scripts\office-demo-oracle.ps1 -WorkspaceRoot C:\path\to\workspace -Scenario p0-4
```

## 目录

| 路径 | 说明 |
|------|------|
| `inbox/生产部_昨日简报.md` | 产量、OEE、异常 |
| `inbox/品质部_昨日简报.md` | 良率、不良 TOP、8D 状态 |
| `inbox/销售部_昨日简报.md` | 订单、回款、客户跟进 |
| `data/价目表.csv` | P0-4 价目表（6 行产品） |
| `data/客户需求.md` | P0-4 华东机械 500 台询价 |
| `data/生产日报_昨日.xlsx` | P0-3 生产+品质双 sheet（与 inbox 数字对齐） |

## 重新生成 XLSX fixture

```bash
python scripts/gen-office-demo-fixtures.py
```
