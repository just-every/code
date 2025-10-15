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
import os
import sqlite3
import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, Iterable, List, Sequence


DEFAULT_IMPORTANCE = 7


@dataclass
class MigrationEntry:
    id: str
    content: str
    domain: str
    tags: List[str]
    importance: int
    slug: str | None = None
    created_at: str | None = None
    updated_at: str | None = None
    source: str | None = "byterover"


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

    entry_id = raw.get("id") or raw.get("memory_id") or str(uuid.uuid4())
    slug = raw.get("slug")

    created = raw.get("created_at")
    updated = raw.get("updated_at") or created

    return MigrationEntry(
        id=str(entry_id),
        content=content,
        domain=domain,
        tags=tags,
        importance=importance,
        slug=slug,
        created_at=created,
        updated_at=updated,
        source=raw.get("source") or "byterover",
    )


def resolve_database(explicit: Path | None) -> Path:
    if explicit:
        return explicit

    env_home = os.environ.get("LOCAL_MEMORY_HOME")
    if env_home:
        candidate = Path(env_home) / "unified-memories.db"
        if candidate.exists():
            return candidate

    default_path = Path.home() / ".local-memory" / "unified-memories.db"
    if default_path.exists():
        return default_path

    raise FileNotFoundError("local-memory database not found; specify --database")


def ensure_timestamp(value: str | None) -> str:
    if value:
        return value
    return datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S%z")


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
    parser.add_argument("--domains", nargs="*", help="Optional domain allow-list (e.g. spec-tracker docs-ops)")
    parser.add_argument("--database", type=Path, help="Path to local-memory SQLite database")
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
    if args.domains:
        allowed = {d.lower() for d in args.domains}
        entries = [e for e in entries if e.domain.lower() in allowed]

    report = {
        "mode": "dry-run" if args.dry_run else "apply",
        "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "source_files": [str(p) for p in args.source],
        "summary": summarise(entries),
        "results": [],
    }

    if args.apply:
        db_path = resolve_database(args.database)
        conn = sqlite3.connect(db_path)
        try:
            conn.execute("PRAGMA foreign_keys = ON")
            existing_ids = {row[0] for row in conn.execute("SELECT id FROM memories")}
            existing_slugs = {
                row[0]
                for row in conn.execute("SELECT slug FROM memories WHERE slug IS NOT NULL")
            }

            inserted = 0
            skipped = 0
            for entry in entries:
                if entry.id in existing_ids:
                    report["results"].append(
                        {
                            "status": "skipped_existing_id",
                            "id": entry.id,
                            "domain": entry.domain,
                            "slug": entry.slug,
                        }
                    )
                    skipped += 1
                    continue

                if entry.slug and entry.slug in existing_slugs:
                    report["results"].append(
                        {
                            "status": "skipped_existing_slug",
                            "id": entry.id,
                            "domain": entry.domain,
                            "slug": entry.slug,
                        }
                    )
                    skipped += 1
                    continue

                created = ensure_timestamp(entry.created_at)
                updated = ensure_timestamp(entry.updated_at)
                tags_json = json.dumps(entry.tags, ensure_ascii=False)

                conn.execute(
                    """
                    INSERT INTO memories (
                        id, content, source, importance, tags, session_id, domain,
                        created_at, updated_at, agent_type, agent_context, access_scope, slug
                    ) VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?, NULL, NULL, NULL, ?)
                    """,
                    (
                        entry.id,
                        entry.content,
                        entry.source,
                        entry.importance,
                        tags_json,
                        entry.domain,
                        created,
                        updated,
                        entry.slug,
                    ),
                )

                existing_ids.add(entry.id)
                if entry.slug:
                    existing_slugs.add(entry.slug)
                inserted += 1
                report["results"].append(
                    {
                        "status": "inserted",
                        "id": entry.id,
                        "domain": entry.domain,
                        "slug": entry.slug,
                    }
                )

            conn.commit()
            report["summary"].update({
                "inserted": inserted,
                "skipped": skipped,
                "database": str(db_path),
            })
        finally:
            conn.close()

    if args.out_json:
        write_report(args.out_json, report)
    else:
        print(json.dumps(report, indent=2, sort_keys=True))

    return 0


if __name__ == "__main__":  # pragma: no cover - CLI entry point
    raise SystemExit(main())
