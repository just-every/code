#!/usr/bin/env python3
"""Fetch a single Byterover bulk export and cache the JSON payload.

The migration run on 2025-10-02 will use this helper to materialise a single
batched snapshot of Byterover memories.  The script is intentionally thin: it
invokes the configured Byterover export command once, validates the returned
JSON, and writes both the raw entries and a lightweight summary to disk.

Usage example (command provided via CLI):

    scripts/spec-kit/fetch_byterover_bulk.py \
        --command coder mcp call byterover-memories bulk-export \
        --out-json docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/\\
SPEC-KIT-010/byterover_export_latest.json

If the `--command` flag is omitted the script falls back to the
`BYTEROVER_EXPORT_COMMAND` environment variable, which should contain a shell
command string (e.g. `coder mcp call byterover-memories bulk-export`).
"""

from __future__ import annotations

import argparse
import json
import os
import shlex
import subprocess
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, Iterable, List, Sequence


EVIDENCE_ROOT = Path(
    "docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-010"
)


def parse_command(cli_args: Sequence[str] | None) -> List[str]:
    if cli_args:
        return list(cli_args)

    env_value = os.environ.get("BYTEROVER_EXPORT_COMMAND")
    if env_value:
        return shlex.split(env_value)

    raise SystemExit(
        "No Byterover export command provided. Pass --command or set "
        "BYTEROVER_EXPORT_COMMAND."
    )


def run_export(command: Sequence[str], timeout: int | None) -> Dict[str, Any]:
    try:
        result = subprocess.run(  # noqa: S603 (intentional subprocess)
            command,
            check=True,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.CalledProcessError as exc:  # pragma: no cover - defensive
        raise SystemExit(
            f"Byterover export command failed with code {exc.returncode}: {exc.stderr}"
        ) from exc
    except subprocess.TimeoutExpired as exc:  # pragma: no cover - defensive
        raise SystemExit(
            f"Byterover export command timed out after {timeout} seconds"
        ) from exc

    output = result.stdout.strip()
    if not output:
        raise SystemExit("Byterover export command produced no stdout")

    try:
        payload = json.loads(output)
    except json.JSONDecodeError as exc:  # pragma: no cover - defensive
        raise SystemExit(f"Export output is not valid JSON: {exc}") from exc

    return payload


def extract_entries(payload: Dict[str, Any]) -> List[Dict[str, Any]]:
    if isinstance(payload, list):
        return payload
    if isinstance(payload, dict):
        if "entries" in payload:
            entries = payload["entries"]
            if isinstance(entries, list):
                return entries
            raise SystemExit("`entries` field is not a list in Byterover payload")
        return [payload]
    raise SystemExit("Unexpected Byterover export shape; expected list or dict")


def summarise(entries: Iterable[Dict[str, Any]]) -> Dict[str, Any]:
    domain_counts: Counter[str] = Counter()
    tag_counts: Counter[str] = Counter()

    total = 0
    for entry in entries:
        total += 1
        domain = (
            entry.get("domain")
            or entry.get("target_domain")
            or entry.get("workspace_domain")
            or "unknown"
        )
        domain_counts[domain] += 1

        tags = entry.get("tags") or []
        if isinstance(tags, list):
            tag_counts.update(str(tag) for tag in tags)

    return {
        "total_entries": total,
        "domains": dict(sorted(domain_counts.items(), key=lambda kv: (-kv[1], kv[0]))),
        "top_tags": dict(sorted(tag_counts.items(), key=lambda kv: (-kv[1], kv[0]))[:50]),
    }


def build_output_path(base: Path | None, timestamp: datetime) -> Path:
    if base:
        return base

    filename = f"byterover_export_{timestamp.strftime('%Y%m%dT%H%M%SZ')}.json"
    return EVIDENCE_ROOT / filename


def write_payload(path: Path, data: Dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(data, handle, indent=2, sort_keys=True)
        handle.write("\n")


def main() -> int:
    parser = argparse.ArgumentParser(description="Fetch and cache Byterover export")
    parser.add_argument(
        "--command",
        nargs="+",
        help="Command (and args) that emits Byterover export JSON to stdout",
    )
    parser.add_argument(
        "--out-json",
        type=Path,
        help=(
            "Where to store the cached export. Defaults to the SPEC-KIT-010 evidence "
            "folder with a timestamped filename."
        ),
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=120,
        help="Seconds to wait for the export command (default: 120)",
    )
    parser.add_argument(
        "--pretty",
        action="store_true",
        help="Print a human summary to stdout after caching",
    )
    args = parser.parse_args()

    command = parse_command(args.command)
    timestamp = datetime.now(timezone.utc)
    payload = run_export(command, args.timeout)
    entries = extract_entries(payload)
    summary = summarise(entries)

    output_path = build_output_path(args.out_json, timestamp)
    cache_payload = {
        "generated_at": timestamp.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "command": command,
        "summary": summary,
        "entries": entries,
    }
    write_payload(output_path, cache_payload)

    if args.pretty:
        print(f"Cached {summary['total_entries']} entries → {output_path}")
        if summary["domains"]:
            print("Top domains:")
            for domain, count in list(summary["domains"].items())[:10]:
                print(f"  - {domain}: {count}")

    return 0


if __name__ == "__main__":  # pragma: no cover - CLI entry point
    raise SystemExit(main())
