#!/usr/bin/env python3
"""Summarize LHT harness runs.jsonl and optional regression gate."""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any


GATE_FAIL_OUTCOMES = frozenset({"harness_regression", "false_green", "harness_misconfig", "infra"})


def load_runs(path: Path) -> list[dict[str, Any]]:
    runs: list[dict[str, Any]] = []
    text = path.read_text(encoding="utf-8-sig")
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        runs.append(json.loads(line))
    return runs


def summarize(runs: list[dict[str, Any]]) -> dict[str, Any]:
    by_key: dict[str, list[dict[str, Any]]] = defaultdict(list)
    outcomes = Counter()
    for r in runs:
        key = f"{r.get('task_id')}::{r.get('harness_profile')}"
        by_key[key].append(r)
        outcomes[r.get("outcome_class", "unknown")] += 1

    rows = []
    for key, group in sorted(by_key.items()):
        task_id, profile = key.split("::", 1)
        passed = sum(1 for g in group if g.get("passed"))
        rows.append(
            {
                "task_id": task_id,
                "harness_profile": profile,
                "runs": len(group),
                "passed": passed,
                "pass_rate": round(passed / len(group), 3) if group else 0.0,
            }
        )

    gate_failures = sum(outcomes[o] for o in GATE_FAIL_OUTCOMES)
    return {
        "total_runs": len(runs),
        "outcome_counts": dict(outcomes),
        "gate_failure_count": gate_failures,
        "rows": rows,
    }


def diff_baseline(summary: dict[str, Any], baseline: dict[str, Any]) -> dict[str, Any]:
    prev = {
        (r["task_id"], r["harness_profile"]): r.get("pass_rate", 0.0)
        for r in baseline.get("rows", [])
    }
    changes = []
    for row in summary.get("rows", []):
        key = (row["task_id"], row["harness_profile"])
        old = prev.get(key)
        if old is None:
            changes.append({"key": key, "change": "new", "pass_rate": row["pass_rate"]})
        elif old != row["pass_rate"]:
            changes.append(
                {
                    "key": key,
                    "change": "pass_rate",
                    "from": old,
                    "to": row["pass_rate"],
                }
            )
    return {"changes": changes}


def main() -> int:
    parser = argparse.ArgumentParser(description="LHT harness run report")
    parser.add_argument("jsonl", type=Path, help="runs.jsonl path")
    parser.add_argument("--baseline", type=Path, default=None)
    parser.add_argument("--out", type=Path, default=None, help="Write summary JSON")
    parser.add_argument("--gate", action="store_true", help="Exit 1 on gate failures")
    parser.add_argument("--json", action="store_true", dest="as_json")
    args = parser.parse_args()

    if not args.jsonl.is_file():
        print(f"runs file not found: {args.jsonl}", file=sys.stderr)
        return 2

    runs = load_runs(args.jsonl)
    summary = summarize(runs)
    if args.baseline and args.baseline.is_file():
        baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
        summary["baseline_diff"] = diff_baseline(summary, baseline)

    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(summary, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    if args.as_json:
        print(json.dumps(summary, indent=2, ensure_ascii=False))
    else:
        print(f"total runs: {summary['total_runs']}")
        print(f"outcomes: {summary['outcome_counts']}")
        print(f"gate failures (harness/infra): {summary['gate_failure_count']}")
        print("")
        for row in summary["rows"]:
            print(
                f"  {row['task_id']} / {row['harness_profile']}: "
                f"{row['passed']}/{row['runs']} passed ({row['pass_rate']})"
            )
        if "baseline_diff" in summary:
            changes = summary["baseline_diff"]["changes"]
            if changes:
                print("")
                print("baseline diff:")
                for c in changes:
                    print(f"  {c}")

    if args.gate and summary["gate_failure_count"] > 0:
        print("\nGATE FAIL", file=sys.stderr)
        return 1
    if args.gate:
        print("\nGATE PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
