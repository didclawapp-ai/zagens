#!/usr/bin/env python3
"""Generate office-demo binary fixtures (XLSX). Run from repo root."""

from __future__ import annotations

from pathlib import Path

import openpyxl

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs" / "harness" / "fixtures" / "office-demo" / "data"


def write_production_daily(path: Path) -> None:
    wb = openpyxl.Workbook()
    prod = wb.active
    prod.title = "生产"
    prod.append(["日期", "2026-06-04"])
    prod.append([])
    prod.append(["产线", "计划", "实际", "达成率"])
    prod.append(["A 线", 1200, 1156, 0.963])
    prod.append(["B 线", 800, 802, 1.003])
    prod.append([])
    prod.append(["OEE 综合", 0.872])
    prod.append(["异常", "A 线 14:00–16:30 计划外停机（伺服报警，已复位）"])

    qual = wb.create_sheet("品质")
    qual.append(["日期", "2026-06-04"])
    qual.append([])
    qual.append(["指标", "数值", "目标/说明"])
    qual.append(["总良率", 0.986, 0.99])
    qual.append(["外观划伤占比", 0.42, "主要不良"])
    qual.append(["尺寸超差占比", 0.31, "主要不良"])
    qual.append([])
    qual.append(["批次", "抽检", "状态"])
    qual.append(["#B240604-07", "3/80 不合格", "已隔离，8D 进行中"])
    qual.append([])
    qual.append(["待决", "是否放宽 B 线某工序公差（销售催货）", "需经营层拍板"])

    path.parent.mkdir(parents=True, exist_ok=True)
    wb.save(path)
    print(f"wrote {path}")


def main() -> None:
    write_production_daily(OUT / "生产日报_昨日.xlsx")


if __name__ == "__main__":
    main()
