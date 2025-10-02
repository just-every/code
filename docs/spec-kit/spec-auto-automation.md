# Spec Auto Automation State

## Current Coverage (October 2025)
- **Guardrail stages**: `/spec-ops-plan`, `/spec-ops-tasks`, `/spec-ops-implement`, `/spec-ops-validate`, `/spec-ops-audit`, `/spec-ops-unlock`, and the new `/spec-ops-auto` slash command (wrapper for `scripts/spec_ops_004/spec_auto.sh`) are all invokable directly from the TUI.
- **Multi-agent follow-ups**: `/spec-plan`, `/spec-tasks`, `/spec-implement`, `/spec-validate`, `/spec-audit`, `/spec-unlock` remain manual because we still rely on the TUI to coordinate Gemini / Claude / GPT outputs and consensus review.

## Decision
- Keep multi-agent execution manual until we have:
  1. Deterministic prompt bundling/export for each agent stage (now tracked via `PROMPT_VERSION`).
  2. A way to capture consensus verdicts programmatically without bypassing manual review safeguards.
  3. Validation coverage demonstrating that evidence + local-memory writes are resilient to automation retries.

## Next Steps
1. Extend `scripts/spec_ops_004/spec_auto.sh` to emit a run summary (stage → telemetry path) for easier hand-off to the TUI.
2. Prototype a "headless" `/spec-plan` invocation that uses the same prompts but writes outputs directly to evidence + local-memory.
3. Only after (1) and (2) succeed should we consider a non-interactive consensus runner.
