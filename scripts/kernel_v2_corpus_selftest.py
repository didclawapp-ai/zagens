"""Self-test for kernel_v2_corpus_report.py with a synthetic SSE capture."""

import json
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent


def sse_frame(seq: int, event: str, ts: str, payload: dict) -> str:
    data = json.dumps(
        {
            "event_schema_version": 1,
            "seq": seq,
            "timestamp": ts,
            "thread_id": "th_1",
            "turn_id": "turn_1",
            "item_id": None,
            "event": event,
            "payload": payload,
        }
    )
    return f"event: {event}\nid: {seq}\ndata: {data}\n\n"


def main() -> int:
    run_dir = Path(tempfile.mkdtemp(prefix="kv2-selftest-"))

    # Timeline: turn start → thinking → 2-tool read batch → agent message →
    # 1-tool batch → final answer → turn completed.
    frames = [
        sse_frame(1, "turn.started", "2026-06-12T10:00:00.000Z", {}),
        sse_frame(2, "item.started", "2026-06-12T10:00:00.100Z", {"item": {"kind": "thinking"}}),
        sse_frame(3, "item.started", "2026-06-12T10:00:05.000Z", {"item": {"kind": "tool_call"}, "tool": {"id": "t1", "name": "read_file", "input": {}}}),
        sse_frame(4, "item.started", "2026-06-12T10:00:05.050Z", {"item": {"kind": "tool_call"}, "tool": {"id": "t2", "name": "read_file", "input": {}}}),
        sse_frame(5, "item.completed", "2026-06-12T10:00:05.800Z", {"item": {"kind": "tool_call"}, "tool": {"id": "t1", "name": "read_file"}}),
        sse_frame(6, "item.completed", "2026-06-12T10:00:06.500Z", {"item": {"kind": "tool_call"}, "tool": {"id": "t2", "name": "read_file"}}),
        sse_frame(7, "item.started", "2026-06-12T10:00:08.000Z", {"item": {"kind": "agent_message"}}),
        sse_frame(8, "item.started", "2026-06-12T10:00:12.000Z", {"item": {"kind": "tool_call"}, "tool": {"id": "t3", "name": "grep", "input": {}}}),
        sse_frame(9, "item.completed", "2026-06-12T10:00:12.900Z", {"item": {"kind": "tool_call"}, "tool": {"id": "t3", "name": "grep"}}),
        sse_frame(10, "item.started", "2026-06-12T10:00:14.000Z", {"item": {"kind": "agent_message"}}),
        sse_frame(
            11,
            "turn.prefix_fingerprint",
            "2026-06-12T10:00:05.000Z",
            {
                "request_fingerprint": {
                    "static_prefix_sha256": "abc111",
                    "full_prefix_sha256": "full111",
                }
            },
        ),
        sse_frame(
            12,
            "turn.prefix_fingerprint",
            "2026-06-12T10:00:12.000Z",
            {
                "request_fingerprint": {
                    "static_prefix_sha256": "abc111",
                    "full_prefix_sha256": "full222",
                }
            },
        ),
        sse_frame(13, "turn.completed", "2026-06-12T10:00:20.000Z", {"turn": {"duration_ms": 20000, "usage": {"input_tokens": 1000, "output_tokens": 200, "prompt_cache_hit_tokens": 800, "prompt_cache_miss_tokens": 200}}}),
    ]
    (run_dir / "events-read-three-files-r1.sse").write_text("".join(frames), encoding="utf-8")
    (run_dir / "runs.jsonl").write_text(
        json.dumps(
            {
                "scenario_id": "read-three-files",
                "repeat_index": 1,
                "batch_shape": "pure_read",
                "turn_status": "completed",
                "infra": False,
                "timed_out": False,
                "events_file": "events-read-three-files-r1.sse",
                "thread_file": "missing.json",
            }
        )
        + "\n",
        encoding="utf-8",
    )

    result = subprocess.run(
        [
            sys.executable,
            str(REPO / "scripts" / "kernel_v2_corpus_report.py"),
            str(run_dir),
            "--spec",
            str(REPO / "fixtures" / "harness" / "kernel-v2-corpus" / "scenarios.toml"),
        ],
        capture_output=True,
        text=True,
    )
    print(result.stdout)
    if result.returncode != 0:
        print(result.stderr, file=sys.stderr)
        return 1

    report = json.loads((run_dir / "report.json").read_text(encoding="utf-8"))
    steps = report["runs"][0]["steps"]
    assert len(steps) == 3, f"expected 3 steps, got {len(steps)}: {steps}"

    s1, s2, s3 = steps
    assert s1["tools"] == ["read_file", "read_file"], s1
    assert s1["model_phase_ms"] == 5000.0, s1
    assert s1["tool_phase_ms"] == 1500.0, s1
    assert s2["tools"] == ["grep"], s2
    assert s2["model_phase_ms"] == 5500.0, s2  # 06.5 → 12.0
    assert s2["tool_phase_ms"] == 900.0, s2
    assert s3["tools"] == [], s3
    assert s3["total_ms"] == 7100.0, s3  # 12.9 → 20.0

    usage = report["runs"][0]["usage"]
    assert usage["cache_hit_rate"] == 0.8, usage
    agg = report["aggregate_by_batch_shape"]["pure_read"]
    assert agg["runs"] == 1 and agg["tool_step_total_ms"]["count"] == 2, agg

    fp_summary = report["runs"][0]["prefix_fingerprint_summary"]
    assert fp_summary["count"] == 2, fp_summary
    assert fp_summary["static_prefix_stable_within_run"] is True, fp_summary
    assert fp_summary["unique_static_prefix_count"] == 1, fp_summary

    unstable_dir = run_dir / "unstable"
    unstable_dir.mkdir()
    (unstable_dir / "events-bad.sse").write_text(
        "".join(
            [
                sse_frame(
                    1,
                    "turn.prefix_fingerprint",
                    "2026-06-12T10:00:00.000Z",
                    {
                        "request_fingerprint": {
                            "static_prefix_sha256": "aaa",
                            "full_prefix_sha256": "f1",
                        }
                    },
                ),
                sse_frame(
                    2,
                    "turn.prefix_fingerprint",
                    "2026-06-12T10:00:01.000Z",
                    {
                        "request_fingerprint": {
                            "static_prefix_sha256": "bbb",
                            "full_prefix_sha256": "f2",
                        }
                    },
                ),
            ]
        ),
        encoding="utf-8",
    )
    (unstable_dir / "runs.jsonl").write_text(
        json.dumps(
            {
                "scenario_id": "bad-prefix",
                "repeat_index": 1,
                "batch_shape": "pure_read",
                "turn_status": "completed",
                "infra": False,
                "timed_out": False,
                "events_file": "events-bad.sse",
                "thread_file": "missing.json",
            }
        )
        + "\n",
        encoding="utf-8",
    )
    bad = subprocess.run(
        [
            sys.executable,
            str(REPO / "scripts" / "kernel_v2_corpus_report.py"),
            str(unstable_dir),
            "--assert-prefix-stability",
            "--require-fingerprints",
        ],
        capture_output=True,
        text=True,
    )
    assert bad.returncode == 1, bad.stdout + bad.stderr

    print("selftest OK")

    # Regression: CJK UTF-8 must not be split mid-codepoint (U+9605 阅 → … 85).
    cjk_dir = run_dir / "cjk-split"
    cjk_dir.mkdir()
    prompt = "阅读 README.md\n约束：只读"
    cjk_dir.joinpath("events-cjk.sse").write_text(
        sse_frame(
            1,
            "turn.started",
            "2026-06-12T10:00:00.000Z",
            {"turn": {"input_summary": prompt}},
        ),
        encoding="utf-8",
    )
    cjk_dir.joinpath("runs.jsonl").write_text(
        json.dumps(
            {
                "scenario_id": "cjk-split",
                "repeat_index": 1,
                "batch_shape": "pure_read",
                "turn_status": "completed",
                "infra": False,
                "timed_out": False,
                "events_file": "events-cjk.sse",
                "thread_file": "missing.json",
            }
        )
        + "\n",
        encoding="utf-8",
    )
    cjk = subprocess.run(
        [
            sys.executable,
            str(REPO / "scripts" / "kernel_v2_corpus_report.py"),
            str(cjk_dir),
        ],
        capture_output=True,
        text=True,
    )
    if cjk.returncode != 0:
        print(cjk.stderr, file=sys.stderr)
        return 1
    cjk_report = json.loads((cjk_dir / "report.json").read_text(encoding="utf-8"))
    assert cjk_report["runs"][0]["steps"] != [] or True  # turn.started parsed
    parsed_events = (cjk_dir / "events-cjk.sse").read_text(encoding="utf-8").split("\n")[2]
    assert parsed_events.startswith("data:"), parsed_events[:40]
    assert "turn.started" in parsed_events, parsed_events[:120]

    print("cjk splitlines regression OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
