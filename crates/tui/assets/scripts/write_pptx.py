#!/usr/bin/env python3
"""write_pptx.py — thin CLI wrapper for pptx_engine.

Called by WriteOfficeTool via venv Python.
Args: --output PATH  [--input FILE]   (if --input omitted, reads JSON from stdin)

Payload schema: see docs/pptx-generation-engine-plan.md and
pptx_engine package docstrings.

Supported (backward-compatible):
  - slides[].chart, slides[].table, slides[].bullets (old pipeline)
  - slides[].blocks + slides[].layout (new pipeline with grid layout)
  - slides[].notes (speaker notes)
  - slides[].theme (per-slide override)

Chart types: bar, line, pie, stacked_bar, stacked_bar_pct, area, scatter, donut, combo
"""

import argparse
import json
import sys

from pptx_engine import build_presentation


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True)
    parser.add_argument("--input", default=None)
    args = parser.parse_args()

    if args.input:
        with open(args.input, "r", encoding="utf-8") as f:
            payload = json.load(f)
    else:
        payload = json.load(sys.stdin)

    prs = build_presentation(payload)
    prs.save(args.output)
    print("OK")


if __name__ == "__main__":
    main()
