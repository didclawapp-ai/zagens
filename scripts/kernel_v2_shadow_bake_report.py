#!/usr/bin/env python3
"""Aggregate kernel-v2 M3/M4 shadow counters from corpus runs.jsonl (bake monitoring).

Reads one or more run directories (or a parent containing multiple labels) and
summarizes policy_shadow / scheduler_shadow blocks recorded by
scripts/kernel-v2-corpus-run.ps1 after each scenario (via GET /v1/runtime/kernel-shadow).

Gate target (M3): policy_shadow.diff_rate_pct < 0.1% sustained over 2 calendar weeks
before flipping `[tools] policy` default to `engine`.

Usage:
  python scripts/kernel_v2_shadow_bake_report.py results/kernel-v2-corpus/m3-shadow
  python scripts/kernel_v2_shadow_bake_report.py results/kernel-v2-corpus --gate
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


def load_runs_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    if not path.is_file():
        return rows
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(row, dict):
            rows.append(row)
    return rows


def collect_run_dirs(args_paths: list[str]) -> list[Path]:
    dirs: list[Path] = []
    for raw in args_paths:
        p = Path(raw)
        if p.is_file() and p.name == "runs.jsonl":
            dirs.append(p.parent)
            continue
        if p.is_dir():
            jsonl = p / "runs.jsonl"
            if jsonl.is_file():
                dirs.append(p)
            else:
                for child in sorted(p.iterdir()):
                    if child.is_dir() and (child / "runs.jsonl").is_file():
                        dirs.append(child)
    return dirs


def aggregate_shadow(
    runs: list[dict[str, Any]], field: str
) -> dict[str, Any] | None:
    blocks = [r[field] for r in runs if isinstance(r.get(field), dict)]
    if not blocks:
        return None
    comparisons = sum(int(b.get("comparisons") or 0) for b in blocks)
    diffs = sum(int(b.get("diffs") or 0) for b in blocks)
    rate = 0.0 if comparisons == 0 else (diffs / comparisons) * 100.0
    scenarios_with_data = len(blocks)
    scenarios_with_diffs = sum(1 for b in blocks if int(b.get("diffs") or 0) > 0)
    return {
        "scenarios_with_data": scenarios_with_data,
        "scenarios_with_diffs": scenarios_with_diffs,
        "comparisons": comparisons,
        "diffs": diffs,
        "diff_rate_pct": round(rate, 6),
    }


def print_block(title: str, agg: dict[str, Any] | None, gate_pct: float) -> bool:
    print(f"\n{title}")
    if not agg:
        print("  (no shadow counters in runs.jsonl — probe sidecar before stop or use updated corpus runner)")
        return True
    print(f"  scenarios with data: {agg['scenarios_with_data']}")
    print(f"  scenarios with diffs: {agg['scenarios_with_diffs']}")
    print(f"  comparisons: {agg['comparisons']}")
    print(f"  diffs:       {agg['diffs']}")
    print(f"  diff_rate:   {agg['diff_rate_pct']:.4f}%")
    if gate_pct > 0:
        ok = agg["diff_rate_pct"] < gate_pct
        print(f"  gate (<{gate_pct}%): {'PASS' if ok else 'FAIL'}")
        return ok
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description="Kernel v2 shadow bake report")
    parser.add_argument(
        "paths",
        nargs="+",
        help="Run directory, parent directory, or runs.jsonl path",
    )
    parser.add_argument(
        "--gate",
        action="store_true",
        help="Exit 1 if policy shadow diff_rate_pct >= 0.1%%",
    )
    parser.add_argument(
        "--gate-pct",
        type=float,
        default=0.1,
        help="M3 bake gate threshold (default 0.1%%)",
    )
    args = parser.parse_args()

    run_dirs = collect_run_dirs(args.paths)
    if not run_dirs:
        print("No runs.jsonl found under given paths", file=sys.stderr)
        return 2

    all_runs: list[dict[str, Any]] = []
    for d in run_dirs:
        rows = load_runs_jsonl(d / "runs.jsonl")
        print(f"== {d} ({len(rows)} scenario runs) ==")
        for row in rows:
            sid = row.get("scenario_id", "?")
            ps = row.get("policy_shadow")
            if isinstance(ps, dict) and ps.get("comparisons"):
                print(
                    f"  {sid}: policy comparisons={ps.get('comparisons')} "
                    f"diffs={ps.get('diffs')} rate={ps.get('diff_rate_pct')}%"
                )
        all_runs.extend(rows)

    gate_pct = args.gate_pct if args.gate else 0.0
    policy_ok = print_block("Policy shadow (M3)", aggregate_shadow(all_runs, "policy_shadow"), gate_pct)
    sched_ok = print_block(
        "Scheduler shadow (M4)",
        aggregate_shadow(all_runs, "scheduler_shadow"),
        0.0,
    )

    if args.gate and not policy_ok:
        return 1
    if args.gate and not sched_ok:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
