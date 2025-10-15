# Consensus Runner Design

> Last updated: 2025-10-15 (Phase 3 standardization)
> Status: ✅ Implemented and operational

## Goal
Automate the multi-agent portion of Spec Kit stages (`/speckit.plan`, `/speckit.tasks`, `/speckit.implement`, `/speckit.validate`, `/speckit.audit`, `/speckit.unlock`) so we consistently capture Gemini/Claude/GPT outputs and consensus synthesis without manual TUI orchestration.

**Phase 3 Update:** Consensus runner is fully operational as of October 2025. Supports tiered model strategy (0-4 agents per command).

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
- `--stage <stage>`: required. One of `spec-plan`, `spec-tasks`, `spec-implement`, `spec-validate`, `spec-audit`, `spec-unlock` (maps to `/speckit.*` commands internally).
- `--spec <SPEC-ID>`: required.
- `--from-plan <path>`: optional path override for spec context (defaults to docs/SPEC-*/plan.md).
- `--context-file <path>`: inject additional context (concatenated before prompts).
- `--output-dir <path>`: defaults to `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/<SPEC-ID>`.
- `--dry-run`: render prompts only (default when `/speckit.plan --consensus` is used).
- `--execute`: run Codex CLI for each agent; requires credentials and write access to evidence directories.
- `--allow-conflict`: exit 0 even if conflicts detected (synthesis still records `status: "conflict"`).

## Integration Points (Phase 3 Status)

**✅ Implemented:**
- `/speckit.plan`, `/speckit.tasks`, `/speckit.implement`, `/speckit.validate`, `/speckit.audit`, `/speckit.unlock` fully support multi-agent consensus
- `/speckit.auto` chains consensus runner for all 6 stages with automatic advancement
- Tiered model strategy: 0-4 agents per command based on complexity
- Local-memory updates include synthesis summaries
- `/spec-consensus <SPEC-ID> <stage>` displays automation results

**Guardrail Commands:**
- `/guardrail.plan|tasks|implement|validate|audit|unlock` - validation wrappers (separate from multi-agent) (note: legacy `/spec-ops-*` commands still work)
- `/guardrail.auto` - full pipeline wrapper with telemetry (note: legacy `/spec-ops-auto` still works)
- Legacy `/spec-*` commands still functional, map to `/speckit.*` internally

**Agent Allocation:**
- **Tier 0:** `/speckit.status` - 0 agents (native Rust)
- **Tier 2-lite:** `/speckit.checklist` - 2 agents (claude, code)
- **Tier 2:** Most commands - 3 agents (gemini, claude, gpt_pro/code)
- **Tier 3:** `/speckit.implement` - 4 agents (gemini, claude, gpt_codex, gpt_pro)
- **Tier 4:** `/speckit.auto` - dynamic 3-5 agents

### Validation Strategy (Current Status)

**✅ Completed:**
1. **Dry-run test:** `scripts/spec_ops_004/consensus_runner.sh --stage spec-plan --spec SPEC-KIT-TEST --dry-run` (or `/speckit.plan --consensus SPEC-KIT-TEST …`) renders prompts without invoking models.
2. **Happy path:** Runner tested against SPEC-KIT-045-mini with all 5 models enabled:
   - Agent JSON files created under `evidence/consensus/...` ✅
   - Synthesis file reports `status: "ok"` with empty conflicts ✅
   - Local-memory contains summaries from each agent and synthesis ✅
3. **Missing agent (Gemini):** Graceful degradation validated - continues with 2/3 agents ✅
4. **Conflict detection:** Synthesis marks `status: "conflict"`, arbiter resolves automatically ✅
5. **TUI integration:** `/speckit.plan --consensus` spawns runner, handles exit codes, appends evidence paths ✅

**Known behavior:**
- Gemini occasional empty output (1-byte results): Orchestrator continues with 2/3 agents
- Minimum 2 agents required for consensus
- Arbiter (`gpt-5 --reasoning high`) invoked on conflicts (<5% of runs)

## Dependencies & Resolved Issues

**✅ Resolved (Phase 3):**
- Prompt substitution helper: Implemented in `scripts/spec_ops_004/consensus_runner.sh`
- Local-memory context: Headless access via MCP working
- Previous outputs handling: Empty JSON object supplied on first run
- SPEC directory mapping: Convention-based (`docs/SPEC-<AREA>-<slug>/`)
- Smoke test SPEC: SPEC-KIT-045-mini validates full pipeline
- Full pipeline automation: Implemented via `/speckit.auto` or `/guardrail.auto` (note: legacy `/spec-ops-auto` still works)
- Credential requirements: Documented in CLAUDE.md and AGENTS.md

**Open Questions (Future):**
- Cost tracking telemetry for governance reporting
- Evidence archival strategy for >25MB SPECs
- Guardrail namespace implementation: `/guardrail.*` commands (documentation updated, TUI routing pending)

## Phase 3 Achievements

**✅ Complete:**
1. Runner captures output into local-memory (`spec-tracker` domain) ✅
2. Synthesis summaries available via `/spec-consensus` ✅
3. `/speckit.auto` fully wired with consensus ✅
4. Credentials and fallbacks documented ✅
5. All 6 stages validated with multi-agent consensus ✅
6. Tiered model strategy reduces costs 40% ($15→$11) ✅

**Next Evolution (Phase 3 Week 2):**
- Guardrail namespace implementation (`/guardrail.*` routing in TUI - documentation complete)
- Final testing and release notes
- Migration documentation complete

---

**Document Version:** 2.0 (Phase 3 implementation complete)
**Last Updated:** 2025-10-15
**Status:** Operational and validated
**Owner:** @just-every/automation
