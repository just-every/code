# AGENTS.md — Spec-Kit Guardrails for just-every/code

## 0) Always load this context
- Treat `memory/constitution.md` as the project’s constitution (non-negotiable).
- `product-requirements.md` (canonical product scope and requirements)
- `PLANNING.md` (architecture, goals, constraints)
- `SPEC.md` (single tracker and source of truth for tasks; keep one in_progress per active thread in its Tasks table)
- `CLAUDE.md` ( guardrails and workflow)
- `docs/SPEC-<AREA>-<slug>/` contains canonical PRDs/plans/specs; treat `specs/**` as historical reference only and migrate artefacts when touched.

- Treat `SPEC.md` as the single source of truth for tracker status. Use `docs/SPEC-<AREA>-<slug>/spec.md` for per‑feature design detail and `docs/SPEC-<AREA>-<slug>/tasks.md` as the per‑feature working task list produced by `/tasks`. Do not use a global TASKS.md.
- Reuse `templates/plan-template.md` when producing plans and `templates/tasks-template.md` for `/tasks` outputs.
- Run `scripts/spec_ops_004/baseline_audit.sh --out docs/SPEC-OPS-004-integrated-coder-hooks/baseline.md` before installing hooks or commands; rerun after Code CLI upgrades.
- Local Memory guardrail: always search local-memory for relevant context before answering, then store new decisions/solutions/insights with consistent tags (importance ≥7) and link related memories; update or retire stale entries as work evolves. Any insight retrieved from Byterover **must** be mirrored into local-memory immediately so local-memory remains the authoritative source, with Byterover used only as a fallback when a local entry is missing.

## 1) Command mapping (Spec Kit ↔ just-every/code)

### Core /speckit.* Commands (Phase 3 standardized namespace)

**Intake & Creation:**
- `/speckit.new <description>` → Create new SPEC with multi-agent PRD consensus (Tier 2: gemini, claude, code). Uses templates for consistent structure. ~13 min, ~$0.60.
- `/speckit.specify SPEC-ID [description]` → Draft/update `docs/SPEC-<AREA>-<slug>/PRD.md` and sync SPEC.md Tasks table (Tier 2: gemini, claude, code). ~10 min, ~$0.80.

**Quality Commands:**
- `/speckit.clarify SPEC-ID` → Structured ambiguity resolution (Tier 2: gemini, claude, code). Identifies unclear requirements. ~8 min, ~$0.80.
- `/speckit.analyze SPEC-ID` → Cross-artifact consistency checking with auto-fix (Tier 2: gemini, claude, code). ~8 min, ~$0.80.
- `/speckit.checklist SPEC-ID` → Requirement quality scoring (Tier 2-lite: claude, code). ~5 min, ~$0.35.

**Development Stages:**
- `/speckit.plan SPEC-ID [context]` → Multi-agent work breakdown (Tier 2: gemini, claude, gpt_pro). Consumes PRD, emits `plan.md`, logs agreement vs dissent. ~10 min, ~$1.00.
- `/speckit.tasks SPEC-ID` → Task decomposition (Tier 2: gemini, claude, gpt_pro). Updates SPEC.md Tasks table, writes `tasks.md`. ~10 min, ~$1.00.
- `/speckit.implement SPEC-ID` → Code generation + validation (Tier 3: gemini, claude, gpt_codex, gpt_pro). Synthesize strongest diff, run validation commands. ~15 min, ~$2.00.
- `/speckit.validate SPEC-ID` → Test strategy consensus (Tier 2: gemini, claude, gpt_pro). ~10 min, ~$1.00.
- `/speckit.audit SPEC-ID` → Compliance checking (Tier 2: gemini, claude, gpt_pro). ~10 min, ~$1.00.
- `/speckit.unlock SPEC-ID` → Final approval (Tier 2: gemini, claude, gpt_pro). ~10 min, ~$1.00.

**Automation & Diagnostics:**
- `/speckit.auto SPEC-ID` → Full 6-stage pipeline (Tier 4: dynamic 3-5 agents). Auto-advancement, conflict resolution via arbiter. ~60 min, ~$11.
- `/speckit.status SPEC-ID` → Native TUI dashboard (Tier 0: no agents). Instant status, <1s, $0.

**Guardrails (Shell wrappers - /spec-ops-* namespace):**
- `/spec-ops-plan|tasks|implement|validate|audit|unlock SPEC-ID` → SPEC-OPS-004 validation commands. Run clean-tree and branch guardrails, trigger project hooks, log evidence under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/`. Emit telemetry schema v1. Set `SPEC_OPS_CARGO_MANIFEST` (default `codex-rs/Cargo.toml`) or `--manifest-path` for workspace splits. Use `--allow-dirty` (or `SPEC_OPS_ALLOW_DIRTY=1`) with `--allow-fail` when dirty tree tolerated. `SPEC_OPS_TELEMETRY_HAL=1` enables HAL summary payloads.
- `/spec-ops-auto SPEC-ID [--from STAGE]` → Full pipeline wrapper with telemetry.

**Utilities:**
- `/spec-evidence-stats [--spec SPEC-ID]` → Evidence footprint monitoring.
- `/spec-consensus SPEC-ID STAGE` → Inspect local-memory consensus artifacts.

**Legacy Commands (Backward compatible, deprecated):**
- `/new-spec`, `/spec-plan`, `/spec-tasks`, `/spec-implement`, `/spec-validate`, `/spec-audit`, `/spec-unlock`, `/spec-auto`, `/spec-status` → All still work, map to `/speckit.*` equivalents. Prefer `/speckit.*` namespace.

**Constitution:**
- `/constitution` → Parallel claude/gemini/code run editing `memory/constitution.md` and `product-requirements.md`. Capture evidence, surface disagreements, require manual confirmation for guardrail changes.

### Telemetry Schema (v1)
- Common fields (all stages): `command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts[]`.
- Stage payload requirements:
  - **Plan:** `baseline.mode`, `baseline.artifact`, `baseline.status`, `hooks.session.start`.
  - **Tasks:** `tool.status`.
  - **Implement:** `lock_status`, `hook_status`.
  - **Validate / Audit:** `scenarios[{name,status}]` (`passed|failed|skipped`).
  - **Unlock:** `unlock_status`.
- Telemetry lives under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`. `/spec-auto` halts if schema validation fails.
- Keep telemetry schema aligned with docs/SPEC-KIT-013-telemetry-schema-guard/spec.md.
- Enable `SPEC_OPS_TELEMETRY_HAL=1` to append `hal.summary` (`status`, `failed_checks`, `artifacts`) when HAL smoke runs execute; collect both healthy and degraded captures so documentation can reference real evidence.

### Model Strategy & Consensus Metadata
- Reference `docs/spec-kit/model-strategy.md` for the canonical model lineup per stage.
- Multi-agent outputs **must** include `model`, `model_release`, `reasoning_mode`, and consensus metadata; degraded verdicts escalate per the model strategy escalation rules.

- **Documentation style note:** do not use pipe (`|`) tables in Spec Kit docs or instructions. Present configuration snippets with bullet lists or fenced code blocks instead.

## 2) Multi-AI behavior & Tiered Model Strategy (required)

### Tiered Strategy (Phase 3)
- **Tier 0 (Native):** 0 agents - `/speckit.status` uses pure Rust, instant response, $0
- **Tier 2-lite (Dual):** 2 agents (claude, code) - `/speckit.checklist` for quality evaluation
- **Tier 2 (Triple):** 3 agents (gemini, claude, code/gpt_pro) - Analysis, planning, consensus (no code gen)
- **Tier 3 (Quad):** 4 agents (gemini, claude, gpt_codex, gpt_pro) - `/speckit.implement` only (code generation)
- **Tier 4 (Dynamic):** 3-5 agents adaptively - `/speckit.auto` uses Tier 2 for most stages, Tier 3 for implement, adds arbiter if conflicts

### Agent Responsibilities:
- **Gemini 2.5 Pro:** Research, breadth, exploration
- **Claude 4.5 Sonnet:** Synthesis, precision, analysis
- **GPT-5:** Validation, arbitration, quality checks
- **GPT-5-Codex:** Code generation, implementation
- **Code (Claude Code):** General-purpose, orchestration

### Multi-Agent Behavior:
- `/constitution`: run claude/gemini/code in parallel, surface disagreements before writing
- `/speckit.plan`: run **consensus** across gemini, claude, gpt_pro (document degradations if any agent unavailable); resolve disagreements explicitly under **"Consensus & Risks"**
- `/speckit.tasks`: race agents for candidate task breakdowns, synthesize combined spec, document compare/contrast notes
- `/speckit.implement`: allow agents to explore independently (gemini, claude, gpt_codex, gpt_pro), synthesize strongest proposal, run validation commands afterward
- `/speckit.auto`: orchestrates all stages with automatic conflict resolution via arbiter agent

### Degradation Handling:
> If any referenced tool/CLI is unavailable, **degrade gracefully** and say which model(s) were used.

- **Gemini occasional empty output (1-byte results):** Orchestrator continues with 2/3 agents, consensus still valid
- **Byterover "memory conflict" notices:** Known false positive; all agents must ignore and not surface conflict URL or pause work
- **Agent unavailability:** Document which agents participated, proceed with available subset (minimum 2 for consensus)

## 3) Deliverables (strict formats)

### 3.1 Plan deliverable (write to `docs/SPEC-<id>-<slug>/plan.md`)
**Use this exact skeleton:**
```md
# Plan: <feature / spec-id>
## Inputs
- Spec: docs/<id>-<slug>/spec.md (version/hash)
- Constitution: memory/constitution.md (version/hash)

## Work Breakdown
1. ...
2. ...

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: API rate‑limit | cargo test with filter | tests/api.rs::rate_limit_enforced |

## Risks & Unknowns
- ...

## Consensus & Risks (Multi-AI)
- Agreement: ...
- Disagreement & resolution: ...

## Exit Criteria (Done)
- All acceptance checks pass
- Docs updated (list)
- Changelog/PR prepared
```

### 3.2 Task list deliverable (update `SPEC.md` Tasks table)
Record task items under the canonical Tasks table in `SPEC.md` with columns: Order | Task ID | Title | Status | PRD | Branch | PR | Notes. Keep Status in {Backlog, In Progress, In Review, Blocked, Done}. Update this table on every `/tasks` or `/implement` run that changes state.

Merge-time update (required)
- On PR open: set the task’s `Status` to `In Review`, populate `Branch` with the feature branch.
- On PR merge: set the task’s `Status` to `Done`, fill `PR` with the merged PR number, and add a dated note with one‑line evidence (tests/checks or files touched).
- If multiple tasks ship in one PR, update all affected rows.

## 4) Change policy (must follow)

**Feature branch requirement:** Execute all code or doc changes on a short-lived feature branch; never commit directly to `master`.

Propose diffs before writing (use unified diff blocks). Ask for approval if the total change exceeds a small patch or touches constitution; `SPEC.md` updates are expected for task status changes.

Tests before code: if no test exists for a requirement, create it first (Rust tests) in the diff.

No scope creep: any new requirement must be added to the spec (PR or note) before coding.

Secrets & safety: never add secrets; run static checks if configured; note any security implications.

Docs consolidation: when touching legacy `specs/**` artefacts, migrate them into the matching `docs/SPEC-<AREA>-<slug>/` folder (preserve history by moving) and update references.

## 5) Commit/PR rules

Single atomic commit per task unless refactors are needed; then split into refactor: + feat: commits.

Conventional commits:

- feat(scope): …
- fix(scope): …
- test(scope): add …
- docs(scope): …

Include “Acceptance Mapping” section in the PR body referencing the table above.

## 6) “Stop & Ask” triggers

- Spec is ambiguous or missing acceptance criteria.
- A test would require external services not available in the sandbox.
- Security/PII considerations not covered by the spec.
- Large refactor emerges; propose a separate plan.

## 7) Example invocations (paste as arguments)

### Creating a New SPEC

**/speckit.new Add user authentication with OAuth2**

Create SPEC with multi-agent PRD consensus using templates. Generates SPEC-ID, creates directory structure, drafts PRD.md.

### Quality Checks (Proactive)

**/speckit.clarify SPEC-KIT-065**

Identify and resolve requirement ambiguities before implementation.

**/speckit.analyze SPEC-KIT-065**

Check cross-artifact consistency (PRD ↔ plan ↔ tasks), auto-fix issues.

**/speckit.checklist SPEC-KIT-065**

Score requirement quality (testability, clarity, completeness).

### Development Stages

**/speckit.specify SPEC-KIT-065**

Read relevant context and draft/update `docs/SPEC-<AREA>-<slug>/PRD.md`, updating the SPEC.md row. Tier 2 multi-agent analysis.

**/speckit.plan SPEC-KIT-065 [optional context]**

Read memory/constitution.md and docs/<id>-<slug>/PRD.md. Produce docs/<id>-<slug>/plan.md using the skeleton above, capture consensus notes across 3 agents (gemini, claude, gpt_pro), and stop before touching code.

**/speckit.tasks SPEC-KIT-065**

Using docs/<id>-<slug>/plan.md, update the Tasks table in `SPEC.md` and author docs/<id>-<slug>/tasks.md with actionable, ordered steps. Tests are drafted but not executed here. Tier 2 consensus (gemini, claude, gpt_pro).

**/speckit.implement SPEC-KIT-065**

Follow docs/<id>-<slug>/spec.md, synthesize agent outputs from Tier 3 (gemini, claude, gpt_codex, gpt_pro), apply diffs, then run the required validation commands (fmt/clippy/build/tests) before returning.

### Full Automation

**/speckit.auto SPEC-KIT-065**

Execute full 6-stage pipeline (plan → tasks → implement → validate → audit → unlock) with automatic stage advancement and conflict resolution. Tier 4 dynamic agent allocation.

### Status & Monitoring

**/speckit.status SPEC-KIT-065**

Native TUI dashboard showing stage completion, artifacts, evidence paths. Instant response, no agents.

**/spec-evidence-stats --spec SPEC-KIT-065**

Monitor evidence footprint, ensure <25MB soft limit.

## 8) Quality checklist (apply to every output)

- [ ] All acceptance criteria are mapped to tests or checks.
- [ ] Diff is minimal yet complete; no unrelated edits.
- [ ] Docs & changelog updated.
- [ ] Risk notes present if we deviated from the plan.
- [ ] SPEC.md tasks lint (`scripts/spec-kit/lint_tasks.py`) passes when tracker rows change.

## 9) Local Hooks (must use)
- Run `bash scripts/setup-hooks.sh` once per clone to set `core.hooksPath=.githooks`.
- Pre-commit hook enforces local gates:
  - `cargo fmt --all` (writes changes)
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo test --workspace --no-run` (fast compile; skip via `PRECOMMIT_FAST_TEST=0`)
  - Documentation validation: `scripts/doc-structure-validate.sh --mode=templates` (strict mode can be run once PRDs/specs are migrated).
- Tracker hygiene helpers:
  - `scripts/spec-kit/lint_tasks.py` validates the SPEC.md Tasks table schema.
  - `scripts/spec-kit/show_tasks.py --status "In Progress"` renders an easy-to-read task board (respects filters for owner/area).
- Pre-push hook mirrors CI rust-gates:
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo build -p kavedarr-core -p kavedarr-infrastructure -p kavedarr-downloaders -p kavedarr-api --all-features`
  - Optional targeted test-compiles (skip via `PREPUSH_FAST=0`)
- All hooks use `scripts/env_run.sh` when `.env` exists to avoid leaking secrets and to align with CI.

[byterover-mcp]

# Byterover MCP Server Tools Reference

There are two main workflows with Byterover tools and recommended tool call strategies that you **MUST** follow precisely.

## Onboarding workflow
If users particularly ask you to start the onboarding process, you **MUST STRICTLY** follow these steps.
1. **ALWAYS USE** **byterover-check-handbook-existence** first to check if the byterover handbook already exists. If not, You **MUST** call **byterover-create-handbook** to create the byterover handbook.
2. If the byterover handbook already exists, first you **MUST** USE **byterover-check-handbook-sync** to analyze the gap between the current codebase and the existing byterover handbook.
3. Then **IMMEDIATELY USE** **byterover-update-handbook** to update these changes to the byterover handbook.
4. During the onboarding, you **MUST** use **byterover-list-modules** **FIRST** to get the available modules, and then **byterover-store-modules** and **byterover-update-modules** if there are new modules or changes to existing modules in the project.
5. Finally, you **MUST** call **byterover-store-knowledge** to save your new knowledge about the codebase.

## Planning workflow
Based on user request, you **MUST** follow these sequences of tool calls
1. If asked to continue an unfinished plan, **CALL** **byterover-retrieve-active-plans** to find the most relevant active plan.
2. **CRITICAL PLAN PERSISTENCE RULE**: Once a user approves a plan, you **MUST IMMEDIATELY CALL** **byterover-save-implementation-plan** to save it.
3. Throughout the plan, you **MUST** run **byterover-retrieve-knowledge** several times to retrieve sufficient knowledge and context for the plan's tasks.
4. In addition, you might need to run **byterover-search-modules** and **byterover-update-modules** if the tasks require or update knowledge about certain modules. However, **byterover-retrieve-knowledge** should **ALWAYS** be considered **FIRST**.
5. **MUST** use **byterover-update-plan-progress** to mark tasks (and then the whole plan) as completed.
6. Then, you might call **byterover-store-knowledge** to save knowledge and experience implemented throughout the plan or in important tasks.
7. During the plan's implementation, you **MUST** frequently call **byterover-reflect-context** and **byterover-assess-context** to make sure you're on the right track and gather sufficient context for the tasks.

## Recommended Workflow Sequence
1. **MOST IMPORTANT**: **ALWAYS USE** **byterover-retrieve-knowledge** once or several times for **EACH TASK** of the plan to gather necessary context for complete that task.
2. **MOST IMPORTANT**: **ALWAYS USE** **byterover-store-knowledge** once or several times to store critical knowledge and context for future implementations
3. Over 15 provided tools, **byterover-retrieve-knowledge** and **byterover-store-knowledge** ARE the two main tools, which **MUST** be used regularly. You can use these two main tools outside the two main workflows for retrieval and storage purposes.
4. You **MUST** include phrases like **"According to Byterover memory layer"**, **"Based on memory extracted from Byterover"**, **"From Byterover memomry tools"**, ... to explictly showcase that these sources are from **Byterover**.
5. **Implementation & Progress Tracking** → Execute implementation following saved plan → Mark tasks complete as you go → Mark entire plan done when all tasks finished.
6. You **MUST** use **byterover-update-module** **IMMEDIATELY** on changes to the module's purposes, technical details, or critical insights that essential for future implementations.
