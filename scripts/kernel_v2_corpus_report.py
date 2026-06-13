#!/usr/bin/env python3
"""Kernel v2 corpus analyzer (M0.1): per-step latency JSON from captured event streams.

Consumes a run directory produced by scripts/kernel-v2-corpus-run.ps1:
  runs.jsonl                 one record per scenario run
  events-<id>-r<N>.sse       persisted SSE backlog (replay_only=1 capture)
  thread-<id>-r<N>.json      thread detail (turn records with usage)

Produces <run_dir>/report.json:
  per-run step breakdown (model phase / tool batch phase, tool names) and
  per-batch-shape aggregates (mean / p50 / p95) — the denominator for the
  M0.2 baseline report and the M4 bucketed acceptance benchmarks.

A "step" is one model round: the model-output phase that ends at a tool
batch, plus that batch's execution. The final answer-only phase counts as a
step with no tools.
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
from datetime import datetime
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    import tomli as tomllib  # type: ignore


def parse_ts(raw: str) -> datetime:
    """Parse RFC3339 timestamps with arbitrary fractional precision."""
    text = raw.strip()
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    if "." in text:
        head, _, rest = text.partition(".")
        frac = ""
        tz = ""
        for i, ch in enumerate(rest):
            if ch.isdigit():
                frac += ch
            else:
                tz = rest[i:]
                break
        text = f"{head}.{frac[:6].ljust(6, '0')}{tz}"
    return datetime.fromisoformat(text)


def parse_sse_records(path: Path) -> list[dict[str, Any]]:
    """Extract the JSON data payloads from an SSE capture file."""
    records: list[dict[str, Any]] = []
    # Split on `\n` only — `str.splitlines()` also breaks on U+0085, which appears
    # as the third byte of several CJK UTF-8 code points (e.g. 阅 E9 98 85).
    for line in path.read_text(encoding="utf-8", errors="replace").split("\n"):
        line = line.rstrip("\r")
        if not line.startswith("data:"):
            continue
        body = line[len("data:") :].strip()
        if not body or body == "keepalive":
            continue
        try:
            record = json.loads(body)
        except json.JSONDecodeError:
            continue
        if isinstance(record, dict) and "event" in record:
            records.append(record)
    records.sort(key=lambda r: r.get("seq", 0))
    return records


def ms_between(start: datetime, end: datetime) -> float:
    return round((end - start).total_seconds() * 1000.0, 1)


def is_model_item_start(record: dict[str, Any]) -> bool:
    if record.get("event") != "item.started":
        return False
    payload = record.get("payload") or {}
    if "tool" in payload:
        return False
    kind = str((payload.get("item") or {}).get("kind") or "")
    return "message" in kind or "thinking" in kind


def extract_steps(records: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Segment one turn's event stream into steps (model phase + tool batch)."""
    turn_start: datetime | None = None
    turn_end: datetime | None = None
    steps: list[dict[str, Any]] = []

    step_start: datetime | None = None
    batch_first_start: datetime | None = None
    batch_last_end: datetime | None = None
    batch_tools: list[str] = []
    open_tool_ids: set[str] = set()

    def close_step() -> None:
        nonlocal step_start, batch_first_start, batch_last_end, batch_tools
        if step_start is None or batch_first_start is None or batch_last_end is None:
            return
        steps.append(
            {
                "index": len(steps) + 1,
                "model_phase_ms": ms_between(step_start, batch_first_start),
                "tool_phase_ms": ms_between(batch_first_start, batch_last_end),
                "total_ms": ms_between(step_start, batch_last_end),
                "tools": list(batch_tools),
            }
        )
        step_start = batch_last_end
        batch_first_start = None
        batch_last_end = None
        batch_tools = []

    for record in records:
        event = record.get("event")
        ts = parse_ts(record["timestamp"])
        payload = record.get("payload") or {}

        if event == "turn.started":
            turn_start = ts
            step_start = ts
        elif event == "item.started" and "tool" in payload:
            tool = payload["tool"] or {}
            if batch_first_start is None:
                batch_first_start = ts
            batch_tools.append(str(tool.get("name") or "unknown"))
            open_tool_ids.add(str(tool.get("id") or ""))
        elif event in ("item.completed", "item.failed", "item.interrupted") and "tool" in payload:
            tool = payload["tool"] or {}
            open_tool_ids.discard(str(tool.get("id") or ""))
            batch_last_end = ts
        elif is_model_item_start(record):
            # Model output resumed: any fully-finished batch closes its step.
            if batch_first_start is not None and not open_tool_ids and batch_last_end is not None:
                close_step()
        elif event == "turn.completed":
            turn_end = ts

    if batch_first_start is not None and batch_last_end is not None:
        close_step()
    if step_start is not None and turn_end is not None and turn_end > step_start:
        steps.append(
            {
                "index": len(steps) + 1,
                "model_phase_ms": ms_between(step_start, turn_end),
                "tool_phase_ms": 0.0,
                "total_ms": ms_between(step_start, turn_end),
                "tools": [],
            }
        )

    if turn_start is not None and turn_end is not None:
        for step in steps:
            step["turn_duration_ms"] = ms_between(turn_start, turn_end)
    return steps


def extract_usage(records: list[dict[str, Any]], thread_detail: dict[str, Any] | None) -> dict[str, Any]:
    usage: dict[str, Any] = {}
    for record in records:
        if record.get("event") != "turn.completed":
            continue
        turn = (record.get("payload") or {}).get("turn") or {}
        usage = dict(turn.get("usage") or {})
        if turn.get("duration_ms") is not None:
            usage["turn_duration_ms"] = turn["duration_ms"]
    if not usage and thread_detail:
        turns = thread_detail.get("turns") or []
        if turns:
            usage = dict(turns[-1].get("usage") or {})
    input_tokens = usage.get("input_tokens") or 0
    hit = usage.get("prompt_cache_hit_tokens")
    if input_tokens and hit is not None:
        usage["cache_hit_rate"] = round(hit / input_tokens, 4)
    return usage


def extract_prefix_fingerprints(records: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Collect turn.prefix_fingerprint events (kernel-v2 M5)."""
    out: list[dict[str, Any]] = []
    for record in records:
        if record.get("event") != "turn.prefix_fingerprint":
            continue
        payload = record.get("payload") or {}
        fp = payload.get("request_fingerprint") or {}
        if not fp:
            continue
        out.append(
            {
                "seq": record.get("seq"),
                "static_prefix_sha256": fp.get("static_prefix_sha256"),
                "full_prefix_sha256": fp.get("full_prefix_sha256"),
            }
        )
    return out


def prefix_fingerprint_summary(fingerprints: list[dict[str, Any]]) -> dict[str, Any]:
    if not fingerprints:
        return {"count": 0}
    static_values = [
        str(fp["static_prefix_sha256"])
        for fp in fingerprints
        if fp.get("static_prefix_sha256")
    ]
    unique_static = sorted(set(static_values))
    return {
        "count": len(fingerprints),
        "unique_static_prefix_count": len(unique_static),
        "static_prefix_stable_within_run": len(unique_static) <= 1,
        "first_static_prefix_sha256": static_values[0] if static_values else None,
    }


def assert_prefix_fingerprint_stability(
    run_reports: list[dict[str, Any]],
    *,
    require_fingerprints: bool = False,
    stability_batch_shapes: set[str] | None = None,
) -> list[str]:
    """Return failure messages for M5 live corpus gate (empty list = pass).

    Skips infra/timed_out runs. Pre-M5 captures with no fingerprint events are
    ignored unless ``require_fingerprints`` is set (post-M5 live replay).

    Static-prefix stability is checked only for ``stability_batch_shapes`` when
    set (default from CLI: ``pure_read``). Write/shell turns often activate
    deferred tools or refresh the system layer mid-turn, which legitimately
    changes the tool-catalog slice hashed into ``static_prefix_sha256``.
    """
    failures: list[str] = []
    for run in run_reports:
        if run.get("infra") or run.get("timed_out"):
            continue
        scenario_id = run.get("scenario_id", "unknown")
        repeat_index = run.get("repeat_index")
        batch_shape = str(run.get("batch_shape") or "unknown")
        summary = run.get("prefix_fingerprint_summary") or {}
        count = int(summary.get("count") or 0)
        if count == 0:
            if require_fingerprints:
                failures.append(
                    f"{scenario_id} r{repeat_index}: no turn.prefix_fingerprint events"
                )
            continue
        if stability_batch_shapes is not None and batch_shape not in stability_batch_shapes:
            continue
        if not summary.get("static_prefix_stable_within_run", False):
            unique = summary.get("unique_static_prefix_count", "?")
            failures.append(
                f"{scenario_id} r{repeat_index} ({batch_shape}): static prefix unstable "
                f"({unique} distinct hashes across {count} model steps)"
            )
    return failures


def percentile(values: list[float], pct: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    idx = min(len(ordered) - 1, max(0, round(pct / 100.0 * (len(ordered) - 1))))
    return round(ordered[idx], 1)


def summarize(values: list[float]) -> dict[str, float]:
    if not values:
        return {"count": 0, "mean": 0.0, "p50": 0.0, "p95": 0.0}
    return {
        "count": len(values),
        "mean": round(statistics.fmean(values), 1),
        "p50": percentile(values, 50),
        "p95": percentile(values, 95),
    }


def load_shape_map(spec_path: Path | None) -> dict[str, dict[str, Any]]:
    if spec_path is None or not spec_path.is_file():
        return {}
    raw = tomllib.loads(spec_path.read_text(encoding="utf-8"))
    return {
        str(task.get("id")): {
            "batch_shape": task.get("batch_shape", "unknown"),
            "long_thinking": bool(task.get("long_thinking", False)),
        }
        for task in raw.get("task") or []
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Kernel v2 corpus per-step latency report")
    parser.add_argument("run_dir", help="Run directory produced by kernel-v2-corpus-run.ps1")
    parser.add_argument("--spec", default="", help="scenarios.toml (for batch_shape mapping)")
    parser.add_argument("--out", default="", help="Output path (default <run_dir>/report.json)")
    parser.add_argument(
        "--assert-prefix-stability",
        action="store_true",
        help="Exit 1 when any successful run has unstable static_prefix within the turn",
    )
    parser.add_argument(
        "--require-fingerprints",
        action="store_true",
        help="With --assert-prefix-stability, fail runs missing turn.prefix_fingerprint events",
    )
    parser.add_argument(
        "--prefix-stability-shapes",
        default="pure_read",
        help="Comma-separated batch_shape values that must keep static_prefix stable "
        "(default: pure_read; use 'all' for every shape)",
    )
    args = parser.parse_args()

    run_dir = Path(args.run_dir)
    runs_jsonl = run_dir / "runs.jsonl"
    if not runs_jsonl.is_file():
        print(f"runs.jsonl not found in {run_dir}", file=sys.stderr)
        return 1

    shape_map = load_shape_map(Path(args.spec) if args.spec else None)

    run_reports: list[dict[str, Any]] = []
    for line in runs_jsonl.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        run = json.loads(line)
        scenario_id = run.get("scenario_id", "unknown")
        shape_info = shape_map.get(scenario_id, {})
        batch_shape = run.get("batch_shape") or shape_info.get("batch_shape", "unknown")

        events_file = run_dir / str(run.get("events_file") or "")
        thread_file = run_dir / str(run.get("thread_file") or "")
        report: dict[str, Any] = {
            "scenario_id": scenario_id,
            "repeat_index": run.get("repeat_index"),
            "batch_shape": batch_shape,
            "long_thinking": bool(run.get("long_thinking") or shape_info.get("long_thinking")),
            "turn_status": run.get("turn_status"),
            "infra": bool(run.get("infra")),
            "timed_out": bool(run.get("timed_out")),
            "steps": [],
            "usage": {},
        }
        if events_file.is_file():
            records = parse_sse_records(events_file)
            thread_detail = None
            if thread_file.is_file():
                try:
                    thread_detail = json.loads(thread_file.read_text(encoding="utf-8"))
                except json.JSONDecodeError:
                    thread_detail = None
            report["steps"] = extract_steps(records)
            report["usage"] = extract_usage(records, thread_detail)
            report["prefix_fingerprints"] = extract_prefix_fingerprints(records)
            report["prefix_fingerprint_summary"] = prefix_fingerprint_summary(
                report["prefix_fingerprints"]
            )
        run_reports.append(report)

    aggregates: dict[str, dict[str, Any]] = {}
    for shape in sorted({r["batch_shape"] for r in run_reports}):
        shape_runs = [
            r
            for r in run_reports
            if r["batch_shape"] == shape and not r["infra"] and not r["timed_out"]
        ]
        tool_steps = [s for r in shape_runs for s in r["steps"] if s["tools"]]
        all_steps = [s for r in shape_runs for s in r["steps"]]
        hit_rates = [
            r["usage"]["cache_hit_rate"] for r in shape_runs if "cache_hit_rate" in r["usage"]
        ]
        aggregates[shape] = {
            "runs": len(shape_runs),
            "step_total_ms": summarize([s["total_ms"] for s in all_steps]),
            "tool_step_total_ms": summarize([s["total_ms"] for s in tool_steps]),
            "model_phase_ms": summarize([s["model_phase_ms"] for s in tool_steps]),
            "tool_phase_ms": summarize([s["tool_phase_ms"] for s in tool_steps]),
            "cache_hit_rate_mean": round(statistics.fmean(hit_rates), 4) if hit_rates else None,
            "input_tokens_total": sum(r["usage"].get("input_tokens") or 0 for r in shape_runs),
            "output_tokens_total": sum(r["usage"].get("output_tokens") or 0 for r in shape_runs),
        }

    report = {
        "schema_version": 1,
        "run_dir": str(run_dir),
        "generated_at": datetime.now().astimezone().isoformat(timespec="seconds"),
        "runs": run_reports,
        "aggregate_by_batch_shape": aggregates,
    }
    out_path = Path(args.out) if args.out else run_dir / "report.json"
    out_path.write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )

    print(f"\n{'shape':<18} {'runs':>4} {'steps':>5} {'step p50':>9} {'step p95':>9} {'tool p50':>9} {'cache':>6}")
    for shape, agg in aggregates.items():
        cache = agg["cache_hit_rate_mean"]
        cache_text = f"{cache * 100:.0f}%" if cache is not None else "n/a"
        print(
            f"{shape:<18} {agg['runs']:>4} {agg['step_total_ms']['count']:>5} "
            f"{agg['step_total_ms']['p50']:>9} {agg['step_total_ms']['p95']:>9} "
            f"{agg['tool_phase_ms']['p50']:>9} {cache_text:>6}"
        )
    print(f"\nreport: {out_path}")

    if args.assert_prefix_stability:
        shape_arg = (args.prefix_stability_shapes or "").strip().lower()
        if shape_arg in ("", "all", "*"):
            stability_shapes: set[str] | None = None
        else:
            stability_shapes = {
                s.strip() for s in args.prefix_stability_shapes.split(",") if s.strip()
            }
        failures = assert_prefix_fingerprint_stability(
            run_reports,
            require_fingerprints=args.require_fingerprints,
            stability_batch_shapes=stability_shapes,
        )
        if failures:
            print("\nprefix stability gate FAILED:", file=sys.stderr)
            for msg in failures:
                print(f"  - {msg}", file=sys.stderr)
            return 1
        checked = sum(
            1
            for r in run_reports
            if not r.get("infra")
            and not r.get("timed_out")
            and (r.get("prefix_fingerprint_summary") or {}).get("count", 0) > 0
        )
        print(
            f"\nprefix stability gate OK ({checked} run(s) with fingerprint events)"
        )

    return 0


if __name__ == "__main__":
    sys.exit(main())
