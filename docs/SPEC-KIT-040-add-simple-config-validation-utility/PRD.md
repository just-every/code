# PRD: Simple Config Validation Utility (SPEC-KIT-040-add-simple-config-validation-utility)

## Problem Statement
- Codex CLI surfaces misconfigured values during runtime, often after expensive startup or guardrail execution, producing opaque stack traces and blocking `/spec-auto` runs.
- Operators and contributors lack a pre-flight mechanism to validate `config.toml`, profile overrides, and CLI `-c key=value` flags before invoking automation.
- Existing guardrail scripts (`scripts/spec-kit/lint_tasks.py`, `scripts/doc-structure-validate.sh`) do not cover configuration drift, allowing mismatches between documented options and the Rust data models in `codex-rs/core/src/config.rs`.

## Goals
- Deliver a fast config validation entry point that checks structure, enum bounds, file paths, and environment references before the CLI commits to workflows.
- Provide both a standalone command (`codex config validate …`) and an optional startup hook that can fail fast in strict mode while defaulting to non-blocking warnings.
- Emit telemetry compatible with SPEC-OPS schema v1 so guardrails can capture validator outcomes alongside baseline evidence.
- Present actionable remediation guidance with consistent UX across the TUI and headless modes.

## Requirements
- Implement reusable validation logic (likely `codex-rs/core/src/config_validator.rs`) that inspects TOML structure, serde model compatibility, enum/string bounds, numeric ranges, and cross-field constraints using existing `config.rs` and `config_types.rs` types.
- Verify filesystem references (MCP server commands, hook scripts, assets) and environment keys referenced in config, with an option to skip slow checks.
- Expose CLI surface area under a new `codex config validate` subcommand supporting `--config-path`, `--profile`, `--json`, `--strict`, and `--skip-path-checks` flags with deterministic exit codes (0 valid, 1 validation errors, 2 I/O/runtime issues).
- Hook validation into CLI startup in warning mode, with opt-in strict enforcement via environment variable or config flag (`CODEX_CONFIG_STRICT=1`).
- Record validator results in `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/…` telemetry artifacts (populated `tool.status`, optional `hal.summary`, model metadata per `docs/spec-kit/model-strategy.md`).
- Update documentation (`docs/config.md`, `config.toml.example`, new quick-start snippet) describing validator usage, strict mode, and integration into hooks or CI.
- Supply developer-facing APIs/tests so downstream SPEC automation can invoke validation helpers without duplicating logic.
- Optional but encouraged: surface guardrail wiring so pre-commit/pre-push and `/spec-ops-*` stages can run the validator; ensure configuration keeps existing workflows opt-in to avoid regressions.

## Non-Goals
- Refactoring the full configuration loading pipeline or introducing automatic fixes.
- Adding new configuration keys beyond what is necessary to enable the validator.
- Validating remote or server-side configuration sources.
- Enforcing new policy gates unrelated to configuration correctness.

## Scope
- Rust workspace updates within `codex-rs/core`, `codex-rs/cli`, and supporting crates (e.g., shared error types) plus documentation assets under `docs/`.
- Telemetry and guardrail touchpoints within `scripts/spec_ops_004/` and associated evidence directories.
- Test fixtures (sample TOML files, CLI integration harness) residing near existing configuration tests.
- UX polish in TUI/headless modes to surface validation summaries without blocking unrelated flows unless strict mode is active.

## Assumptions
- Configuration parsing continues to rely on serde/TOML and existing enums/structs.
- Local and CI environments can run Rust unit/integration tests and create temp files.
- No additional heavy dependencies are required; prefer standard library and crates already in the workspace.
- Telemetry directories remain the canonical evidence sink for guardrail runs.

## Acceptance Criteria
- AC1: `codex config validate` against the default `config.toml` exits zero and reports a clean summary (CLI integration test).
- AC2: Invalid enum/boolean values surface descriptive diagnostics listing allowed options and exit non-zero in strict mode (unit + CLI tests).
- AC3: Missing files or environment variables referenced in config produce actionable messages with severity levels respected by strict/warn modes (unit tests with mocked filesystem/env).
- AC4: `/spec-auto SPEC-KIT-040` captures a validator telemetry artifact that passes schema v1 checks and, when applicable, populates `hal.summary`.
- AC5: Documentation (`docs/config.md`, `config.toml.example`) references the validator and passes `scripts/doc-structure-validate.sh --mode=templates` and `python3 scripts/spec-kit/lint_tasks.py`.

## Risks
- Divergence between validator logic and runtime parsing may create false positives; mitigate by reusing shared data structures and adding regression tests.
- Startup latency could increase if validation repeats costly filesystem/network calls; keep checks efficient and cacheable.
- UX drift between TUI messaging and CLI output may confuse users; design shared formatting helpers.
- Telemetry schema mistakes could break guardrail evidence ingestion; validate artifacts against schema v1 before release.

## Validation Plan
- Unit tests covering individual validation rules (structural, enum bounds, cross-field constraints) using fixture TOML files.
- CLI integration tests exercising success, warning, strict failure, JSON output, and skip-path scenarios with temporary directories.
- Spec-Ops smoke run (`scripts/spec_ops_004/spec_auto.sh SPEC-KIT-040`) ensuring telemetry integration and guardrail visibility remain intact.
- Documentation validation via `scripts/doc-structure-validate.sh --mode=templates` and tracker lint `python3 scripts/spec-kit/lint_tasks.py` after doc updates.
- Manual TUI/headless UX review confirming warnings do not block workflows unless strict enforcement is enabled.
