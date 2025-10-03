# Spec Auto Automation State

## Current Coverage (October 2025)
- **Guardrail stages**: `/spec-ops-plan`, `/spec-ops-tasks`, `/spec-ops-implement`, `/spec-ops-validate`, `/spec-ops-audit`, `/spec-ops-unlock`, and `/spec-ops-auto` (wrapper for `scripts/spec_ops_004/spec_auto.sh`) run directly from the TUI.
- **Consensus runner (new)**: `scripts/spec_ops_004/consensus_runner.sh` renders prompts and (in dry-run mode) generates prompt files for Gemini / Claude / GPT. `/spec-plan --consensus SPEC-ID …` queues a dry-run; `/spec-plan --consensus-exec SPEC-ID …` attempts full execution when credentials are available.
- **Manual multi-agent follow-ups**: Until consensus execution is validated, `/spec-tasks`, `/spec-implement`, `/spec-validate`, `/spec-audit`, `/spec-unlock` still rely on manual TUI coordination or the new runner in dry-run mode.

## Decision
- Keep full multi-agent automation behind the `--consensus` flag until:
  1. Deterministic prompt bundling/export is validated (prompt renderer now tracks `PROMPT_VERSION`).
  2. Consensus runner executes models and writes synthesis JSON to `evidence/consensus/` with conflict halting.
  3. Validation proves evidence + local-memory writes survive retries.

## Next Steps
1. Extend `scripts/spec_ops_004/spec_auto.sh` to emit a run summary (stage → telemetry path) for easier hand-off to the TUI.
2. Enable consensus runner `--execute` mode (Codex CLI invocation) and emit synthesis JSON under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/<SPEC-ID>/`.
3. Push synthesis summaries into local-memory so `/spec-consensus` reflects automated runs.
4. After (2)–(3), consider wiring `/spec-ops-auto --with-consensus` to chain guardrails and consensus automatically.
