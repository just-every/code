# Spec-Kit Multi-Agent Framework - Architecture & Planning

> Status: v1.1 (2025-10-15). Phase 3 standardization complete. Complements `product-requirements.md` and fulfils the constitution's mandatory context references.

## 1. Monorepo Overview
- **Repository:** just-every/code (feature branch: feat/spec-auto-telemetry, 76+ commits)
- **Primary language:** Rust (Codex CLI fork with spec-kit extensions)
- **Secondary tooling:** Bash guardrail scripts, MCP (Model Context Protocol) servers, Python utilities
- **Upstream:** anthropics/claude-code (fork maintained with FORK_DEVIATIONS.md rebase strategy)
- **Key directories:**
  - `codex-rs/` – Rust workspace containing Codex CLI/TUI, native spec-kit commands, MCP clients
  - `templates/` – GitHub-inspired spec/PRD/plan/tasks templates (validated 55% faster)
  - `scripts/spec_ops_004/` – Shell automation for guardrail stages and support utilities
  - `docs/spec-kit/` – Prompt versions, model strategy, automation runbooks, evidence policies
  - `docs/SPEC-<AREA>-<slug>/` – Individual SPEC directories (PRD, plan, tasks, evidence)
  - `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/` – Telemetry and evidence artefacts organised by SPEC ID
  - `memory/` – Constitution and stored guidance for operating Spec Kit flows

## 2. Component Architecture

### 2.1 Command Layer (/speckit.* namespace)
**13 TUI-native slash commands** organized by tier:

**Tier 0 - Native TUI** (0 agents, instant):
- `/speckit.status` – Pure Rust implementation, reads from evidence directory

**Tier 2-lite - Dual Agent** (2 agents: claude, code):
- `/speckit.checklist` – Requirement quality scoring

**Tier 2 - Triple Agent** (3 agents: gemini, claude, code/gpt_pro):
- `/speckit.new`, `/speckit.specify` – SPEC creation with templates
- `/speckit.clarify`, `/speckit.analyze` – Quality commands
- `/speckit.plan`, `/speckit.tasks`, `/speckit.validate`, `/speckit.audit`, `/speckit.unlock` – Development stages

**Tier 3 - Quad Agent** (4 agents: gemini, claude, gpt_codex, gpt_pro):
- `/speckit.implement` – Code generation with validation

**Tier 4 - Dynamic** (3-5 agents adaptively):
- `/speckit.auto` – Full 6-stage pipeline with automatic conflict resolution

### 2.2 Multi-Agent Orchestration Layer
**Implementation:** `scripts/spec_ops_004/consensus_runner.sh`
- **Agent spawning:** Parallel execution via Codex CLI subagent framework
- **Consensus synthesis:** Automatic comparison, conflict detection, arbiter invocation
- **Status:** ✅ Fully operational (October 2025)
- **Evidence:** All agent outputs captured in `evidence/consensus/<SPEC-ID>/`

**Model allocation** (tiered strategy):
- **Gemini 2.5 Pro:** Research, breadth, exploration
- **Claude 4.5 Sonnet:** Synthesis, precision, analysis
- **GPT-5:** Validation, arbitration, quality checks
- **GPT-5-Codex:** Code generation, implementation
- **Code (Claude Code):** General-purpose, orchestration

### 2.3 Template System
**Location:** `templates/` directory
- `spec-template.md` – P1/P2/P3 user scenario format (GitHub-inspired)
- `PRD-template.md` – Structured requirements
- `plan-template.md` – Work breakdown with acceptance mapping
- `tasks-template.md` – Checkbox task lists

**Performance:** 55% faster generation vs baseline (validated SPEC-KIT-060)
**Adoption:** 100% (all commands use templates)

### 2.4 Guardrail Layer (Shell wrappers)
**Purpose:** Validation and policy enforcement separate from agent orchestration

**Commands:** `/spec-ops-{plan,tasks,implement,validate,audit,unlock,auto}`
- Stage-specific scripts (`spec_ops_004/commands/spec_ops_{stage}.sh`) share helpers via `common.sh`
- Telemetry emitted as JSON (schema v1) per stage with optional HAL payloads
- `/spec-ops-auto` orchestrates sequential execution with clean tree enforcement

**Telemetry schema v1:**
- Common: `command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts[]`
- Stage-specific: `baseline`, `lock_status`, `scenarios[]`, `unlock_status`, `hal.summary`

### 2.5 Codex CLI / TUI Integration
**Slash command routing** (`codex-rs/tui/src/slash_command.rs`):
- 13 SpecKit* enum variants for `/speckit.*` commands
- Legacy variants for backward compatibility (`/spec-*` → SpecPlan, etc.)
- Native implementation for `/speckit.status` (no agents)
- Orchestrator delegation for multi-agent commands

**Agent framework** (`~/.code/config.toml`):
- 5 agent types: gemini, claude, gpt_pro, gpt_codex, code
- Subagent commands defined per-stage with model and reasoning mode
- Write mode enabled for all agents
- Parallel spawning supported

### 2.6 External Services
- **Local-memory MCP:** Canonical knowledge base for Spec Kit (conversation history, decisions, evidence)
- **Byterover MCP:** Fallback knowledge retrieval (migration to local-memory ongoing)
- **HAL HTTP MCP:** Validates API endpoints; requires `HAL_SECRET_KAVEDARR_API_KEY` (used in example Kavedarr project)
- **Git-status MCP:** Repository state monitoring

## 3. Technology Stack & Dependencies

**Core Infrastructure:**
- **Rust Toolchain:** Stable 1.80+ (configured via `rust-toolchain.toml`)
- **Codex CLI:** anthropics/claude-code fork (rebase strategy in FORK_DEVIATIONS.md)
- **Package Management:** Cargo workspaces (`codex-rs/Cargo.toml`)
- **Shell Environment:** Bash 5+, `env_run.sh` ensures `.env` secrets respected

**AI Models (via Codex CLI):**
- **Gemini 2.5 Pro** – Research, breadth (Tier 2/3/4)
- **Claude 4.5 Sonnet** – Synthesis, precision (Tier 2/3/4)
- **GPT-5** – Validation, arbitration (Tier 2/3/4)
- **GPT-5-Codex** – Code generation (Tier 3/4 only)
- **Code (Claude Code)** – General-purpose (all tiers)

**MCP Servers:**
- **local-memory** – Primary knowledge base (conversation history, decisions)
- **byterover-mcp** – Fallback knowledge retrieval
- **git-status** – Repository state monitoring
- **hal** – API endpoint validation (project-specific, optional)

**Utilities:**
- Python 3.8+ for telemetry processing (`check_synthesis.py`)
- Standard Unix tooling for evidence stats (`evidence_stats.sh`)
- Git hooks for pre-commit/pre-push validation (`scripts/setup-hooks.sh`)

## 4. Constraints & Assumptions

**Operational:**
- Guardrail scripts expect clean git status unless `SPEC_OPS_ALLOW_DIRTY=1`
- Cargo workspace root is `codex-rs/` (all Rust commands must run from there)
- Evidence stored in git with 25 MB per-SPEC soft limit (monitored via `/spec-evidence-stats`)
- HAL service optional; set `SPEC_OPS_HAL_SKIP=1` if unavailable

**Multi-Agent Execution:**
- ✅ Full automation operational (October 2025) - `/speckit.auto` orchestrates all 6 stages
- All 5 agents must be configured in `~/.code/config.toml`
- Gemini occasional empty output handled gracefully (orchestrator continues with 2/3 agents)
- Arbiter automatically invoked on conflicts (no human gate required)

**Performance:**
- Template generation 55% faster than baseline (validated SPEC-KIT-060)
- Tiered strategy reduces costs 40% ($15→$11 per pipeline)
- Parallel agent spawning 30% faster than sequential
- Native status queries <1s (no API calls)

**Backward Compatibility:**
- All `/spec-*` legacy commands continue to work
- Deprecation warnings planned for future release
- Migration guide available (see docs/spec-kit/MIGRATION_GUIDE.md)

## 5. Build & Test Plan

**Rust Development:**
```bash
cd codex-rs
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build -p codex-tui --profile dev-fast
```

**Git Hooks (one-time setup):**
```bash
bash scripts/setup-hooks.sh
```

**Guardrail Validation:**
```bash
# Individual stage
/speckit.plan SPEC-KIT-###
/spec-ops-plan SPEC-KIT-###

# Full pipeline
/speckit.auto SPEC-KIT-###
/spec-ops-auto SPEC-KIT-### --from plan

# Evidence monitoring
/spec-evidence-stats --spec SPEC-KIT-###
```

**Documentation Validation:**
```bash
scripts/doc-structure-validate.sh --mode=templates
python3 scripts/spec-kit/lint_tasks.py
```

## 6. Current State & Roadmap

**Phase 3 Complete** (October 2025):
- ✅ All 13 /speckit.* commands functional
- ✅ Tiered model strategy (40% cost reduction)
- ✅ Template system (55% speed improvement)
- ✅ GitHub quality commands (clarify, analyze, checklist)
- ✅ Native status dashboard (instant, $0)
- ✅ Backward compatibility maintained

**Phase 3 Week 2** (Planned):
- [ ] Guardrail namespace: `/spec-ops-*` → `/guardrail.*`
- [ ] Remove legacy `/spec-*` enum variants
- [ ] Final testing and release notes
- [ ] Migration complete

**Future Considerations:**
- Extract spec-kit to separate repo vs embedded tooling
- Cost tracking telemetry for governance
- Evidence archival strategy for >25MB SPECs
- Tier 1 optimization (single agent for scaffolding)

## 7. Risks & Mitigations

**Agent Reliability:**
- **Risk:** Gemini occasional empty output (1-byte results)
- **Mitigation:** Orchestrator continues with 2/3 agents, consensus valid ✅

**Evidence Growth:**
- **Risk:** Repository size growth from evidence artifacts
- **Mitigation:** 25MB soft limit, monitoring via `/spec-evidence-stats`, archival strategy planned

**Fork Maintenance:**
- **Risk:** Upstream anthropics/claude-code changes conflict with spec-kit
- **Mitigation:** FORK_DEVIATIONS.md documents all changes, rebase strategy defined

**Model Costs:**
- **Risk:** Multi-agent execution expensive at scale
- **Mitigation:** Tiered strategy (40% reduction), native Tier 0 where possible ✅

**Documentation Drift:**
- **Risk:** Docs become outdated as system evolves
- **Mitigation:** Constitution requires docs updates per SPEC, CLAUDE.md as authoritative source ✅

## 8. Success Metrics

**Automation:**
- ✅ Idea → implementation without manual intervention
- ✅ 95%+ consensus resolution (arbiter handles conflicts)
- ✅ <5% deadlocks requiring human input

**Performance:**
- ✅ 55% faster generation via templates (13 min vs 30 min)
- ✅ 40% cost reduction via tiered strategy ($15→$11)
- ✅ 30% faster via parallel agent spawning
- ✅ <1s status queries (native, no agents)

**Quality:**
- ✅ Multi-model perspectives catch gaps
- ✅ Evidence trails enable debugging
- ✅ Constitution compliance enforced
- ✅ Cross-artifact consistency validated

## 9. Example Use Cases

**Kavedarr (Media Automation):**
- Uses spec-kit to develop media asset management features
- HAL MCP validates Kavedarr API endpoints
- Example SPECs: SPEC-KIT-DEMO, SPEC-KIT-045-mini

**Other Projects:**
- Framework is project-agnostic
- Customize prompts, guardrails, validation per project
- Template system adaptable to any domain

## 10. Open Questions

- Default HAL mode: mock or live?
- Evidence archival strategy for >25MB SPECs
- Guardrail namespace finalization: `/guardrail.*` vs `/spec-ops-*`
- Separate spec-kit repo vs embedded in Codex fork?
- Cost tracking telemetry for governance reporting

## 11. Review Notes

**Phase 3 standardization:** 2025-10-15
- ✅ All architecture sections updated for current state
- ✅ /speckit.* namespace documented
- ✅ Tiered model strategy detailed
- ✅ Template system performance validated
- ✅ Multi-agent orchestration fully operational
- Next: Week 2 guardrail separation, final release

**Reviewed by:** Spec-Kit maintainers + Claude Code analysis (Sonnet 4.5)
**Review date:** 2025-10-15
**Review verdict:** v1.1 approved - Phase 3 architecture accurately reflects implementation
**Status:** Current and authoritative
**Next review trigger:** After Phase 3 Week 2 completion or architectural changes

Document owner: @just-every/automation
