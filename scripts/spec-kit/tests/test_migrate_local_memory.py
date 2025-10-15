import json
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


FIXTURE_DIR = Path(__file__).resolve().parent / "fixtures"
SCRIPT_PATH = Path(__file__).resolve().parents[1] / "migrate_local_memory.py"


class MigrateLocalMemoryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture = FIXTURE_DIR / "byterover_export_sample.json"
        if not self.fixture.exists():  # pragma: no cover - guard for missing fixtures
            self.fail(f"fixture not found: {self.fixture}")

    def run_script(self, *args: str) -> subprocess.CompletedProcess[str]:
        cmd = [sys.executable, str(SCRIPT_PATH), *args]
        return subprocess.run(
            cmd,
            check=False,
            capture_output=True,
            text=True,
        )

    def create_database(self) -> Path:
        temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(temp_dir.cleanup)
        db_path = Path(temp_dir.name) / "memories.db"

        conn = sqlite3.connect(db_path)
        try:
            conn.execute(
                """
                CREATE TABLE memories (
                    id TEXT PRIMARY KEY,
                    content TEXT NOT NULL,
                    source TEXT,
                    importance INTEGER,
                    tags TEXT,
                    session_id TEXT,
                    domain TEXT,
                    created_at TEXT,
                    updated_at TEXT,
                    agent_type TEXT,
                    agent_context TEXT,
                    access_scope TEXT,
                    slug TEXT UNIQUE
                )
                """
            )

            conn.execute(
                "INSERT INTO memories (id, content, source, importance, tags, domain, created_at, updated_at, slug) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    "existing-plan",
                    "Existing plan entry",
                    "byterover",
                    7,
                    json.dumps(["spec:SPEC-OPS-123", "stage:spec-plan"]),
                    "spec-tracker",
                    "2025-09-27T08:00:00Z",
                    "2025-09-27T08:15:00Z",
                    "spec-ops-123-plan-20250927",
                ),
            )

            conn.execute(
                "INSERT INTO memories (id, content, source, importance, tags, domain, created_at, updated_at, slug) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    "mem-existing-002",
                    "Existing tasks entry",
                    "byterover",
                    7,
                    json.dumps(["spec:SPEC-OPS-123", "stage:spec-tasks"]),
                    "spec-tracker",
                    "2025-09-27T10:00:00Z",
                    "2025-09-27T10:05:00Z",
                    "spec-ops-123-tasks",
                ),
            )

            conn.commit()
        finally:
            conn.close()

        return db_path

    def test_dry_run_generates_summary_report(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            out_path = Path(temp_dir) / "dry_run_report.json"
            result = self.run_script(
                "--source",
                str(self.fixture),
                "--dry-run",
                "--out-json",
                str(out_path),
            )

            if result.returncode != 0:
                self.fail(f"dry-run failed: {result.stderr}\nstdout: {result.stdout}")

            payload = json.loads(out_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["mode"], "dry-run")
            self.assertEqual(payload["summary"]["total"], 3)
            self.assertEqual(
                payload["summary"]["domains"].get("spec-tracker"),
                2,
            )
            self.assertEqual(
                payload["summary"]["domains"].get("docs-ops"),
                1,
            )
            self.assertEqual(payload["results"], [])

    def test_apply_inserts_and_skips_expected_entries(self) -> None:
        db_path = self.create_database()
        with tempfile.TemporaryDirectory() as temp_dir:
            out_path = Path(temp_dir) / "apply_report.json"
            result = self.run_script(
                "--source",
                str(self.fixture),
                "--apply",
                "--database",
                str(db_path),
                "--out-json",
                str(out_path),
            )

            if result.returncode != 0:
                self.fail(f"apply failed: {result.stderr}\nstdout: {result.stdout}")

            payload = json.loads(out_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["mode"], "apply")
            summary = payload["summary"]
            self.assertEqual(summary.get("inserted"), 1)
            self.assertEqual(summary.get("skipped"), 2)
            self.assertEqual(summary.get("database"), str(db_path))

            statuses = {(item["id"], item["status"]) for item in payload["results"]}
            self.assertIn(("mem-plan-001", "skipped_existing_slug"), statuses)
            self.assertIn(("mem-existing-002", "skipped_existing_id"), statuses)
            self.assertIn(("mem-new-003", "inserted"), statuses)

            conn = sqlite3.connect(db_path)
            conn.row_factory = sqlite3.Row
            try:
                row = conn.execute(
                    "SELECT id, domain, tags, content FROM memories WHERE id = ?",
                    ("mem-new-003",),
                ).fetchone()
            finally:
                conn.close()

            self.assertIsNotNone(row, "expected mem-new-003 to be inserted")
            self.assertEqual(row["domain"], "docs-ops")
            tags = json.loads(row["tags"])
            self.assertIn("agent:codex", tags)
            self.assertIn("spec:SPEC-OPS-123", tags)


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
