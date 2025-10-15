from __future__ import annotations

import argparse
import json
import unittest
from pathlib import Path

from scripts.spec_ops_004 import telemetry_utils


def build_namespace(**kwargs):
    return argparse.Namespace(**kwargs)


class TelemetryUtilsTest(unittest.TestCase):
    def test_extract_agent_metrics(self):
        with tempfile_dir() as tmp_path:
            events = tmp_path / "events.jsonl"
            events.write_text(
                """
{"type": "response.output_text.delta", "delta": {"type": "output_text", "text": "foo"}}
{"type": "response.completed", "response": {"usage": {"input_tokens": 120, "output_tokens": 80, "total_tokens": 200, "prompt_tokens_details": {"cached_tokens": 0}}}}
                """.strip()
                + "\n",
                encoding="utf-8",
            )

            output_json = tmp_path / "output.json"
            output_json.write_text(json.dumps({"agent": "gpt_pro"}), encoding="utf-8")

            metrics_path = tmp_path / "metrics.json"

            args = build_namespace(
                events=str(events),
                output_path=str(output_json),
                out=str(metrics_path),
                agent="gpt_pro",
                model_id="gpt-5",
                model_release="2025-09-29",
                reasoning_mode="high",
                latency_ms=1234,
                exec_status=0,
            )

            telemetry_utils.cmd_extract_agent(args)

            metrics = json.loads(metrics_path.read_text(encoding="utf-8"))
            self.assertEqual(metrics["agent"], "gpt_pro")
            self.assertEqual(metrics["prompt_tokens"], 120)
            self.assertEqual(metrics["completion_tokens"], 80)
            self.assertEqual(metrics["total_tokens"], 200)
            self.assertEqual(metrics["status"], "ok")

    def test_write_telemetry(self):
        with tempfile_dir() as tmp_path:
            metrics_file = tmp_path / "metrics.json"
            metrics_file.write_text(
                json.dumps(
                    {
                        "agent": "gpt_pro",
                        "model_id": "gpt-5",
                        "reasoning_mode": "high",
                        "output_path": "/tmp/output.json",
                        "latency_ms": 1000,
                        "prompt_tokens": 120,
                        "completion_tokens": 60,
                        "total_tokens": 180,
                        "cost_usd": 0.0,
                        "status": "ok",
                    }
                ),
                encoding="utf-8",
            )

            synthesis_file = tmp_path / "synthesis.json"
            synthesis_file.write_text(
                json.dumps(
                    {
                        "status": "ok",
                        "consensus": {"agreements": ["step"], "conflicts": []},
                    }
                ),
                encoding="utf-8",
            )

            telemetry_file = tmp_path / "telemetry.jsonl"

            args = build_namespace(
                telemetry_file=str(telemetry_file),
                stage="spec-plan",
                command="spec-plan",
                spec="SPEC-KIT-DEMO",
                timestamp="2025-10-05T01:21:02Z",
                session_id=None,
                prompt_version="20251002-plan-a",
                synthesis=str(synthesis_file),
                metrics_file=[str(metrics_file)],
                note=None,
            )

            telemetry_utils.cmd_write_telemetry(args)

            lines = telemetry_file.read_text(encoding="utf-8").strip().splitlines()
            self.assertEqual(len(lines), 1)
            record = json.loads(lines[0])
            self.assertEqual(record["schemaVersion"], "2.0")
            self.assertEqual(record["consensus"]["status"], "ok")
            self.assertEqual(record["consensus"]["total_tokens"], 180.0)
            self.assertEqual(record["consensus"]["total_latency_ms"], 1000.0)


class tempfile_dir:
    def __enter__(self):
        import tempfile

        self._tmp = tempfile.TemporaryDirectory()
        return Path(self._tmp.name)

    def __exit__(self, exc_type, exc, tb):
        self._tmp.cleanup()


if __name__ == "__main__":
    unittest.main()
