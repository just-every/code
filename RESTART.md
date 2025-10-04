# Restart Plan: Spec Kit Telemetry & Consensus

## Status Snapshot (2025-10-04)
- **T23 – Telemetry Hook**: ✅ Complete. ChatWidget now writes per-agent JSON, append-only `*_telemetry.jsonl`, and `*_synthesis.json` when `SPEC_KIT_TELEMETRY_ENABLED=1`. Latest artefacts live under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/`.
- **T24 – Consensus Halt Gating**: ⚙️ In progress. ChatWidget loads the synthesis artefact, annotates history with its status, and clears `/spec-auto` when consensus is degraded or conflicting. Needs interactive validation from the TUI.
- **T25 – Tests & E2E**: ⏳ Blocked pending a successful multi-agent run. Unit tests that hit `run_spec_consensus_*` still lack a Tokio runtime wrapper.
- Current branch: `feat/spec-auto-telemetry` (dirty worktree with telemetry/gating code and outstanding doc edits).

## What We Just Verified
- `SPEC_KIT_TELEMETRY_ENABLED=1 SPEC_OPS_ALLOW_DIRTY=1 ./scripts/spec_ops_004/spec_auto.sh SPEC-KIT-DEMO` runs plan → tasks, producing fresh guardrail telemetry JSON/logs. The script times out waiting for implement (expected; requires manual diff acceptance).
- No new consensus synthesis files were produced because the TUI hook is the mechanism that writes them. Remaining validation must happen inside the Codex TUI.

## Immediate Next Steps
1. **Drive `/spec-plan --consensus` via TUI**
   ```bash
   cd codex-rs
   SPEC_KIT_TELEMETRY_ENABLED=1 cargo run -p codex-tui --bin code
   ```
   Then run `/spec-plan SPEC-KIT-DEMO --consensus "Telemetry MVP verification"` and inspect the resulting history cell plus artefacts in `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/`.
2. **Force a degraded/conflict scenario**
   - Delete or edit one of the agent JSON files before rerunning `/spec-plan --consensus`.
   - Confirm `/spec-auto` halts and surfaces the synthesis path.
3. **Wrap consensus unit tests with Tokio**
   - Add `#[tokio::test(flavor = "current_thread")]` to `run_spec_consensus_*` tests so they exercise the async paths without panicking.
4. **Update docs once behaviour is confirmed**
   - Refresh `docs/spec-kit/model-strategy.md`, `docs/slash-commands.md`, and `AGENTS.md` to mention telemetry and halt requirements.

## Validation Checklist
- `/spec-auto SPEC-KIT-DEMO --from plan` (after TUI run) should display the new consensus notice and halt on degraded/conflict.
- `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/` must contain:
  - `spec-plan_<timestamp>_<agent>.json`
  - `spec-plan_<timestamp>_telemetry.jsonl`
  - `spec-plan_<timestamp>_synthesis.json`
- `local-memory search --tags consensus --limit 5` should show the latest consensus verdict JSON with `synthesisPath` populated.

## Useful Commands
- List consensus artefacts: `find docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO -maxdepth 1 -type f | sort`
- Check for degraded/conflict status quickly:
  ```bash
  jq '.status, .missing_agents, .consensus.conflicts' \
     docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/spec-plan_*_synthesis.json
  ```
- Tail telemetry: `tail -n 20 docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/spec-plan_*_telemetry.jsonl`

## Next Session Prompt
```
/set-env SPEC_KIT_TELEMETRY_ENABLED=1
/spec-plan SPEC-KIT-DEMO --consensus "Validate halt gating"
/spec-auto SPEC-KIT-DEMO --from plan
```
(Stop immediately if `/spec-auto` does *not* halt on conflict; capture the history cell and the offending synthesis JSON.)

## Troubleshooting Notes
- **No synthesis file**: Ensure you are running through the Codex TUI, not the shell scripts; the telemetry hook lives in ChatWidget.
- **Tokenizer warnings**: `cargo check -p codex-tui` still emits the pre-existing `GuardrailWait` unused field warning—safe to ignore during validation.
- **Tests crashing**: Wrap `run_spec_consensus_*` tests in a Tokio runtime before rerunning.
- **Dirty tree**: Guardrails normally insist on a clean worktree. Export `SPEC_OPS_ALLOW_DIRTY=1` during local validation to suppress the check (already done for the last run).
