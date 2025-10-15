#!/usr/bin/env python3

"""Utilities for consensus telemetry generation."""

from __future__ import annotations

import argparse
import json
import math
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional


MODEL_PRICING: Dict[str, Dict[str, float]] = {
    "gpt-5": {"prompt": 0.010, "completion": 0.030},
    "gpt-5-codex": {"prompt": 0.012, "completion": 0.036},
    "claude-4.5-sonnet": {"prompt": 0.003, "completion": 0.015},
    "gemini-2.5-pro": {"prompt": 0.0007, "completion": 0.0021},
}


@dataclass
class UsageMetrics:
    prompt_tokens: Optional[int] = None
    completion_tokens: Optional[int] = None
    reasoning_tokens: Optional[int] = None
    total_tokens: Optional[int] = None
    cache_hit: Optional[bool] = None


@dataclass
class AgentMetrics:
    agent: str
    model_id: str
    model_release: Optional[str]
    reasoning_mode: Optional[str]
    output_path: Path
    latency_ms: Optional[int]
    exec_status: int
    usage: UsageMetrics = field(default_factory=UsageMetrics)
    cost_usd: Optional[float] = None
    error: Optional[str] = None

    def to_json(self) -> Dict[str, Any]:
        return {
            "agent": self.agent,
            "model_id": self.model_id,
            "model_release": self.model_release,
            "reasoning_mode": self.reasoning_mode,
            "output_path": str(self.output_path),
            "latency_ms": self.latency_ms,
            "prompt_tokens": self.usage.prompt_tokens,
            "completion_tokens": self.usage.completion_tokens,
            "reasoning_tokens": self.usage.reasoning_tokens,
            "total_tokens": self.usage.total_tokens,
            "cache_hit": self.usage.cache_hit,
            "cost_usd": None if self.cost_usd is None else round(self.cost_usd, 6),
            "status": "ok" if self.exec_status == 0 and self.error is None else "error",
            "error": self.error,
        }


def _load_json_lines(path: Path) -> Iterable[Dict[str, Any]]:
    with path.open("r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            try:
                yield json.loads(line)
            except json.JSONDecodeError:
                continue


def _coerce_int(value: Any) -> Optional[int]:
    if value is None:
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def _extract_usage(events_path: Path) -> UsageMetrics:
    usage = UsageMetrics()

    for event in _load_json_lines(events_path):
        candidate: Optional[Dict[str, Any]] = None
        if isinstance(event, dict):
            if "usage" in event and isinstance(event["usage"], dict):
                candidate = event["usage"]
            elif event.get("type") == "response.completed":
                response = event.get("response")
                if isinstance(response, dict) and isinstance(response.get("usage"), dict):
                    candidate = response["usage"]

        if not candidate:
            continue

        usage.prompt_tokens = usage.prompt_tokens or _coerce_int(
            candidate.get("input_tokens")
            or candidate.get("prompt_tokens")
            or candidate.get("input_tokens_total")
        )
        usage.completion_tokens = usage.completion_tokens or _coerce_int(
            candidate.get("output_tokens")
            or candidate.get("completion_tokens")
        )
        usage.reasoning_tokens = usage.reasoning_tokens or _coerce_int(
            candidate.get("reasoning_output_tokens")
        )
        usage.total_tokens = usage.total_tokens or _coerce_int(
            candidate.get("total_tokens")
        )

        details = candidate.get("input_tokens_details") or candidate.get("prompt_tokens_details")
        if isinstance(details, dict):
            cached = _coerce_int(details.get("cached_tokens"))
            if cached is not None:
                usage.cache_hit = cached > 0

    return usage


def _cost_from_usage(model_id: str, usage: UsageMetrics) -> Optional[float]:
    pricing = MODEL_PRICING.get(model_id)
    if not pricing:
        return None

    prompt_tokens = usage.prompt_tokens or 0
    completion_tokens = usage.completion_tokens or 0

    if prompt_tokens == 0 and completion_tokens == 0:
        return None

    prompt_cost = (prompt_tokens / 1000.0) * pricing.get("prompt", 0.0)
    completion_cost = (completion_tokens / 1000.0) * pricing.get("completion", 0.0)
    total = prompt_cost + completion_cost
    return float(total)


def cmd_extract_agent(args: argparse.Namespace) -> None:
    events_path = Path(args.events)
    output_path = Path(args.output_path)
    metrics_path = Path(args.out)

    usage = _extract_usage(events_path)
    cost = _cost_from_usage(args.model_id, usage)

    error_message: Optional[str] = None
    if args.exec_status != 0:
        error_message = f"agent exited with status {args.exec_status}"
    else:
        try:
            payload = json.loads(output_path.read_text(encoding="utf-8"))
            if isinstance(payload, dict) and payload.get("error"):
                error_message = str(payload["error"])
        except Exception as exc:  # noqa: BLE001 - best effort
            error_message = f"failed to read output JSON: {exc}"

    metrics = AgentMetrics(
        agent=args.agent,
        model_id=args.model_id,
        model_release=args.model_release,
        reasoning_mode=args.reasoning_mode,
        output_path=output_path,
        latency_ms=args.latency_ms,
        exec_status=args.exec_status,
        usage=usage,
        cost_usd=cost,
        error=error_message,
    )

    metrics_path.write_text(json.dumps(metrics.to_json(), indent=2), encoding="utf-8")


def _sum_optional(items: Iterable[Dict[str, Any]], key: str) -> Optional[float]:
    values: List[float] = []
    for item in items:
        value = item.get(key)
        if value is None:
            continue
        try:
            values.append(float(value))
        except (TypeError, ValueError):
            continue

    if not values:
        return None

    total = math.fsum(values)
    return float(total)


def cmd_write_telemetry(args: argparse.Namespace) -> None:
    synthesis_path = Path(args.synthesis)
    telemetry_path = Path(args.telemetry_file)

    agent_payloads: List[Dict[str, Any]] = []
    for metrics_file in args.metrics_file:
        metrics_data = json.loads(Path(metrics_file).read_text(encoding="utf-8"))
        agent_payloads.append(metrics_data)

    synthesis_payload = json.loads(synthesis_path.read_text(encoding="utf-8"))
    consensus_status = synthesis_payload.get("status", "unknown")
    consensus_block = {
        "status": consensus_status,
        "agreements": synthesis_payload.get("consensus", {}).get("agreements", []),
        "conflicts": synthesis_payload.get("consensus", {}).get("conflicts", []),
        "agents": agent_payloads,
    }

    disagreement_detected = bool(consensus_block["conflicts"])
    for agent in agent_payloads:
        if agent.get("status") != "ok":
            disagreement_detected = True
            break

    total_tokens = _sum_optional(agent_payloads, "total_tokens")
    total_latency_ms = _sum_optional(agent_payloads, "latency_ms")
    total_cost = _sum_optional(agent_payloads, "cost_usd")

    consensus_block.update(
        {
            "disagreement_detected": disagreement_detected,
            "disagreement_points": consensus_block["conflicts"],
            "escalation_triggered": False,
            "escalation_reason": None,
            "total_tokens": total_tokens,
            "total_latency_ms": total_latency_ms,
            "total_cost_usd": total_cost,
        }
    )

    record: Dict[str, Any] = {
        "schemaVersion": "2.0",
        "command": args.command,
        "specId": args.spec,
        "sessionId": args.session_id or args.timestamp,
        "timestamp": args.timestamp,
        "promptVersion": args.prompt_version,
        "artifacts": [str(agent["output_path"]) for agent in agent_payloads]
        + [str(synthesis_path)],
        "notes": args.note or [],
        "consensus": consensus_block,
        "quality_metrics": {
            "automated_checks_passed": 0,
            "automated_checks_failed": 0,
            "human_review_score": None,
            "completeness_score": None,
        },
        "guardrail": {
            "prefilter_model": None,
            "prefilter_status": None,
            "policy_model": None,
            "policy_status": None,
            "latency_ms": None,
            "cost_usd": None,
        },
        "consensusStatus": consensus_status,
        "consensusSummary": synthesis_payload.get("consensus"),
    }

    telemetry_path.parent.mkdir(parents=True, exist_ok=True)
    with telemetry_path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record) + "\n")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Consensus telemetry helper")
    sub = parser.add_subparsers(dest="cmd", required=True)

    extract = sub.add_parser("extract-agent", help="Extract usage metrics for a single agent")
    extract.add_argument("--events", required=True, help="Path to JSONL event log")
    extract.add_argument("--output-path", required=True, help="Path to agent output JSON")
    extract.add_argument("--out", required=True, help="Where to write metrics JSON")
    extract.add_argument("--agent", required=True)
    extract.add_argument("--model-id", required=True)
    extract.add_argument("--model-release")
    extract.add_argument("--reasoning-mode")
    extract.add_argument("--latency-ms", type=int, default=None)
    extract.add_argument("--exec-status", type=int, default=0)
    extract.set_defaults(func=cmd_extract_agent)

    write = sub.add_parser("write-telemetry", help="Assemble telemetry JSONL record")
    write.add_argument("--telemetry-file", required=True)
    write.add_argument("--stage", required=True)
    write.add_argument("--command", required=True)
    write.add_argument("--spec", required=True)
    write.add_argument("--timestamp", required=True)
    write.add_argument("--session-id")
    write.add_argument("--prompt-version", required=True)
    write.add_argument("--synthesis", required=True)
    write.add_argument("--metrics-file", action="append", required=True)
    write.add_argument("--note", action="append")
    write.set_defaults(func=cmd_write_telemetry)

    return parser


def main(argv: Optional[List[str]] = None) -> None:
    parser = build_parser()
    args = parser.parse_args(argv)
    args.func(args)


if __name__ == "__main__":
    main()
