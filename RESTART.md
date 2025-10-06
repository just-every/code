# Restart Plan: Spec Kit Telemetry & Consensus

## Status Snapshot (2025-10-05)
- **T21 – Multi-agent consensus automation**: ✅ Done. Consensus runner now emits per-agent metrics (tokens/latency/optional cost) and schema‑v2 telemetry JSONL. Unit tests live in `scripts/spec_ops_004/tests/test_telemetry_utils.py`.
- **T22 – Foundation documents baseline**: ✅ Done. `product-requirements.md`, `PLANNING.md`, and `docs/SPEC-KIT-DEMO/{spec.md,plan.md,tasks.md}` are published and linked from `SPEC.md`.
- **T24 – Consensus halt gating**: ⚙️ Still in flight. ChatWidget surfaces synthesis status and should halt `/spec-auto`, but we need a live TUI run (with real agents) to confirm behaviour and capture evidence.
- **T25 – Consensus integration tests & E2E**: ⏳ Not started. Awaiting the halt validation so we know what to assert.
- **T26 – SPEC-KIT-DEMO guardrail baseline**: ⚙️ In progress. Latest consensus bundle is `spec-plan_2025-10-05T04:31:14Z_*`; we still owe the conflict-halt screenshot and HAL follow-up notes.
- Branch: `feat/spec-auto-telemetry` (clean after the last push).

## What We Just Verified
- `scripts/spec_ops_004/consensus_runner.sh` (with the stub Codex binary) writes full telemetry bundles under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/`, proving the shell path works.
- `python3 -m unittest scripts.spec_ops_004.tests.test_telemetry_utils` passes, validating token/cost extraction and telemetry assembly.

## Immediate Next Steps
1. **Run real `/spec-plan --consensus` via TUI**
   ```bash
   SPEC_KIT_TELEMETRY_ENABLED=1 cargo run -p codex-tui --bin code
   ```
   Command inside TUI: `/spec-plan SPEC-KIT-DEMO --consensus "halt gating validation"`
   Capture the history cell + verify artefacts (JSON, synthesis, telemetry) timestamped with the live run.
2. **Trigger a conflict/degraded verdict**
   - Use the adversarial prompt in local-memory or temporarily modify one agent’s output.
   - Re-run `/spec-auto SPEC-KIT-DEMO --from plan` and confirm it halts, saving a screenshot for T26.
3. **Document HAL fallback**
   - Either run the HAL MCP checks or explicitly record why they’re skipped (task 3 in `docs/SPEC-KIT-DEMO/tasks.md`).
4. **Plan follow-up tests (T25)**
   - Once halt behaviour is confirmed, scope the integration tests (happy/conflict/missing agent) so we can queue them up next session.

## Validation Checklist
- `/spec-plan SPEC-KIT-DEMO --consensus ...` from the TUI produces fresh artefacts dated ≥2025-10-05 with real model metadata.
- `/spec-auto SPEC-KIT-DEMO --from plan` halts on conflict/degraded and the UI highlights the synthesis/telemetry paths.
- `docs/SPEC-KIT-DEMO/tasks.md` reflects the new evidence and notes (Task 2 moves to Done once screenshot attached; Task 3 tracks HAL notes).
- `python3 scripts/spec-kit/lint_tasks.py` stays green after doc updates.

## Useful Commands
- List consensus artefacts: `find docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO -maxdepth 1 -type f | sort`
- Inspect metrics quickly:
  ```bash
  jq '.consensus.status, .consensus.total_tokens, .consensus.total_cost_usd' \
     docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/spec-plan_2025-10-05T04:31:14Z_telemetry.jsonl
  ```
- Verify agent usage: `jq '.prompt_tokens, .completion_tokens' specs-ops..._metrics.json`

## Next Session Prompt
```
/set-env SPEC_KIT_TELEMETRY_ENABLED=1
/spec-plan SPEC-KIT-DEMO --consensus "halt gating validation"
/spec-auto SPEC-KIT-DEMO --from plan
```
Bring back the consensus history cell and the resulting telemetry JSONL path; stop if `/spec-auto` does **not** halt.

## Troubleshooting Notes
- **No synthesis/telemetry output**: Ensure you’re in the Codex TUI; the shell runner only emits telemetry when `CODEX_BIN` returns JSON events.
- **Agent stubs lingering**: Delete any `_metrics.json` files before re-running with real models to avoid mixing stub data with production evidence.
- **Guardrail dirty tree**: Use `SPEC_OPS_ALLOW_DIRTY=1` if you need to iterate locally, but capture clean evidence for commits.
