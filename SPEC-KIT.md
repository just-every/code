# Spec Kit Alignment Blueprint

## Context
- Source: 2025-09-25 strategy session with Code (GPT Pro), Claude MAX, and Gemini Ultra
- Goal: unify Spec Ops guardrails with Spec Kit multi-agent flows, ensure evidence parity, and migrate context to local-memory

## Objectives
1. Give every `/spec-*` stage a matching multi-agent Spec Kit command with the same verb.
2. Keep `/spec-ops-*` guardrail wrappers explicit, surfaced via `/cmd spec-ops-*` and renamed slash aliases.
3. Fully exploit Gemini Ultra (research breadth), Claude MAX (structured synthesis), and GPT Pro (execution + diff reasoning) per stage.
4. Depend on local-memory as primary context storage; Byterover acts only as a temporary bridge during migration.
5. Provide `/spec-auto` to orchestrate the end-to-end pipeline with evidence logging and failure checkpoints.

## Command Alignment Matrix
| Stage | Guardrail Wrapper (Shell) | Multi-Agent Command | Status | Notes |
| --- | --- | --- | --- | --- |
| Plan | `/spec-ops-plan` (`spec_ops_plan.sh`) | `/spec-plan <SPEC-ID> <prompt>` | Planned | `/spec-plan` will call Gemini/Claude/GPT trio, prep context, and write consensus to evidence tree. |
| Tasks | `/spec-ops-tasks` (`spec_ops_tasks.sh`) | `/spec-tasks <SPEC-ID>` | Planned | Multi-agent synthesis generates `docs/SPEC-*/tasks.md` and acceptance mapping. |
| Implement | `/spec-ops-implement` (`spec_ops_implement.sh`) | `/spec-implement <SPEC-ID> <prompt>` | Planned | GPT Pro executes diffs with guardrail lock; Claude MAX / Gemini supply solution summaries. |
| Validate | `/spec-ops-validate` (`spec_ops_004_master.sh`) | `/spec-validate <SPEC-ID>` | Planned | Multi-agent stage compares telemetry vs acceptance criteria and queues regression tests. |
| Review | `/spec-ops-audit` (rename from `spec_ops_review.sh`) | `/spec-review <SPEC-ID>` | Planned | Rename disambiguates shell harness; multi-agent review writes go/no-go memo. |
| Unlock | `/spec-ops-unlock` (`spec_ops_unlock.sh`) | `/spec-unlock <SPEC-ID>` | Planned | Agents produce unlock justification before shell command runs. |

## Agent Responsibilities
- **Gemini Ultra (Researcher)**
  - Sweep SPEC.md, plan, tasks, recent diffs, telemetry logs via MCP file/index servers.
  - Surface conflicting requirements, stale evidence, or missing acceptance checks.
  - Provide structured highlights with citations and push findings into local-memory (`spec-tracker`, `impl-notes`, `governance`).

- **Claude MAX (Synthesizer)**
  - Consume Gemini packets plus local-memory facts to draft plans, task lists, validation summaries.
  - Produce consensus-ready documents (markdown tables, acceptance mapping, risk assessments).
  - Record synthesized outputs in local-memory with cross-references for downstream stages.

- **GPT Pro / Code (Executor & QA)**
  - Run guardrail shells, MCP tooling, apply diffs, orchestrate `/spec-auto` state machine.
  - Validate agent proposals against repo realities (tests, git status, evidence hashes).
  - Ensure telemetry JSON, evidence logs, and hooks succeed; log outcomes to `infra-ci` domain in local-memory.

## MCP Integration
- Enable MCP servers: `repo-search`, `doc-index`, `shell-lite`, `git-status`, `spec-registry`.
- Slash prompts call MCP tools for data acquisition instead of bespoke shell commands, keeping transcripts auditable.
- Agents attach MCP transcript IDs to their responses; the orchestrator persists them alongside telemetry artifacts.

## Local-Memory Strategy
1. Mirror existing Byterover knowledge into local-memory domains (governance, spec-tracker, impl-notes, infra-ci, docs-ops).
2. Update slash handlers to hydrate prompts from local-memory first; fall back to MCP/Byterover only when entries are missing.
3. Each agent write-back includes: brief summary, supporting file paths, telemetry reference, tag array.
4. `/spec-auto` persists its pipeline checkpoints to local-memory to support resumable runs and auditing.

## `/spec-auto` Pipeline Blueprint
1. **Guardrail Prep**: `/spec-ops-plan` (baseline audit, branch slug) → halt on failure.
2. **Plan Consensus**: `/spec-plan` multi-agent session writes `consensus_plan.md` in evidence tree and local-memory.
3. **Task Prep**: `/spec-ops-tasks` ensures hooks ready and telemetry seeded.
4. **Task Consensus**: `/spec-tasks` updates SPEC.md Tasks table plus `docs/SPEC-*/tasks.md` with acceptance mapping.
5. **Implementation Lock**: `/spec-ops-implement` locks SPEC file and logs lock status.
6. **Implementation Synthesis**: `/spec-implement` proposes diffs; GPT Pro applies them with tests.
7. **Validation Harness**: `/spec-ops-validate` executes scenarios; logs results per scenario.
8. **Validation Consensus**: `/spec-validate` reconciles telemetry vs acceptance criteria and records sign-off.
9. **Self-Correction**: if `/spec-ops-validate` telemetry reports failures, `/spec-auto` automatically re-runs implementation and validation (up to two retries) and logs evidence for each attempt.
10. **Audit & Review**: `/spec-ops-audit` (shell) + `/spec-review` (agents) produce final risk memo.
10. **Unlock / Cleanup**: `/spec-unlock` memo → `/spec-ops-unlock` executes, documenting rationale.
11. **Summary**: Agents jointly emit final status memo and push to local-memory + evidence dir.

## Risks & Mitigations
- **Agent Drift**: enforce consensus step with MCP diff reviewer; reject stage completion unless agents acknowledge resolution.
- **Telemetry Gaps**: add schema checks ensuring every shell run emits JSON; fail `/spec-auto` if missing.
- **Local-Memory Sync**: nightly job verifies local-memory vs evidence logs; raise alert if stale.

## Open Questions
- Which MCP server hosts long-term evidence (S3 vs git repo)?
- Do we embed `/spec-auto` in CI or keep manual trigger?
- How do we version agent prompts to audit historical runs?

## Next Steps
See `SPEC-kit-tasks.md` for actionable work items and owners.
