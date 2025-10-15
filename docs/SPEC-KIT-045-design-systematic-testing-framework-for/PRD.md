# PRD: Systematic Testing Framework for Spec-Auto Orchestrator (SPEC-KIT-045-design-systematic-testing-framework-for)

## Objective
Deliver a deterministic, stage-focused testing framework that validates the Spec-Auto orchestrator instructions for plan, tasks, implement, validate, audit, and unlock without relying on full 90-minute end-to-end runs. The framework must confirm telemetry, evidence, and agent orchestration correctness while keeping fixtures lightweight for rapid iteration.

## Problem
The current Spec-Auto verification loop depends on long manual runs and ad-hoc inspection. Small regressions in guardrail scripts, consensus orchestration, agent metadata, or file outputs can slip through until late validation, slowing operator feedback and eroding confidence. Existing fixtures are heavy, scattered, and slow to regenerate, making it costly to reproduce issues.

## Goals
- Provide sub-10-minute automation that exercises each Spec-Auto stage independently with deterministic pass and failure cases.
- Validate evidence generation: guardrail artifacts, consensus synthesis JSON, telemetry schema v1 payloads, and SPEC.md/task updates where applicable.
- Confirm parallel agent spawning across Gemini, Claude, GPT Pro, and GPT Codex configurations, including degraded and retry paths from `docs/spec-kit/model-strategy.md`.
- Capture predictable error handling for guardrail exits, missing telemetry, agent unavailability, and consensus deadlocks.
- Deliver a reusable fixture library (<100 KB total) that can be regenerated or refreshed via documented scripts.

## Scope
- Harnesses to execute stage guardrail scripts and orchestrator entry points with synthetic SPEC IDs.
- Mocked agent outputs (success, degraded, conflict) aligned with current prompt metadata fields (`model`, `model_release`, `reasoning_mode`, `consensus`).
- Schema validation tooling for telemetry JSON, synthesis bundles, and evidence directories.
- Documentation and runbooks describing setup, execution, evidence capture, and failure triage.
- Integration with existing guardrail helpers (`scripts/spec_ops_004/common.sh`, telemetry validators) and Codex CLI tests (`codex-rs/tui`).

## Non-Goals
- Replacing the orchestrator or guardrail implementations.
- Introducing new language models or modifying the canonical agent lineup.
- Delivering long-duration load testing or cost telemetry dashboards.
- Automating HAL live calls beyond optional smoke hooks triggered via environment flags.

## Stakeholders
- Spec Kit maintainers responsible for guardrails, telemetry, and orchestrator prompts.
- Codex CLI engineers maintaining `chatwidget.rs`, stage state machines, and slash commands.
- QA and release engineering teams who rely on reproducible evidence bundles.
- Agent operations managing model availability and fallback policies.
- Documentation owners updating SPEC.md, CLAUDE.md, AGENTS.md, and related runbooks.

## Requirements
### Functional
- Provide stage-specific runners that call `scripts/spec_ops_004/commands/spec_ops_<stage>.sh` and capture exit codes, stdout/stderr, and evidence paths.
- Generate synthetic consensus artifacts for each stage (per-agent outputs, synthesis JSON, telemetry JSONL) and validate required metadata fields.
- Verify agent orchestration by asserting the presence of Gemini, Claude, GPT Pro, and GPT Codex outputs per stage, including arbiter resolutions.
- Support configurable error injections (guardrail failure, missing synthesis artifact, agent dropout, telemetry schema violation) with assertions on halt behaviour and messaging.
- Implement resumable checks triggered by `--from <stage>` flags to validate partial pipelines.
- Produce machine-readable summaries (JSON) of each run including stage status, evidence directories, and notable regressions.
- Expose CLI entrypoints and Rust integration tests so workflows can run locally, in pre-commit hooks, and in CI.

### Non-Functional
- Full suite runtime ≤10 minutes on developer hardware and CI default configuration.
- Default execution requires no external network calls; HAL smoke runs only when `SPEC_OPS_TELEMETRY_HAL=1` and credentials are present.
- Tests execute in isolated temp directories and never modify real evidence paths without explicit flag.
- Fixtures remain below 100 KB across all files and are reproducible via scripted generation.
- Schema validation reuses existing utilities (`scripts/spec-kit/lint_tasks.py`, consensus validators) to avoid duplicate implementations.

## Acceptance Criteria
- `cd codex-rs && cargo test -p codex-tui spec_auto::systematic` exercises all six stages using fixtures, asserting guardrail exits, telemetry schema compliance, consensus metadata, and agent lineups.
- `scripts/spec_ops_004/spec_auto.sh SPEC-KIT-045-mini --consensus --dry-run` emits deterministic evidence that passes automated validation scripts.
- Error-injection suites demonstrate controlled halting for guardrail failure, missing telemetry, and agent unavailability with evidence diffs logged to `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-045-design-systematic-testing-framework-for/`.
- Documentation updates instruct maintainers how to refresh fixtures, run the suite locally/CI, and upload evidence snapshots to SPEC tracker notes.
- CI integration (or documented manual workflow) fails builds when telemetry or consensus artifacts drift from expected schema or structure.

## Test Strategy
- **Unit tests** validate schema helpers, fixture generation, and agent metadata parsing inside `codex-rs/tui` and supporting crates.
- **Integration tests** invoke stage runners with mocked agents and guardrail scripts, validating success, degraded, and failure paths.
- **Smoke scripts** under `scripts/spec_ops_004/` replay stage runs end-to-end using fixture SPEC IDs, emitting machine-readable summaries.
- **Telemetry checks** use serde or JSON schema validation to confirm required fields (command, specId, sessionId, stage-specific keys) and optional `hal.summary` blocks when HAL hooks execute.
- **File diffing** compares generated evidence directories against expected manifests, normalizing timestamps to avoid false positives.
- **Reporting** outputs a summary JSON and human-readable log enumerating stage outcomes, agent counts, and evidence paths for attachment in SPEC.md notes.

### Stage Coverage Highlights
- **Plan**: baseline audit behaviour, policy pass/fail toggles, HAL stub invocation, synthesis agreements/conflicts, SPEC.md linkage placeholders.
- **Tasks**: SPEC.md task patch generation, consensus agreement/dissent capture, tool.status telemetry checks.
- **Implement**: diff artifact registration, hook status reporting, lock enforcement, retry budget coverage for degraded consensus.
- **Validate**: verification that required commands (`cargo fmt`, `cargo clippy`, targeted tests) are logged, HAL outcomes propagate to telemetry, and failure gates halt pipeline.
- **Audit**: evidence completeness, claim-to-source coverage metadata, degraded vs healthy audit telemetry.
- **Unlock**: unlock_status assertion, final evidence summary, tracker update cues.

## Fixtures
- Synthetic SPEC directory (`docs/SPEC-KIT-045-mini/`) containing trimmed constitution excerpts, minimal spec/plan/tasks placeholders, and sample telemetry bundles.
- Mock agent response files for each model and stage covering success, degraded, conflict, and timeout branches.
- Fixture regeneration guidance (manual today; future xtask optional) to rebuild archives and enforce size limits.
- Telemetry baseline JSON templates capturing schema v1 fields with optional HAL sections.

## Risks and Mitigations
- **Telemetry schema drift**: Couple tests to shared schema helpers and update fixtures alongside schema revisions; add CI guard to ensure validators and fixtures update together.
- **Fixture staleness**: Provide regeneration script and document expected checksum/size so drift is detectable.
- **Runtime creep**: Track suite duration in CI and fail when exceeding thresholds; keep fixtures minimal and support parallel execution where safe.
- **Mock divergence from production agents**: Periodically replay one real orchestrator run (with overrides) and diff outputs to refresh fixtures.
- **Evidence directory collisions**: Always run tests in temp directories and clean up after execution; include safeguards against writing into live evidence trees.
- **Optional HAL dependencies**: Gate HAL-specific tests behind environment flags and provide mock responses to keep default runs self-contained.

## Dependencies
- `scripts/spec_ops_004/commands/*` guardrail scripts and shared helpers (`common.sh`).
- Codex CLI orchestrator integration within `codex-rs/tui/src/chatwidget.rs` and `slash_command.rs`.
- Prompt metadata defined in `docs/spec-kit/prompts.json` and model lineup in `docs/spec-kit/model-strategy.md`.
- Telemetry schema documentation (`docs/SPEC-KIT-013-telemetry-schema-guard/spec.md`, `docs/spec-kit/telemetry-schema-v2.md`).
- Existing validation utilities (`scripts/spec-kit/lint_tasks.py`, consensus parsers, HAL mock helpers`).

## Open Questions
- Should HAL smoke coverage run automatically in CI when credentials are configured, or remain developer-triggered with evidence upload?
- Do we persist summarised run metadata into local-memory after each automated suite to support `/spec-auto` resumability?
- How should evidence diffs be surfaced to operators (CI artifact viewer, TUI notifications, or SPEC.md notes)?
- Is additional tooling needed to replay historical evidence bundles against new validators to detect regressions retroactively?
