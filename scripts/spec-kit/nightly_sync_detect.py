#!/usr/bin/env python3
"""Nightly sync drift detector.

Compares exported local-memory snapshots against guardrail evidence so
nightly jobs surface drift immediately.
"""

from __future__ import annotations

import argparse
import fnmatch
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Sequence, Set, Tuple

EVIDENCE_ROOT_DEFAULT = Path(
    "docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands"
)
EVIDENCE_REGEX = re.compile(
    r"(docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/[\\w./\\-]+)"
)
GLOBAL_SPEC = "GLOBAL"


@dataclass
class MemoryReference:
    memory_id: Optional[str]
    slug: Optional[str]
    summary: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Detect drift between local-memory exports and Spec Ops evidence.",
    )
    parser.add_argument(
        "--memory",
        type=Path,
        default=Path("tmp/memories.jsonl"),
        help="Path to the local-memory export JSONL (default: tmp/memories.jsonl).",
    )
    parser.add_argument(
        "--evidence-root",
        type=Path,
        default=EVIDENCE_ROOT_DEFAULT,
        help="Evidence artifacts root directory.",
    )
    parser.add_argument(
        "--spec",
        dest="spec_filter",
        action="append",
        help="Limit comparison to one or more SPEC IDs (repeatable).",
    )
    parser.add_argument(
        "--allowlist",
        type=Path,
        help="Optional newline-delimited glob list of paths that may drift.",
    )
    parser.add_argument(
        "--json-out",
        type=Path,
        help="Optional file path to write the JSON report.",
    )
    parser.add_argument(
        "--pretty",
        action="store_true",
        help="Pretty-print JSON output.",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        help="Repository root for rendering relative paths.",
    )
    return parser.parse_args()


def load_allowlist(path: Optional[Path]) -> List[str]:
    if path is None:
        return []
    if not path.exists():
        raise FileNotFoundError(f"Allowlist file not found: {path}")
    entries: List[str] = []
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        entries.append(line)
    return entries


def normalise_path(raw: str) -> Optional[str]:
    path = raw.strip().strip("`\"").rstrip(").,:;").replace("\\", "/")
    if not path:
        return None
    if not path.startswith(str(EVIDENCE_ROOT_DEFAULT)):
        return None
    return path


def extract_spec_id(path: str) -> Optional[str]:
    parts = Path(path).parts
    for idx, part in enumerate(parts):
        if part == "commands" and idx + 1 < len(parts):
            candidate = parts[idx + 1]
            if candidate.upper().startswith("SPEC-"):
                return candidate
    return None


def collect_memory_references(
    memory_jsonl: Path,
    spec_filter: Optional[Set[str]],
) -> Tuple[Dict[str, List[MemoryReference]], int]:
    if not memory_jsonl.exists():
        raise FileNotFoundError(f"Memory export not found: {memory_jsonl}")
    references: Dict[str, List[MemoryReference]] = {}
    entries_scanned = 0
    with memory_jsonl.open("r", encoding="utf-8") as handle:
        for idx, raw_line in enumerate(handle, 1):
            line = raw_line.strip()
            if not line:
                continue
            entries_scanned += 1
            try:
                record = json.loads(line)
            except json.JSONDecodeError as exc:
                raise ValueError(f"Invalid JSON on line {idx}: {exc}") from exc
            content = record.get("content", "")
            matches: List[str] = []
            for match in EVIDENCE_REGEX.finditer(content):
                normalised = normalise_path(match.group(1))
                if normalised is None:
                    continue
                spec_id = extract_spec_id(normalised)
                if spec_filter and (spec_id is None or spec_id not in spec_filter):
                    continue
                matches.append(normalised)
            if not matches:
                continue
            summary = content[:200].replace("\n", " ")
            memory_ref = MemoryReference(
                memory_id=record.get("id"),
                slug=record.get("slug"),
                summary=summary,
            )
            seen: Set[str] = set()
            for path in matches:
                if path in seen:
                    continue
                seen.add(path)
                references.setdefault(path, []).append(memory_ref)
    return references, entries_scanned


def collect_evidence_files(
    evidence_root: Path,
    repo_root: Path,
    spec_filter: Optional[Set[str]],
) -> Set[str]:
    if not evidence_root.exists():
        raise FileNotFoundError(f"Evidence directory not found: {evidence_root}")
    files: Set[str] = set()
    for file_path in evidence_root.rglob("*"):
        if not file_path.is_file():
            continue
        rel = relative_path(file_path, repo_root)
        spec_id = extract_spec_id(rel)
        if spec_filter and (spec_id is None or spec_id not in spec_filter):
            continue
        files.add(rel)
    return files


def relative_path(path: Path, repo_root: Path) -> str:
    try:
        return path.resolve().relative_to(repo_root).as_posix()
    except ValueError:
        return path.as_posix()


def is_allowed(path: str, patterns: Sequence[str]) -> bool:
    spec_id = extract_spec_id(path) or GLOBAL_SPEC
    for pattern in patterns:
        if fnmatch.fnmatch(path, pattern) or fnmatch.fnmatch(spec_id, pattern):
            return True
    return False


def build_report(
    memory_jsonl: Path,
    evidence_root: Path,
    repo_root: Path,
    spec_filter: Optional[Set[str]],
    allowlist: Sequence[str],
) -> Dict[str, object]:
    references, entries_scanned = collect_memory_references(
        memory_jsonl, spec_filter
    )
    evidence_files = collect_evidence_files(evidence_root, repo_root, spec_filter)

    missing_memory = []
    for path in sorted(evidence_files):
        if path in references or is_allowed(path, allowlist):
            continue
        missing_memory.append({"path": path, "spec": extract_spec_id(path) or GLOBAL_SPEC})

    missing_evidence = []
    for path, refs in sorted(references.items()):
        if path in evidence_files or is_allowed(path, allowlist):
            continue
        missing_evidence.append(
            {
                "path": path,
                "spec": extract_spec_id(path) or GLOBAL_SPEC,
                "memories": [
                    {
                        "id": ref.memory_id,
                        "slug": ref.slug,
                        "summary": ref.summary,
                    }
                    for ref in refs
                ],
            }
        )

    report = {
        "drift_detected": bool(missing_memory or missing_evidence),
        "missing_memory": missing_memory,
        "missing_evidence": missing_evidence,
        "stats": {
            "memory_entries_scanned": entries_scanned,
            "memory_reference_paths": len(references),
            "memory_reference_count": sum(len(refs) for refs in references.values()),
            "evidence_files_scanned": len(evidence_files),
            "allowlist_size": len(allowlist),
            "spec_filter": sorted(spec_filter) if spec_filter else [],
        },
    }
    return report


def print_human_report(
    report: Dict[str, object],
    memory_jsonl: Path,
    evidence_root: Path,
    spec_filter: Optional[Iterable[str]],
    repo_root: Path,
) -> None:
    stats = report["stats"]  # type: ignore[index]
    print("Nightly Sync Drift Detector")
    print(f"Memory file: {relative_path(memory_jsonl, repo_root)}")
    print(f"Evidence root: {relative_path(evidence_root, repo_root)}")
    if spec_filter:
        specs_display = ", ".join(sorted(spec_filter))
    else:
        specs_display = "(all)"
    print(f"Specs: {specs_display}")
    print(
        f"Memory entries scanned: {stats['memory_entries_scanned']} | "
        f"Evidence files scanned: {stats['evidence_files_scanned']}"
    )
    print(
        f"Referenced paths: {stats['memory_reference_paths']} "
        f"(entries: {stats['memory_reference_count']})"
    )
    print()

    missing_memory = report["missing_memory"]  # type: ignore[index]
    if missing_memory:
        print(f"Missing memory entries ({len(missing_memory)}):")
        for item in missing_memory:
            print(f"  - {item['path']} [{item['spec']}]")
    else:
        print("Missing memory entries: none.")
    print()

    missing_evidence = report["missing_evidence"]  # type: ignore[index]
    if missing_evidence:
        print(f"Missing evidence files ({len(missing_evidence)}):")
        for item in missing_evidence:
            refs = item["memories"]
            human_refs = ", ".join(
                filter(
                    None,
                    [ref.get("slug") or ref.get("id") for ref in refs],
                )
            )
            print(
                f"  - {item['path']} [{item['spec']}] referenced by "
                f"{human_refs or 'n/a'}"
            )
    else:
        print("Missing evidence files: none.")
    print()

    if report["drift_detected"]:
        print("Drift detected.")
    else:
        print("No drift detected.")


def emit_json_report(report: Dict[str, object], pretty: bool, out_path: Optional[Path]) -> None:
    if pretty:
        payload = json.dumps(report, indent=2, sort_keys=True)
    else:
        payload = json.dumps(report, separators=(",", ":"), sort_keys=True)
    if out_path:
        out_path.write_text(payload + "\n", encoding="utf-8")
    print()
    print("JSON report:")
    print(payload)


def resolve_with_root(path: Path, repo_root: Path) -> Path:
    if path.is_absolute():
        return path
    return (repo_root / path).resolve()


def main() -> int:
    args = parse_args()
    repo_root = (args.repo_root or Path.cwd()).resolve()
    spec_filter = {spec.upper() for spec in args.spec_filter} if args.spec_filter else None

    try:
        allowlist = load_allowlist(args.allowlist)
        memory_jsonl = resolve_with_root(args.memory, repo_root)
        evidence_root = resolve_with_root(args.evidence_root, repo_root)
        report = build_report(
            memory_jsonl,
            evidence_root,
            repo_root,
            spec_filter,
            allowlist,
        )
    except (FileNotFoundError, ValueError) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 2

    print_human_report(report, memory_jsonl, evidence_root, spec_filter, repo_root)
    emit_json_report(report, args.pretty, args.json_out)
    return 1 if report["drift_detected"] else 0


if __name__ == "__main__":
    sys.exit(main())
