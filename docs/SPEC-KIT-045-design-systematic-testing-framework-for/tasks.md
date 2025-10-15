# Tasks: SPEC-KIT-045 Systematic Testing Framework (T49)
## Inputs
- Spec: docs/SPEC-KIT-045-design-systematic-testing-framework-for/spec.md (git 5bb15d6500d5b0ebb37ff8be4da60520cd2c3fd5)
- Plan: docs/SPEC-KIT-045-design-systematic-testing-framework-for/plan.md (2025-10-11 manual synthesis)
- Constitution & product scope: memory/constitution.md, product-requirements.md, PLANNING.md

## Task Slices (2025-10-11)

- **Validation**:
  1. From the TUI, run `/spec-auto <SPEC-ID> --from plan` (or `/spec-ops-plan <SPEC-ID>`) — HAL defaults to mock.
  2. Confirm all four agents (Gemini, Claude, GPT Pro, GPT Codex) appear in the transcript and capture the log path under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/`.
- **Evidence**: TUI transcript showing model roster, updated config diff, local-memory note on remediation.
- **Docs**: Update `docs/spec-kit/model-strategy.md` appendix with confirmed model IDs and escalation guidance.
- **Risks**: GPT Pro/Codex service availability; record fallback procedure if outage persists.

- **Validation**:
  1. Copy the fixture folder under `docs/SPEC-KIT-045-mini/` (or edit existing docs/spec) keeping total size <100 KB (`du -sh docs/SPEC-KIT-045-mini`). *(Seed bundle created 2025-10-12 at 36 KB with checksums in `docs/SPEC-KIT-045-mini/checksums.sha256`.)*
  2. Record checksums of key files (telemetry snapshots, mock agent outputs) in the fixture manifest.
- **Evidence**: Fixture manifest with checksums, size report, generated sample artifacts.
- **Docs**: Add regeneration instructions to `docs/spec-kit/systematic-testing-guide.md`.
- **Risks**: Fixture rot when prompts or schemas change; schedule refresh cadence.

- **Goal**: Define playbooks for running `/spec-ops-<stage>` against fixtures from the TUI, asserting acceptance criteria and evidence writes without the full pipeline.
- **Dependencies**: Tasks 1–2, guardrail scripts in `scripts/spec_ops_004/commands/`.
- **Validation**:
  1. From the TUI, run `/spec-ops-plan SPEC-KIT-045-mini` (mock HAL) and capture log + telemetry paths. Launch the TUI with `--sandbox danger-full-access` to avoid Landlock panics when listing evidence.
  2. Repeat for tasks, implement, validate, audit, unlock. For resumable flows, run `/spec-auto SPEC-KIT-045-mini --from <stage>` and document behaviour.
- **Evidence**: Stage-specific log files, acceptance checklist stored in docs, annotated transcripts.
- **Docs**: Update `docs/spec-kit/systematic-testing-guide.md` with detailed stage instructions.
- **Risks**: `--from <stage>` resume behaviour may diverge; include regression test for resumptions.

- **Goal**: Build manual manifest/telemetry checklists confirming guardrail outputs (telemetry JSON, SPEC.md/task patches, evidence directories) match expectations.
- **Dependencies**: Task 3 outputs; telemetry schema helpers.
- **Validation**:
  1. After each stage playbook run, review the generated telemetry under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC>/`.
  2. Update the manifest checklist (paths, filename patterns, required fields) and record any deviations.
- **Evidence**: Completed manifest checklist, screenshots or logs highlighting mismatches and resolutions.
- **Docs**: Append manifest expectations to troubleshooting guide, note timestamp normalization approach.
- **Risks**: Timestamp variance causing false positives; provide regex/placeholder guidance in the checklist.

- **Goal**: Simulate guardrail failures, missing telemetry, agent dropouts, and file-write errors with actionable halt messages.
- **Dependencies**: Tasks 1–4.
- **Validation**:
  1. Re-run stage playbooks with environment overrides (e.g., `SPEC_OPS_BASELINE_FORCE_FAIL=1` for plan, removing telemetry file for validate) using the TUI.
  2. Capture halt evidence (logs, telemetry, transcript snippets) and document recommended remediation steps.
- **Evidence**: Error scenario logs, telemetry snippets demonstrating halt conditions, documented remediation steps.
- **Docs**: Update troubleshooting section with error matrices and recovery playbook.
- **Risks**: HAL outages masking other failures; default HAL mode is mocked (no network). Document how to flip to live by adding `--hal live` to the relevant `/spec-ops-*` or `/spec-auto` command.

- **Goal**: Document the sub-90-minute workflow, update SPEC tracker, and wire commands into CI.
- **Dependencies**: Tasks 1–5 complete with evidence.
- **Validation**:
  1. Execute the full TUI rehearsal (`/spec-ops-plan|tasks|implement|validate|audit|unlock`) with fixture SPEC and mock HAL.
  2. Update docs, run `scripts/doc-structure-validate.sh --mode=templates` and `python3 scripts/spec-kit/lint_tasks.py`, then record the command list in SPEC.md notes.
- **Evidence**: Consolidated log bundle, CI plan snippet (documented steps), dated SPEC.md note with command list.
- **Docs**: Finalise `docs/spec-kit/systematic-testing-guide.md` + troubleshooting companion; add SPEC.md evidence note referencing logs.
- **Risks**: CI runtime creeping past 10 minutes; enforce timeout and monitor summary output.

## Acceptance Coverage Mapping
- Agent spawn remediation → Requirement: validate and fix agent spawning (Gemini/Claude/GPT Pro/GPT Codex).
- Fixture kit + stage harnesses → Requirement: minimal fixtures + stage-independent testing (plan/tasks/implement/validate/audit/unlock).
- File-writing & telemetry checks → Requirement: verify file writing behaviour and stage success criteria.
- Error suite → Requirement: test error handling paths.
- Documentation slice → Requirement: document methodology without 90-minute runs and capture acceptance criteria per stage.

## Outstanding Questions
- Do we need live-agent verification in CI or keep it as manual smoke? Decide before Task 6.
- What cadence is acceptable for refreshing fixtures when model prompts update?
- Should HAL smoke be mandatory for validate/audit stages or remain opt-in behind `SPEC_OPS_TELEMETRY_HAL`?
