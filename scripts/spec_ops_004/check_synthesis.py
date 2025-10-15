#!/usr/bin/env python3
"""
Check synthesis.json status and exit accordingly.
Used by spec_auto.sh to validate consensus before advancing to next stage.

Exit codes:
  0 - Consensus OK
  1 - Synthesis file missing or invalid JSON
  2 - Conflict detected
  3 - Degraded consensus (missing agents)
"""

import sys
import json
import glob
from pathlib import Path

def find_latest_synthesis(spec_id: str, stage: str, evidence_root: str) -> Path | None:
    """Find most recent synthesis.json for given spec/stage."""
    pattern = f"{evidence_root}/consensus/{spec_id}/spec-{stage}_*_synthesis.json"
    matches = sorted(glob.glob(pattern), reverse=True)
    return Path(matches[0]) if matches else None

def check_synthesis(synthesis_path: Path) -> tuple[str, list[str]]:
    """Parse synthesis.json and return (status, conflicts)."""
    try:
        with open(synthesis_path) as f:
            data = json.load(f)
    except (FileNotFoundError, json.JSONDecodeError) as e:
        return ("invalid", [f"Failed to read synthesis: {e}"])

    status = data.get("status", "unknown")
    consensus = data.get("consensus", {})
    conflicts = consensus.get("conflicts", [])

    return (status, conflicts)

def main():
    if len(sys.argv) < 3:
        print("Usage: check_synthesis.py <SPEC-ID> <stage> [evidence-root]", file=sys.stderr)
        sys.exit(1)

    spec_id = sys.argv[1]
    stage = sys.argv[2]
    evidence_root = sys.argv[3] if len(sys.argv) > 3 else "docs/SPEC-OPS-004-integrated-coder-hooks/evidence"

    synthesis_path = find_latest_synthesis(spec_id, stage, evidence_root)

    if not synthesis_path:
        print(f"ERROR: No synthesis.json found for {spec_id}/{stage}", file=sys.stderr)
        print(f"Expected: {evidence_root}/consensus/{spec_id}/spec-{stage}_*_synthesis.json", file=sys.stderr)
        sys.exit(1)

    status, conflicts = check_synthesis(synthesis_path)

    if status == "ok":
        print(f"✓ Consensus OK for {stage}")
        print(f"  Synthesis: {synthesis_path}")
        sys.exit(0)
    elif status == "conflict":
        print(f"✗ Consensus CONFLICT for {stage}", file=sys.stderr)
        print(f"  Synthesis: {synthesis_path}", file=sys.stderr)
        print(f"  Conflicts:", file=sys.stderr)
        for conflict in conflicts:
            print(f"    - {conflict}", file=sys.stderr)
        sys.exit(2)
    elif status == "degraded":
        print(f"⚠ Consensus DEGRADED for {stage}", file=sys.stderr)
        print(f"  Synthesis: {synthesis_path}", file=sys.stderr)
        print(f"  Some agents failed or missing", file=sys.stderr)
        sys.exit(3)
    else:
        print(f"✗ Consensus INVALID status '{status}' for {stage}", file=sys.stderr)
        print(f"  Synthesis: {synthesis_path}", file=sys.stderr)
        sys.exit(1)

if __name__ == "__main__":
    main()
