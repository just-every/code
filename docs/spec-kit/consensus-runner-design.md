# Consensus Runner Design (Draft)

> Last updated: 2025-10-02

## Goal
Automate the multi-agent portion of Spec Kit stages (`/spec-plan`, `/spec-tasks`, `/spec-implement`, `/spec-validate`, `/spec-audit`, `/spec-unlock`) so we consistently capture Gemini/Claude/GPT outputs and consensus synthesis without manual TUI orchestration.

## Deliverable
Shell entry point `scripts/spec_ops_004/consensus_runner.sh` that:
1. Accepts `--stage <stage>` and `--spec <SPEC-ID>` (plus optional flags described below).
2. Reads prompt definitions from `docs/spec-kit/prompts.json`.
3. Resolves template variables:
   - `${SPEC_ID}` → passed spec id.
   - `${PROMPT_VERSION}` → prompt `version` for the stage.
   - `${MODEL_ID}`, `${MODEL_RELEASE}`, `${REASONING_MODE}` → looked up from `docs/spec-kit/model-strategy.md` (default table) or environment overrides (e.g. `SPEC_KIT_MODEL_GEMINI`).
   - `${CONTEXT}` → combination of `docs/SPEC-<area>-<slug>/spec.md`, latest plan/tasks docs, and local-memory exports (retrieved via MCP shell-lite or local-memory CLI dump when available).
   - `${PREVIOUS_OUTPUTS.*}` → previous agent JSON payloads saved under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/<SPEC-ID>/` for the current stage run (Gemini → Claude, Gemini+Claude → GPT). On first agent we pass empty object.
4. Invokes each agent sequentially using the Codex CLI binary (`codex-rs/target/dev-fast/code` or configured path) with `code exec --sandbox read-only --model <model> --reasoning <mode> -- <prompt>`.
5. Writes each agent response to:
   - `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/<SPEC-ID>/<stage>_<timestamp>_<agent>.json`
   - Local-memory (`spec-tracker` domain) with the same payload for future retrieval.
6. After all agents complete, calls a synthesis helper (bash + jq/python) that merges agreements/conflicts and writes `.../<stage>_<timestamp>_synthesis.json` with:
   ```json
   {
     "stage": "spec-plan",
     "specId": "SPEC-KIT-DEMO",
     "timestamp": "2025-10-02T23:45:00Z",
     "prompt_version": "20251002-plan-a",
     "agents": [ ...list of outputs with paths... ],
     "consensus": { "agreements": [...], "conflicts": [...] },
     "status": "ok|degraded|conflict",
     "notes": [ "Missing Claude output" ]
   }
   ```
7. Returns non-zero exit code if:
   - any agent call fails,
   - required consensus fields missing,
   - conflicts array non-empty (unless `--allow-conflict` supplied).

## Command Flags
- `--stage <stage>`: required. One of `spec-plan`, `spec-tasks`, `spec-implement`, `spec-validate`, `spec-audit`, `spec-unlock`.
- `--spec <SPEC-ID>`: required.
- `--from-plan <path>`: optional path override for spec context (defaults to docs/SPEC-*/plan.md).
- `--context-file <path>`: inject additional context (concatenated before prompts).
- `--output-dir <path>`: defaults to `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/<SPEC-ID>`.
- `--dry-run`: render prompts only (default when `/spec-plan --consensus` is used).
- `--execute`: run Codex CLI for each agent; requires credentials and write access to evidence directories.
- `--allow-conflict`: exit 0 even if conflicts detected (synthesis still records `status: "conflict"`).

## Integration Points
- `/spec-plan` (TUI) will accept `--consensus` flag. When set, handler runs guardrail shell first, then invokes consensus runner. On failure the handler should surface the telemetry/log path and halt stage completion.
- `/spec-tasks`, `/spec-implement`, `/spec-validate`, `/spec-audit`, `/spec-unlock` receive similar `--consensus` support. Default remains manual until the feature is stable.
- `/spec-ops-auto` gains `--with-consensus` to chain consensus runner after each guardrail stage (future work once CLI invocation is reliable).
- Local-memory updates should include the synthesis summary so `/spec-consensus <SPEC-ID> <stage>` can display automation results.

### Validation Strategy
1. **Dry-run test:** `scripts/spec_ops_004/consensus_runner.sh --stage spec-plan --spec SPEC-KIT-TEST --dry-run` (or `/spec-plan --consensus SPEC-KIT-TEST …`) renders prompts without invoking models.
2. **Happy path:** run runner against a test SPEC with all three models enabled; verify:
   - Agent JSON files created under `evidence/consensus/...`.
   - Synthesis file reports `status: "ok"` with empty conflicts.
   - Local-memory contains summaries from each agent and the synthesis.
3. **Missing agent:** simulate failure (set `SPEC_KIT_SKIP_GPT_PRO=1`) and confirm runner exits non-zero with `status: "degraded"`.
4. **Conflict detection:** craft fixtures where Claude and GPT disagree; ensure synthesis marks `status: "conflict"` and exit code non-zero unless `--allow-conflict` supplied.
5. **TUI integration:** add unit/integration tests in `codex-rs/tui` validating that `/spec-plan --consensus` spawns the runner, handles exit codes, and appends history entries referencing evidence paths.

## Dependencies & Open Questions
- Need lightweight helper (probably a Python snippet) to load `prompts.json`, substitute variables, and escape values safely.
- Determine reliable way to pull local-memory context headlessly (current CLI may not support direct export; may require MCP shell call).
- Decide default handling when previous outputs absent (e.g., first run). Plan: supply empty JSON object for placeholders and let prompts handle missing keys.
- Clarify how we map `docs/SPEC-*/` directories from SPEC IDs (currently manual; may need lookup table or naming convention).

## Next Steps
1. Capture runner output into local-memory entries (`spec-tracker` domain) and surface synthesis summaries via `/spec-consensus`.
2. Add smoke test SPEC (`SPEC-KIT-CONSENSUS-TEST`) and automated tests to ensure dry-run/execute flows, conflict handling, and TUI integration behave as expected.
3. Wire `/spec-ops-auto --with-consensus` once execute mode is stable.
4. Document credential requirements and fallback strategies (e.g., OSS/PYQ fallback, HAL secrets) before enabling by default.

Owner: Spec Kit maintainers (feat/spec-auto-telemetry).
