# CLAUDE.md — How Claude Code Works In This Repo

This playbook gives Claude Code everything it needs to operate safely inside **just-every/code**. Read it before touching the tree and keep it open while you work.

## 0. Prerequisites & Known Limitations (October 2025)
- **Foundation docs now exist:** `product-requirements.md` and `PLANNING.md` were added in response to guardrail audits. If either goes missing, stop and recreate or escalate.
- **Consensus automation is pending:** multi-agent prompts exist but are still triggered manually from the TUI. `/speckit.plan --consensus` is aspirational until the consensus runner lands.
- **Cargo workspace location:** run Rust commands from `codex-rs/` (for example `cd codex-rs && cargo test -p codex-tui spec_auto`). Guardrail scripts set `SPEC_OPS_CARGO_MANIFEST` when needed, but manual commands must honour the workspace root.
- **HAL secrets:** full validation requires `HAL_SECRET_KAVEDARR_API_KEY`. If unavailable, set `SPEC_OPS_HAL_SKIP=1` (decision on default behaviour pending) and document the skip in results.
- **Evidence footprint:** keep evidence under the 25 MB per-SPEC soft limit; use `/spec-evidence-stats` after large runs.

## 1. Load These References Every Session
- `memory/constitution.md` – non‑negotiable project charter and guardrail canon.
- `product-requirements.md` – canonical product scope. If missing, pause and ask the user for direction.
- `PLANNING.md` – high-level architecture, goals, constraints. Same rule: request it if absent.
- `SPEC.md` – single source of truth for task tracking; only one `In Progress` row at a time.
- `docs/SPEC-<AREA>-<slug>/` – per-feature specs, plans, tasks. Treat `specs/**` as archival only.
- `AGENTS.md` (this document’s partner) – Spec-Kit automation guardrails.

Always check local-memory before answering, then write back key outcomes (importance ≥7) so it stays authoritative. If you consult the Byterover layer, mirror the insight to local-memory immediately and call out the source with language like “According to Byterover memory layer…”.

## 2. Operating Modes & Slash Commands

### Core Spec-Kit Commands (/speckit.* namespace)

**Intake & Creation:**
- `/speckit.new <description>` – Create new SPEC with multi-agent PRD consensus (Tier 2: 3 agents - gemini, claude, code). Uses templates for consistent structure. ~13 min, ~$0.60.
- `/speckit.specify SPEC-ID [description]` – Draft/update PRD with multi-agent analysis (Tier 2: 3 agents - gemini, claude, code). ~10-12 min, ~$0.80.

**Quality Commands:**
- `/speckit.clarify SPEC-ID` – Structured ambiguity resolution (Tier 2: 3 agents - gemini, claude, code). Identifies unclear requirements. ~8-10 min, ~$0.80.
- `/speckit.analyze SPEC-ID` – Cross-artifact consistency checking with auto-fix (Tier 2: 3 agents - gemini, claude, code). ~8-10 min, ~$0.80.
- `/speckit.checklist SPEC-ID` – Requirement quality scoring (Tier 2-lite: 2 agents - claude, code). ~5-8 min, ~$0.35.

**Development Stages:**
- `/speckit.plan SPEC-ID [context]` – Multi-agent work breakdown (Tier 2: 3 agents - gemini, claude, gpt_pro). ~10-12 min, ~$1.00.
- `/speckit.tasks SPEC-ID` – Task decomposition with consensus (Tier 2: 3 agents - gemini, claude, gpt_pro). ~10-12 min, ~$1.00.
- `/speckit.implement SPEC-ID` – Code generation + validation (Tier 3: 4 agents - gemini, claude, gpt_codex, gpt_pro). ~15-20 min, ~$2.00.
- `/speckit.validate SPEC-ID` – Test strategy consensus (Tier 2: 3 agents - gemini, claude, gpt_pro). ~10-12 min, ~$1.00.
- `/speckit.audit SPEC-ID` – Compliance checking (Tier 2: 3 agents - gemini, claude, gpt_pro). ~10-12 min, ~$1.00.
- `/speckit.unlock SPEC-ID` – Final approval (Tier 2: 3 agents - gemini, claude, gpt_pro). ~10-12 min, ~$1.00.

**Automation:**
- `/speckit.auto SPEC-ID` – Full 6-stage pipeline with auto-advancement (Tier 4: dynamic 3-5 agents, uses Tier 2 for most stages, Tier 3 for implement, adds arbiter if conflicts). ~60 min, ~$11.

**Diagnostic:**
- `/speckit.status SPEC-ID` – Native TUI dashboard (Tier 0: instant, no agents). Shows stage completion, artifacts, evidence paths. <1s, $0.

### Guardrail Commands (Shell wrappers)

- `/guardrail.plan SPEC-ID` – Baseline + policy checks for plan. Must land telemetry under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`. (note: legacy `/spec-ops-plan` still works)
- `/guardrail.tasks SPEC-ID` – Validation for tasks stage. (note: legacy `/spec-ops-tasks` still works)
- `/guardrail.implement SPEC-ID` – Pre-implementation checks. (note: legacy `/spec-ops-implement` still works)
- `/guardrail.validate SPEC-ID` – Test harness execution. (note: legacy `/spec-ops-validate` still works)
- `/guardrail.audit SPEC-ID` – Compliance scanning. (note: legacy `/spec-ops-audit` still works)
- `/guardrail.unlock SPEC-ID` – Final validation. (note: legacy `/spec-ops-unlock` still works)
- `/guardrail.auto SPEC-ID [--from STAGE]` – Full pipeline wrapper. Wraps `scripts/spec_ops_004/spec_auto.sh` (plan→unlock). Enforce clean tree unless `SPEC_OPS_ALLOW_DIRTY=1`. (note: legacy `/spec-ops-auto` still works)

### Utility Commands

- `/spec-evidence-stats [--spec SPEC-ID]` – Evidence footprint monitoring. Wraps `scripts/spec_ops_004/evidence_stats.sh`. Use after large runs to monitor repo footprint.
- `/spec-consensus SPEC-ID STAGE` – Inspect local-memory consensus artifacts for a given stage.

### Legacy Commands (Backward Compatible)

**Deprecated but still functional (will be removed in future release):**
- `/new-spec` → use `/speckit.new`
- `/spec-plan` → use `/speckit.plan`
- `/spec-tasks` → use `/speckit.tasks`
- `/spec-implement` → use `/speckit.implement`
- `/spec-validate` → use `/speckit.validate`
- `/spec-audit` → use `/speckit.audit`
- `/spec-unlock` → use `/speckit.unlock`
- `/spec-auto` → use `/speckit.auto`
- `/spec-status` → use `/speckit.status`

### Command Usage Examples

**Quick start (new feature):**
```bash
# Create SPEC
/speckit.new Add user authentication with OAuth2

# Quality checks (optional)
/speckit.clarify SPEC-KIT-###
/speckit.analyze SPEC-KIT-###
/speckit.checklist SPEC-KIT-###

# Full automation
/speckit.auto SPEC-KIT-###

# Check status
/speckit.status SPEC-KIT-###
```

**Individual stage workflow:**
```bash
# Manual stage-by-stage
/speckit.plan SPEC-KIT-065
/speckit.tasks SPEC-KIT-065
/speckit.implement SPEC-KIT-065
/speckit.validate SPEC-KIT-065
/speckit.audit SPEC-KIT-065
/speckit.unlock SPEC-KIT-065
```

**Guardrail validation:**
```bash
# Run guardrail checks (separate from multi-agent)
/guardrail.plan SPEC-KIT-065
/guardrail.auto SPEC-KIT-065 --from plan

# Monitor evidence footprint
/spec-evidence-stats --spec SPEC-KIT-065
```

### Tiered Model Strategy

**Tier 0: Native TUI** (0 agents, $0, <1s)
- `/speckit.status` - Pure Rust implementation

**Tier 1: Single Agent** (1 agent: code, ~$0.10, 1-3 min)
- Future optimization for deterministic scaffolding

**Tier 2-lite: Dual Agent** (2 agents: claude, code, ~$0.35, 5-8 min)
- `/speckit.checklist` - Quality evaluation without research

**Tier 2: Triple Agent** (3 agents: gemini, claude, code/gpt_pro, ~$0.80-1.00, 8-12 min)
- `/speckit.new`, `/speckit.specify`, `/speckit.clarify`, `/speckit.analyze`
- `/speckit.plan`, `/speckit.tasks`, `/speckit.validate`, `/speckit.audit`, `/speckit.unlock`
- Use for analysis, planning, consensus (no code generation)

**Tier 3: Quad Agent** (4 agents: gemini, claude, gpt_codex, gpt_pro, ~$2.00, 15-20 min)
- `/speckit.implement` only - Code generation + validation

**Tier 4: Dynamic** (3-5 agents adaptively, ~$11, 60 min)
- `/speckit.auto` - Uses Tier 2 for most stages, Tier 3 for implement, adds arbiter if conflicts

### Degradation & Fallbacks

If any slash command or CLI is unavailable, degrade gracefully and record which model/step was substituted. If Gemini agent fails (produces empty output), orchestrator continues with 2/3 agents - consensus still valid.

## 3. Telemetry & Evidence Expectations
- Telemetry schema v1: every JSON needs `command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts[]`.
- Stage-specific fields:
  - Plan – `baseline.mode`, `baseline.artifact`, `baseline.status`, `hooks.session.start`.
  - Tasks – `tool.status`.
  - Implement – `lock_status`, `hook_status`.
  - Validate/Audit – `scenarios[{name,status}]` (`passed|failed|skipped`).
  - Unlock – `unlock_status`.
- Enable `SPEC_OPS_TELEMETRY_HAL=1` during HAL smoke tests to capture `hal.summary.{status,failed_checks,artifacts}`. Collect both healthy and degraded runs.
- `/guardrail.auto` (or legacy `/spec-auto`) halts on schema violations or missing artifacts. Investigate immediately.
- Evidence root: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/`. Keep it under control with `/spec-evidence-stats`; propose offloading if any single SPEC exceeds 25 MB.

## 4. Deliverable Formats (No Deviations)
### Plans (`docs/SPEC-<id>-<slug>/plan.md`)
```
# Plan: <feature / spec-id>
## Inputs
- Spec: docs/<id>-<slug>/spec.md (version/hash)
- Constitution: memory/constitution.md (version/hash)

## Work Breakdown
1. …
2. …

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: … | … | … |

## Risks & Unknowns
- …

## Consensus & Risks (Multi-AI)
- Agreement: …
- Disagreement & resolution: …

## Exit Criteria (Done)
- All acceptance checks pass
- Docs updated (list)
- Changelog/PR prepared
```
(Spec Kit docs prefer bullets over Markdown tables, but this mapping table stays for acceptance clarity.)

### Tasks (`docs/SPEC-<id>-<slug>/tasks.md` + SPEC.md)
- Update SPEC.md’s Tasks table every time a `/tasks` or `/implement` run changes state. Columns: Order | Task ID | Title | Status | PRD | Branch | PR | Notes. Status ∈ {Backlog, In Progress, In Review, Blocked, Done}.
- On PR open: Status → `In Review`, populate `Branch`.
- On merge: Status → `Done`, fill `PR`, add dated note referencing evidence (tests or files).

## 5. Multi-Agent Expectations
- `/plan` – consensus across GPT-5 Codex, Claude Code, Gemini, Qwen. Document degradations and resolutions explicitly.
- `/tasks` – race agents, synthesize combined plan, note similarities/differences.
- `/implement` – agents explore separately; Claude helps synthesize strongest diff. Run validation (`cargo fmt`, `clippy`, tests, doc checks) **before** returning.
- `/validate` & `/audit` – ensure consensus metadata records `model`, `model_release`, `reasoning_mode`. Degraded verdicts escalate per `docs/spec-kit/model-strategy.md`.
- Ignore Byterover “memory conflict” notices—they are a known false positive.

> **Current limitation:** automated consensus capture is not yet wired. Treat these expectations as manual checklists until the consensus runner is implemented.

## 6. Tooling, Hooks, and Tests
- One-time: `bash scripts/setup-hooks.sh` to point Git at `.githooks`.
- Pre-commit (auto): `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --no-run` (skip with `PRECOMMIT_FAST_TEST=0`), `scripts/doc-structure-validate.sh --mode=templates`.
- Pre-push (mirrors CI): `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo build -p kavedarr-core -p kavedarr-infrastructure -p kavedarr-downloaders -p kavedarr-api --all-features` (+ optional targeted test-compiles, skip with `PREPUSH_FAST=0`).
- Always invoke guardrail scripts through `scripts/spec_ops_004/*` using `scripts/env_run.sh` when `.env` exists.
- No secrets, ever. If HAL secrets are required (`HAL_SECRET_KAVEDARR_API_KEY`), ask the user to supply them.

**Workspace reminder:** run Rust commands from `codex-rs/` (for example `cd codex-rs && cargo test -p codex-tui spec_auto`). Update `SPEC_OPS_CARGO_MANIFEST` in guardrail helpers if workspace layout changes.

## 7. Branch & Git Discipline
- Default branch name is **master**. Never reference `main`.
- Sync with `git fetch origin master` then `git merge --no-ff --no-commit origin/master` (no rebases).
- Do all work on short-lived feature branches; never commit directly to master.
- Stick to conventional commits: `feat(scope): …`, `fix(scope): …`, `test(scope): …`, `docs(scope): …`.
- Present diffs before applying (unified diff). Ask for approval if touching the constitution or shipping a large patch.
- One atomic commit per task unless a mechanical refactor is needed (split `refactor:` then feature commit).

## 8. When To Pause And Ask
- Missing or ambiguous acceptance criteria.
- Spec requires external services unavailable here.
- Security/privacy implications are unclear.
- Legacy `specs/**` artefact touched—plan migration before editing.
- Large refactor emerges unexpectedly.
- Required reference documents (`product-requirements.md`, `PLANNING.md`, relevant spec files) are absent.

## 9. Memory Workflow Checklist
1. **Before** solving: query local-memory for relevant notes. If nothing shows, fall back to Byterover, then echo findings back into local-memory.
2. **During** work: keep local-memory updates (importance ≥7) aligned with new decisions, tests, or telemetry.
3. **After** completing a step: store outcomes, including evidence paths and validation results.

## 10. Evidence & Validation Ritual
- Guardrail runs must have a clean tree unless specifically allowed (`SPEC_OPS_ALLOW_DIRTY=1`).
- Capture both success and failure artifacts; `/spec-auto` should be self-healing but document retries.
- After `/implement`, run the full validation harness (fmt, clippy, build/tests, doc validators). Attach logs or cite evidence files in local-memory and user reports.
- Keep `docs/spec-kit/spec-auto-automation.md` and `docs/spec-kit/evidence-baseline.md` updated when coverage changes.

## 11. Escalate Early
- Claude should explicitly state blockers, degraded guardrails, or missing telemetry.
- When HAL telemetry is missing or malformed, pause and re-run the relevant guardrail command (e.g., `/guardrail.plan`) with `SPEC_OPS_TELEMETRY_HAL=1` after restoring prerequisites. (note: legacy `/spec-ops-*` commands still work)
- For consensus drift (agents missing, conflicting verdicts), re-run the stage or run `/spec-consensus <SPEC-ID> <stage>` and include findings in the report.

Stay inside these guardrails and Claude Code will be a courteous teammate instead of an incident report.
