# Plan: Systematic Testing Framework for Spec-Auto Orchestrator (SPEC-KIT-045)
## Inputs
- Spec: docs/SPEC-KIT-045-design-systematic-testing-framework-for/spec.md (git 5bb15d6500d5b0ebb37ff8be4da60520cd2c3fd5)
- Constitution: memory/constitution.md (git 4e159c7eccd2cba0114315e385584abc0106834c)

## Work Breakdown
1. Agent spawn audit & fixes
   - Inspect orchestrator config, prompts, and credentials to ensure Gemini, Claude, GPT Pro, and GPT Codex launch for every stage when triggered from the TUI.
   - Capture remediation steps and log outputs so `/spec-auto` transcripts clearly show the four-model roster, and document the `--hal live` toggle for real HAL runs when needed.
2. Minimal fixture kit
   - Maintain a synthetic SPEC bundle (`docs/SPEC-KIT-045-mini/`) plus mocked agent/telemetry snapshots under 100 KB for fast stage rehearsal.
   - Document a lightweight copy/regeneration workflow (manual or future xtask) with checksum notes to detect drift. (2025-10-12 update: fixture created at 36 KB with checksums in `docs/SPEC-KIT-045-mini/checksums.sha256`.)
3. Stage execution playbooks (plan → unlock)
   - Define repeatable TUI sequences for `/spec-ops-plan|tasks|implement|validate|audit|unlock` against the fixture SPEC, including expected prompts, evidence locations, and resume (`--from`) behaviour.
   - Ensure each playbook records agent roster confirmation and stage-specific acceptance criteria.
4. Telemetry & evidence review
   - Build manual checklists for verifying telemetry JSON, logs, and SPEC.md/task patches after each stage run.
   - Normalize timestamp comparisons (regex/glob guidance) so operators can compare runs without spurious diffs.
5. Error-handling coverage
   - Document how to trigger guardrail failures, missing telemetry, agent dropout, and file-write errors via environment flags or fixture edits, then capture the resulting TUI evidence.
   - Record per-stage success vs failure triggers and the remediation guidance surfaced to the operator.
6. Fast-run documentation & operator workflow
   - Publish the sub-90-minute methodology covering which slash commands to run, how to switch HAL from mock → live using the new `--hal` flag, and where to store evidence.
   - Update troubleshooting notes for agent spawn issues, evidence mismatches, and error scenarios; capture evidence updates for SPEC tracker.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| All four agents spawn reliably | Run `/spec-auto <SPEC>` (or `/spec-ops-plan`) in TUI with transcripts archived | TUI transcript + agent roster note |
| Stage tests run independently | Execute `/spec-ops-<stage> SPEC-KIT-045-mini` for each stage with fixture docs | Evidence logs + acceptance checklist |
| File-writing behavior verified | Compare generated telemetry/logs vs documented manifest checklist | Operator checklist + evidence diff notes |
| Error handling covered | Trigger documented failure scenarios (env flags/fixture tweaks) via TUI commands | Halt transcript + remediation log |
| Fixtures stay minimal & fast | Recorded fixture size check & checksum note in doc appendix | Fixture report (<100 KB) |
| Acceptance criteria documented per stage | `docs/spec-kit/systematic-testing-guide.md` | Stage success matrices |

## Risks & Unknowns
- GPT Pro/Codex access may remain unstable; keep mock fallbacks but block final sign-off until live spawns pass.
- Telemetry schema drift can invalidate fixtures; tie fixtures to shared helpers and rerun checks when schema changes.
- Evidence directories may grow past size budget; monitor with manifest script and prune fixtures aggressively.
- Error cases might rely on HAL availability; default to mocked responses in tests and document optional live checks using HAL flags.
- Landlock sandbox limitations can block rehearsal commands; remind operators to run the TUI with `--sandbox danger-full-access` when using the mini bundle.

## Consensus & Risks (Multi-AI)
- Gemini 2.5 Pro and Claude 4.5 Sonnet agreed on the lean, fixture-first approach with agent spawn remediation and stage-by-stage validation; GPT Pro/GPT Codex alignment pending until access is restored and checked via task 1.

## Exit Criteria (Done)
- Agent spawn rehearsal (via `/spec-auto` or `/spec-ops-plan`) confirms all four models and records evidence.
- Each stage playbook executes via TUI without running the full 90-minute pipeline.
- File-writing checklist shows expected telemetry/log outputs for success and failure scenarios.
- Error scenarios produce documented halt messages and evidence bundles.
- Fixture check reports footprint under 100 KB and regeneration instructions published.
- Fast-run guide and troubleshooting notes available, with SPEC tracker updated using fresh evidence links.
