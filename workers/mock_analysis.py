#!/usr/bin/env python3
"""Mock worker placeholder for the first Timbreprint MVP flow."""

from __future__ import annotations

import json
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: mock_analysis.py <job-dir>", file=sys.stderr)
        return 2

    job_dir = Path(sys.argv[1])
    analysis_path = job_dir / "analysis.json"
    if not analysis_path.exists():
        print(f"missing analysis file: {analysis_path}", file=sys.stderr)
        return 1

    analysis = json.loads(analysis_path.read_text(encoding="utf-8"))
    print(json.dumps({"status": "ok", "tempo": analysis["tempo"]["value"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
