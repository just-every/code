# CLAUDE.md — How Claude Code Works In This Repo

This playbook gives Claude Code everything it needs to operate safely inside **just-every/code**. Read it before touching the tree and keep it open while you work.

## 0. Prerequisites & Known Limitations (October 2025)
- **Foundation docs now exist:** `product-requirements.md` and `PLANNING.md` were added in response to guardrail audits. If either goes missing, stop and recreate or escalate.
- **Consensus automation is pending:** multi-agent prompts exist but are still triggered manually from the TUI. `/spec-plan --consensus` is aspirational until the consensus runner lands.
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
- `/constitution` – multi-agent edits to constitution + product requirements. Surface disagreements and require human confirmation.
- `/specify` – single high-reasoning GPT-5 Codex pass to draft/update PRDs and sync SPEC.md.
- `/plan`, `/tasks`, `/implement`, `/validate`, `/audit`, `/unlock` – multi-agent flows. Follow the Spec Kit skeletons exactly (see §4).
- `/spec-ops-plan|tasks|implement|validate|audit|unlock` – guardrail shell wrappers. They must land telemetry under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`.
- `/spec-ops-auto` – wraps `scripts/spec_ops_004/spec_auto.sh` (plan→unlock). Enforce clean tree unless `SPEC_OPS_ALLOW_DIRTY=1`.
- `/spec-evidence-stats` – wraps `scripts/spec_ops_004/evidence_stats.sh`; use after large runs to monitor repo footprint.
- `/spec-consensus` – inspects local-memory consensus artifacts for a given stage.

If any slash command or CLI is unavailable, degrade gracefully and record which model/step was substituted.

**Slash command quick start**
- Guardrail only: `/spec-ops-plan SPEC-KIT-DEMO`
- Manual consensus (current reality): run `/spec-plan SPEC-KIT-DEMO Align ingestion tasks` then capture results in local-memory.
- Full pipeline (shell stages only): `/spec-ops-auto SPEC-KIT-DEMO --from plan`
- Guardrail + consensus (dry-run default): `/spec-plan --consensus SPEC-KIT-DEMO Align ingestion tasks`
- Execute consensus (requires Codex CLI creds): `/spec-plan --consensus-exec SPEC-KIT-DEMO Align ingestion tasks`
- Evidence footprint: `/spec-evidence-stats --spec SPEC-KIT-DEMO`

## 3. Telemetry & Evidence Expectations
- Telemetry schema v1: every JSON needs `command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts[]`.
- Stage-specific fields:
  - Plan – `baseline.mode`, `baseline.artifact`, `baseline.status`, `hooks.session.start`.
  - Tasks – `tool.status`.
  - Implement – `lock_status`, `hook_status`.
  - Validate/Audit – `scenarios[{name,status}]` (`passed|failed|skipped`).
  - Unlock – `unlock_status`.
- Enable `SPEC_OPS_TELEMETRY_HAL=1` during HAL smoke tests to capture `hal.summary.{status,failed_checks,artifacts}`. Collect both healthy and degraded runs.
- `/spec-auto` halts on schema violations or missing artifacts. Investigate immediately.
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
- When HAL telemetry is missing or malformed, pause and re-run the relevant `/spec-ops-*` stage with `SPEC_OPS_TELEMETRY_HAL=1` after restoring prerequisites.
- For consensus drift (agents missing, conflicting verdicts), re-run the stage or run `/spec-consensus <SPEC-ID> <stage>` and include findings in the report.

Stay inside these guardrails and Claude Code will be a courteous teammate instead of an incident report.
