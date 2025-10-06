# Spec Tracker

## Current State (2025-10-05)

**Vision:** Vague idea → auto-generate SPEC → automatic implementation with validation
**Reality:** Manual SPEC setup → guardrails work → consensus exists but not auto-integrated → manual approval gates

**Critical Blockers:**
1. **T28** - spec_auto.sh doesn't call consensus_runner.sh (automation incomplete)
2. **T29** - No unified intake flow (manual 4-step SPEC creation)
3. **T30** - Hardcoded Rust commands (357 lines, rebase friction)

---

## Architecture Components

### ✅ Fully Working
- Guardrail scripts (plan/tasks/implement/validate/audit/unlock)
- Multi-agent prompts (prompts.json with Gemini/Claude/GPT)
- Consensus runner (can execute with --execute flag)
- Telemetry schema v1 (JSON capture, validation)
- Evidence capture (per-agent + synthesis + telemetry.jsonl)

### ⚠️ Partially Working
- `/spec-auto` - TUI state machine exists, inserts prompts but requires manual send
- `/spec-ops-auto` - Bash orchestrator runs guardrails only, no consensus integration
- Consensus synthesis - Written but not checked/halted automatically

### ❌ Missing
- Unified intake command (/new-spec)
- Auto-execution without approval gates
- Project Commands migration (still hardcoded in Rust)
- Evidence archival strategy

---

## Tasks

| Order | ID | Title | Status | Notes |
|-------|-----|-------|--------|-------|
| 1 | T28 | Integrate consensus into spec_auto.sh | **DONE** | Modified spec_auto.sh to call consensus_runner.sh --execute after each guardrail stage; added check_synthesis.py validator; halts on conflict/degraded. Default: consensus enabled. Use --skip-consensus for guardrails-only. Completed: 2025-10-05. |
| 2 | T29 | Unified intake (/new-spec command) | **DONE** | Created /new-spec subagent command + generate_spec_id.py helper. Takes feature description → generates SPEC-ID → creates directory → runs /specify + /plan + /tasks → presents package. Eliminates 4 manual steps. Usage: /new-spec <description>. Completed: 2025-10-05. |
| 3 | T30 | Migrate to Project Commands | **MEDIUM PRIORITY** | Convert SpecOpsPlan, SpecOpsAuto, etc. from Rust enum to config.toml [[projects.commands]]. Remove 357 lines from slash_command.rs. **Reduces rebase friction.** Effort: 4-6 hours. |
| 4 | T31 | Evidence compression & archival | **MEDIUM PRIORITY** | Implement: gzip *.json older than 7 days, rotation (keep last N runs), optional S3 upload. **Addresses 25MB limit.** Effort: 1 day. |
| 5 | T32 | Orchestrator /spec-auto implementation | **DONE** | Created /spec-auto subagent orchestrator. Runs guardrails visibly, spawns agents natively, synthesizes consensus, auto-advances. Full visibility without TUI code changes. Completed: 2025-10-06. |
| 6 | T25 | Consensus integration tests | **BACKLOG** | Add TUI integration tests for consensus (happy/conflict/missing) + E2E validation. Effort: 1 day. |
| 7 | T33 | Task format unification | **BACKLOG** | Single source (tasks.md), generate plan.md + SPEC.md views. Eliminates manual sync. Effort: 2-3 days. |
| 8 | T34 | Conflict arbiter agent | **BACKLOG** | Auto-resolution: spawn gpt-5 arbiter when synthesis shows tie. Reduces manual intervention by ~60%. Effort: 2-3 days. |
| 9 | T35 | SPEC-KIT-020 automated conflict resolution intake | **BACKLOG** | SPEC-KIT-020-add-automated-conflict-resolution-with created via /new-spec on 2025-10-05. |
| 10 | T36 | SPEC-KIT-025 automated conflict resolution arbiter agent | **BACKLOG** | SPEC-KIT-025-add-automated-conflict-resolution-with generated via /new-spec on 2025-10-05; tasks tracked in docs/SPEC-KIT-025-add-automated-conflict-resolution-with/tasks.md. |

---

## Completed Foundation (Archive)

| ID | Title | Completion | Evidence |
|----|-------|------------|----------|
| T1-T2 | Guardrail command naming | 2025-09-26 | Renamed to /spec-ops-* pattern |
| T3-T8 | Multi-agent prompts | 2025-09 | prompts.json implemented (plan/tasks/implement/validate/audit/unlock) |
| T9 | MCP servers | 2025-09-26 | repo_search, doc_index, git_status, hal configured |
| T10 | Local-memory migration | 2025-09-28 | Byterover → local-memory complete |
| T11 | /spec-auto TUI infrastructure | 2025-09-26 | State machine exists, prompts insert (NOT auto-submit) |
| T12 | Consensus diff reviewer | 2025-09-27 | Synthesis.json structure, conflict detection |
| T13 | Telemetry schema v1 | 2025-09-27 | Validation + unit tests |
| T14 | Documentation refresh | 2025-09-29 | CLAUDE.md, slash-commands.md, AGENTS.md synced |
| T15 | Nightly sync check | 2025-09-27 | Drift detector script |
| T18 | HAL HTTP MCP | 2025-09-29 | Health, list_movies, indexer_test, graphql_ping |
| T20 | Guardrail hardening | 2025-09-29 | Baseline + HAL enforcement |
| T21 | Consensus runner | 2025-10-05 | consensus_runner.sh works with --execute (NOT integrated into spec_auto.sh) |
| T22 | Foundation docs | 2025-10-05 | product-requirements.md, PLANNING.md created |
| T23 | Telemetry hook (TUI) | 2025-10-04 | ChatWidget persists consensus artifacts when SPEC_KIT_TELEMETRY_ENABLED=1 |
| T24 | Consensus ingestion (TUI) | 2025-10-04 | ChatWidget reads synthesis, pauses on conflict (NOT auto-continues) |
| T26 | SPEC-KIT-DEMO baseline | 2025-10-05 | Consensus bundle captured |
| T27 | Hooks architecture investigation | 2025-10-05 | Investigated Project Hooks; tool.before/after don't fire (background exec bypass). Decision: Keep bash telemetry. Hooks provide 40% coverage, need +100 line core fix. Migration not justified. |

---

## Implementation Priority

### Week 1-2: Enable Full Automation
**T28** - Bash consensus integration
**Goal:** `/spec-ops-auto SPEC-ID` runs plan→unlock without human input

### Week 3: Unified Intake
**T29** - /new-spec command
**Goal:** Single command from feature description → ready-to-implement SPEC

### Week 4: Cleanup
**T30** - Project Commands migration
**T31** - Evidence management

---

## Notes

**Architecture decision (T27):** Keep bash telemetry architecture. Project Hooks migration not viable (tool hooks don't fire, provide only 40% of schema fields, require core modifications).

**Automation status:** Guardrails work, consensus runner works, but NOT integrated. T28 is the critical path to full automation.

**Intake flow:** Currently requires 4 manual steps (edit SPEC.md, /specify, /plan, /tasks). T29 unifies into single command.
