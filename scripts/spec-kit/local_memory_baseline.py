#!/usr/bin/env python3
"""Summarise a local-memory JSONL export.

Reads the export produced by `code local-memory export`, aggregates counts by
domain and primary tags, and emits both JSON and Markdown reports for evidence
logging. No Byterover access is required.
"""

from __future__ import annotations

import argparse
import collections
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, Iterable, Tuple


def load_entries(path: Path) -> Iterable[Dict[str, Any]]:
    with path.open("r", encoding="utf-8") as handle:
        for line_no, raw in enumerate(handle, start=1):
            raw = raw.strip()
            if not raw:
                continue
            try:
                yield json.loads(raw)
            except json.JSONDecodeError as exc:  # pragma: no cover - defensive
                raise ValueError(f"line {line_no}: invalid JSON: {exc}") from exc


def summarise(entries: Iterable[Dict[str, Any]]) -> Dict[str, Any]:
    domain_counts: collections.Counter[str] = collections.Counter()
    tag_counts: collections.Counter[str] = collections.Counter()
    importance_hist: collections.Counter[int] = collections.Counter()
    total = 0

    for entry in entries:
        total += 1
        domain = entry.get("domain") or "unknown"
        domain_counts[domain] += 1

        for tag in entry.get("tags", []) or []:
            tag_counts[tag] += 1

        importance = int(entry.get("importance", 0))
        importance_hist[importance] += 1

    return {
        "total_entries": total,
        "domains": dict(sorted(domain_counts.items(), key=lambda kv: (-kv[1], kv[0]))),
        "tags": dict(sorted(tag_counts.items(), key=lambda kv: (-kv[1], kv[0]))),
        "importance_histogram": dict(sorted(importance_hist.items())),
    }


def render_markdown(summary: Dict[str, Any], source: Path) -> str:
    lines = [
        f"# Local-memory Baseline Report",
        "",
        f"Source export: `{source}`",
        "",
        f"Total entries: **{summary['total_entries']}**",
        "",
        "## Entries per domain",
    ]

    if summary["domains"]:
        for domain, count in summary["domains"].items():
            lines.append(f"- `{domain}`: {count}")
    else:
        lines.append("- _no domains recorded_")

    lines.extend(["", "## Top tags"])
    if summary["tags"]:
        for tag, count in list(summary["tags"].items())[:50]:
            lines.append(f"- `{tag}`: {count}")
    else:
        lines.append("- _no tags recorded_")

    lines.extend(["", "## Importance histogram"])
    if summary["importance_histogram"]:
        for level, count in summary["importance_histogram"].items():
            lines.append(f"- Importance {level}: {count}")
    else:
        lines.append("- _no importance values recorded_")

    generated = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    lines.extend(["", f"Generated at: `{generated}` UTC"])
    return "\n".join(lines) + "\n"


def write_payload(path: Path, payload: Dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")


def write_markdown(path: Path, markdown: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(markdown, encoding="utf-8")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Summarise a local-memory export")
    parser.add_argument("--input", required=True, type=Path, help="Path to JSONL export")
    parser.add_argument("--out-json", type=Path, help="Where to write the JSON summary")
    parser.add_argument("--out-md", type=Path, help="Where to write the Markdown report")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    entries = list(load_entries(args.input))
    summary = summarise(entries)

    if args.out_json:
        payload = {
            "source": str(args.input),
            "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "summary": summary,
        }
        write_payload(args.out_json, payload)

    if args.out_md:
        markdown = render_markdown(summary, args.input)
        write_markdown(args.out_md, markdown)

    if not args.out_json and not args.out_md:
        print(json.dumps(summary, indent=2, sort_keys=True))

    return 0


if __name__ == "__main__":  # pragma: no cover - CLI entry point
    raise SystemExit(main())
