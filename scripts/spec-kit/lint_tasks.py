#!/usr/bin/env python3
"""Validate SPEC.md Tasks table structure and metadata."""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, List

EXPECTED_HEADER = "| Order | Task ID | Title | Status | Owners | PRD | Branch | PR | Last Validation | Evidence | Notes |"
EXPECTED_COLUMNS = [
    "Order",
    "Task ID",
    "Title",
    "Status",
    "Owners",
    "PRD",
    "Branch",
    "PR",
    "Last Validation",
    "Evidence",
    "Notes",
]

ALLOWED_STATUS = {"Backlog", "In Progress", "In Review", "Blocked", "Done"}
DATE_PATTERN = re.compile(r"^\d{4}-\d{2}-\d{2}$")


@dataclass
class TaskRow:
    order: str
    task_id: str
    title: str
    status: str
    owners: str
    prd: str
    branch: str
    pr: str
    last_validation: str
    evidence: str
    notes: str


def read_spec_tasks(spec_path: Path) -> List[TaskRow]:
    text = spec_path.read_text(encoding="utf-8")
    lines = text.splitlines()
    header_idx = None
    for idx, line in enumerate(lines):
        if line.strip().startswith("| Order | Task ID | Title | Status | Owners | PRD | Branch | PR | Last Validation | Evidence | Notes |"):
            header_idx = idx
            break
    if header_idx is None:
        raise ValueError("Tasks table header not found")
    if lines[header_idx].strip() != EXPECTED_HEADER:
        raise ValueError("Tasks table header does not match expected schema")

    # find end of table
    def is_table_line(line: str) -> bool:
        stripped = line.strip()
        return bool(stripped and (stripped.startswith("|") or stripped.startswith("HEAD|")))

    end_idx = header_idx + 1
    while end_idx < len(lines) and is_table_line(lines[end_idx]):
        end_idx += 1

    rows: List[TaskRow] = []
    for raw in lines[header_idx + 2 : end_idx]:
        stripped = raw.strip()
        if not stripped:
            continue
        if stripped.startswith("HEAD|"):
            stripped = "|" + stripped
        if not stripped.startswith("|"):
            continue
        cells = [cell.strip() for cell in stripped.split("|")[1:-1]]
        if len(cells) != len(EXPECTED_COLUMNS):
            raise ValueError(f"Unexpected column count in row: {raw}")
        rows.append(TaskRow(*cells))
    return rows


def validate_rows(rows: Iterable[TaskRow], repo_root: Path) -> List[str]:
    failures: List[str] = []
    seen_orders = set()
    for row in rows:
        if row.order in seen_orders:
            failures.append(f"Duplicate order value: {row.order} ({row.task_id})")
        seen_orders.add(row.order)

        if row.status not in ALLOWED_STATUS:
            failures.append(f"{row.task_id}: invalid status '{row.status}'")

        if row.status in {"In Progress", "In Review"} and not row.owners:
            failures.append(f"{row.task_id}: Owners required for status {row.status}")

        if row.prd:
            for prd_path in [part.strip() for part in row.prd.split(',') if part.strip()]:
                if not (repo_root / prd_path).exists():
                    failures.append(f"{row.task_id}: PRD path missing ({prd_path})")

        if row.evidence:
            evidence_path = repo_root / row.evidence
            if not evidence_path.exists():
                failures.append(f"{row.task_id}: Evidence path missing ({row.evidence})")

        if row.last_validation and not DATE_PATTERN.match(row.last_validation):
            failures.append(f"{row.task_id}: Last Validation must be YYYY-MM-DD (got '{row.last_validation}')")

        if row.branch and " " in row.branch:
            failures.append(f"{row.task_id}: Branch should not contain spaces ({row.branch})")

    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description="Lint SPEC.md Tasks table")
    parser.add_argument("spec_file", nargs="?", default="SPEC.md", help="Path to SPEC.md (default: SPEC.md)")
    args = parser.parse_args()

    spec_path = Path(args.spec_file)
    if not spec_path.exists():
        print(f"ERROR: {spec_path} not found", file=sys.stderr)
        return 2

    try:
        rows = read_spec_tasks(spec_path)
    except ValueError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1

    failures = validate_rows(rows, spec_path.parent)
    if failures:
        print("Tasks table lint failed:\n" + "\n".join(f" - {msg}" for msg in failures), file=sys.stderr)
        return 1

    print(f"Tasks table lint passed ({len(rows)} rows)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
