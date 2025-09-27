#!/usr/bin/env python3
"""Local-memory migration utility (Byterover → local-memory).

Dry-run and apply modes operate on a cached Byterover export (JSON) so the
migration can be rehearsed without consuming remote retrievals. Apply mode writes
entries into local-memory using the configurable CLI helper (defaults to the
`local-memory` binary included with Codex).
"""

from __future__ import annotations

import argparse
import json
import subprocess
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, Iterable, List, Sequence


DEFAULT_CLI = "local-memory"
DEFAULT_IMPORTANCE = 7


@dataclass
class MigrationEntry:
    content: str
    domain: str
    tags: List[str]
    importance: int
    created_at: str | None = None
    updated_at: str | None = None

    def to_cli_args(self) -> List[str]:
        args = ["remember", self.content]
        args += ["--importance", str(self.importance)]
        if self.domain:
            args += ["--domain", self.domain]
        for tag in self.tags:
            args += ["--tags", tag]
        if self.created_at:
            args += ["--created-at", self.created_at]
        if self.updated_at:
            args += ["--updated-at", self.updated_at]
        return args


def load_sources(paths: Sequence[Path]) -> List[Dict[str, Any]]:
    entries: List[Dict[str, Any]] = []
    for path in paths:
        with path.open("r", encoding="utf-8") as handle:
            data = json.load(handle)
            if isinstance(data, dict) and "entries" in data:
                data = data["entries"]
            if not isinstance(data, list):  # pragma: no cover - defensive
                raise ValueError(f"Expected list in {path}, got {type(data).__name__}")
            entries.extend(data)
    return entries


def normalise(raw: Dict[str, Any]) -> MigrationEntry:
    content = raw.get("content") or raw.get("summary")
    if not content:
        raise ValueError("source entry missing content field")

    domain = raw.get("domain") or raw.get("target_domain") or "spec-tracker"
    tags = list(dict.fromkeys((raw.get("tags") or []) + raw.get("extra_tags", [])))
    importance = int(raw.get("importance") or DEFAULT_IMPORTANCE)

    created = raw.get("created_at")
    updated = raw.get("updated_at") or created

    return MigrationEntry(
        content=content,
        domain=domain,
        tags=tags,
        importance=importance,
        created_at=created,
        updated_at=updated,
    )


def run_cli(cli: str, entry: MigrationEntry) -> subprocess.CompletedProcess[str]:
    args = [cli] + entry.to_cli_args()
    return subprocess.run(
        args,
        check=False,
        capture_output=True,
        text=True,
    )


def summarise(entries: Iterable[MigrationEntry]) -> Dict[str, Any]:
    totals: Dict[str, int] = {}
    for entry in entries:
        totals[entry.domain] = totals.get(entry.domain, 0) + 1
    return {"total": sum(totals.values()), "domains": totals}


def write_report(path: Path, payload: Dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Migrate Byterover export into local-memory")
    parser.add_argument("--source", required=True, type=Path, nargs="+", help="Path(s) to cached Byterover export JSON")
    parser.add_argument("--dry-run", action="store_true", help="Preview without writing to local-memory")
    parser.add_argument("--apply", action="store_true", help="Apply migration to local-memory")
    parser.add_argument("--cli", default=DEFAULT_CLI, help="Local-memory CLI command (default: local-memory)")
    parser.add_argument("--out-json", type=Path, help="Where to write migration summary JSON")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.dry_run and args.apply:
        raise SystemExit("--dry-run and --apply are mutually exclusive")
    if not args.dry_run and not args.apply:
        raise SystemExit("Specify either --dry-run or --apply")

    raw_entries = load_sources(args.source)
    entries = [normalise(raw) for raw in raw_entries]

    report = {
        "mode": "dry-run" if args.dry_run else "apply",
        "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "source_files": [str(p) for p in args.source],
        "summary": summarise(entries),
        "results": [],
    }

    if args.apply:
        for entry in entries:
            result = run_cli(args.cli, entry)
            status = "success" if result.returncode == 0 else "error"
            report["results"].append(
                {
                    "status": status,
                    "domain": entry.domain,
                    "importance": entry.importance,
                    "tags": entry.tags,
                    "stderr": result.stderr.strip(),
                }
            )
            if result.returncode != 0:
                raise SystemExit(
                    f"local-memory remember failed (domain={entry.domain}): {result.stderr.strip()}"
                )

    if args.out_json:
        write_report(args.out_json, report)
    else:
        print(json.dumps(report, indent=2, sort_keys=True))

    return 0


if __name__ == "__main__":  # pragma: no cover - CLI entry point
    raise SystemExit(main())
