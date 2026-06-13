#!/usr/bin/env python3
"""Compare two kernel-v2 corpus reports (M4 bucketed acceptance).

Usage:
  python scripts/kernel_v2_corpus_compare.py \\
    results/kernel-v2-corpus/m0.2-baseline/report.json \\
    results/kernel-v2-corpus/dag-run/report.json

Prints per-batch-shape step latency deltas and whether pure_read meets the
M4 −20% p50 gate (informational unless --gate is passed).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


def load_report(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def pct_change(baseline: float, candidate: float) -> float | None:
    if baseline <= 0:
        return None
    return round((baseline - candidate) / baseline * 100.0, 1)


def main() -> int:
    parser = argparse.ArgumentParser(description="Compare kernel-v2 corpus reports")
    parser.add_argument("baseline_report", help="Baseline report.json (e.g. M0.2)")
    parser.add_argument("candidate_report", help="Candidate report.json (e.g. scheduler=dag)")
    parser.add_argument(
        "--gate",
        action="store_true",
        help="Exit 1 when pure_read step p50 did not improve by at least 20%%",
    )
    parser.add_argument(
        "--min-improvement-pct",
        type=float,
        default=20.0,
        help="Required pure_read step p50 improvement (default 20)",
    )
    args = parser.parse_args()

    base_path = Path(args.baseline_report)
    cand_path = Path(args.candidate_report)
    for path in (base_path, cand_path):
        if not path.is_file():
            print(f"report not found: {path}", file=sys.stderr)
            return 1

    base = load_report(base_path)
    cand = load_report(cand_path)
    base_agg = base.get("aggregate_by_batch_shape") or {}
    cand_agg = cand.get("aggregate_by_batch_shape") or {}

    print(f"baseline: {base_path}")
    print(f"candidate: {cand_path}")
    print()
    print(f"{'shape':<18} {'base p50':>9} {'cand p50':>9} {'Δ p50':>8} {'base p95':>9} {'cand p95':>9} {'Δ p95':>8}")

    pure_read_ok = True
    for shape in sorted(set(base_agg) | set(cand_agg)):
        b = base_agg.get(shape, {})
        c = cand_agg.get(shape, {})
        b_step = b.get("step_total_ms") or {}
        c_step = c.get("step_total_ms") or {}
        b_p50 = float(b_step.get("p50") or 0)
        c_p50 = float(c_step.get("p50") or 0)
        b_p95 = float(b_step.get("p95") or 0)
        c_p95 = float(c_step.get("p95") or 0)
        d50 = pct_change(b_p50, c_p50)
        d95 = pct_change(b_p95, c_p95)
        d50_text = f"{d50:+.1f}%" if d50 is not None else "n/a"
        d95_text = f"{d95:+.1f}%" if d95 is not None else "n/a"
        print(
            f"{shape:<18} {b_p50:>9.1f} {c_p50:>9.1f} {d50_text:>8} "
            f"{b_p95:>9.1f} {c_p95:>9.1f} {d95_text:>8}"
        )
        if shape == "pure_read" and d50 is not None and d50 < args.min_improvement_pct:
            pure_read_ok = False

    print()
    if "pure_read" in base_agg and "pure_read" in cand_agg:
        b = base_agg["pure_read"]
        c = cand_agg["pure_read"]
        b_tool = (b.get("tool_step_total_ms") or {}).get("p50")
        c_tool = (c.get("tool_step_total_ms") or {}).get("p50")
        b_tphase = (b.get("tool_phase_ms") or {}).get("p50")
        c_tphase = (c.get("tool_phase_ms") or {}).get("p50")
        if b_tool is not None and c_tool is not None:
            d_tool = pct_change(float(b_tool), float(c_tool))
            d_tool_text = f"{d_tool:+.1f}%" if d_tool is not None else "n/a"
            print(
                f"pure_read tool_step p50: {float(b_tool):.1f} → {float(c_tool):.1f} ms ({d_tool_text})"
            )
        if b_tphase is not None and c_tphase is not None:
            d_tp = pct_change(float(b_tphase), float(c_tphase))
            d_tp_text = f"{d_tp:+.1f}%" if d_tp is not None else "n/a"
            print(
                f"pure_read tool_phase p50: {float(b_tphase):.1f} → {float(c_tphase):.1f} ms ({d_tp_text})"
            )
        print()
        if pure_read_ok:
            print(
                f"pure_read step p50 gate: OK (≥ {args.min_improvement_pct:.0f}% improvement)"
            )
        else:
            msg = (
                f"pure_read step p50 gate: FAIL (need ≥ {args.min_improvement_pct:.0f}% improvement)"
            )
            print(msg)
            if args.gate:
                return 1
    else:
        print("pure_read bucket missing in one or both reports — gate skipped")

    return 0


if __name__ == "__main__":
    sys.exit(main())
