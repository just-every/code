import importlib.util
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "fetch_byterover_bulk.py"
spec = importlib.util.spec_from_file_location("fetch_byterover_bulk", MODULE_PATH)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None  # pragma: no cover - loader guaranteed by spec
spec.loader.exec_module(module)  # type: ignore[attr-defined]

extract_entries = module.extract_entries
summarise = module.summarise


class FetchByteroverBulkTests(unittest.TestCase):
    def test_extract_entries_from_list(self) -> None:
        entries = [{"id": "a"}, {"id": "b"}]
        self.assertEqual(extract_entries(entries), entries)

    def test_extract_entries_from_dict(self) -> None:
        payload = {"entries": [{"id": "x"}]}
        self.assertEqual(extract_entries(payload), payload["entries"])

    def test_summarise_counts_domains_and_tags(self) -> None:
        entries = [
            {"domain": "spec-tracker", "tags": ["spec:SPEC-1", "agent:gemini"]},
            {"target_domain": "docs-ops", "tags": ["spec:SPEC-1"]},
        ]
        summary = summarise(entries)
        self.assertEqual(summary["total_entries"], 2)
        self.assertEqual(summary["domains"].get("spec-tracker"), 1)
        self.assertEqual(summary["domains"].get("docs-ops"), 1)
        self.assertIn("spec:SPEC-1", summary["top_tags"])


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
