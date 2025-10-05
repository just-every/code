# Telemetry Schema v2

## Purpose
- Capture per-agent metrics for every consensus run
- Emit cost and latency data so we can evaluate model trade-offs
- Remain backward-compatible with schema v1 until all tooling migrates

## Envelope
- `schemaVersion` (string) → set to `"2.0"`
- `command` (string) → guardrail command identifier
- `specId` (string)
- `sessionId` (string)
- `timestamp` (ISO8601 UTC)
- `artifacts` (array of file paths) → optional for validate/audit stages
- `notes` (array of strings) → optional human context

## Consensus Block
- `agents` (array)
  - `agent` (string) → logical name (`gemini`, `claude`, `gpt_pro`, `gpt_codex`)
  - `model_id` (string)
  - `reasoning_mode` (string)
  - `prompt_tokens` (int)
  - `completion_tokens` (int)
  - `latency_ms` (int)
  - `cost_usd` (float)
  - `cache_hit` (bool, optional)
  - `arbiter_override` (bool, optional)
  - `override_reason` (string, optional)
- `disagreement_detected` (bool)
- `disagreement_points` (array of strings)
- `escalation_triggered` (bool)
- `escalation_reason` (string or null)
- `synthesis_status` (string → `ok|conflict|degraded`)
- `total_tokens` (int)
- `total_latency_ms` (int)

## Stage Metrics
- `quality_metrics`
  - `automated_checks_passed` (int)
  - `automated_checks_failed` (int)
  - `human_review_score` (float, optional)
  - `completeness_score` (float, optional)
- `guardrail`
  - `prefilter_model` (string)
  - `prefilter_status` (string)
  - `policy_model` (string)
  - `policy_status` (string)
  - `latency_ms` (int)
  - `cost_usd` (float)

## Stage-Specific Fields
- Plan → retain baseline fields from schema v1
- Tasks → retain `tool.status`
- Implement → retain `lock_status` and `hook_status`
- Validate/Audit → retain `scenarios` array
- Unlock → retain `unlock_status`

## Compatibility Notes
- Keep schema v1 validators running until Phase 1 wraps
- When writing telemetry, always include schema v2 block plus legacy fields
- Consumers should branch on `schemaVersion`

## Open Questions
- Should we promote `notes` into structured enums later?
- Do we need per-tool latency for HAL once MCP usage stabilises?
