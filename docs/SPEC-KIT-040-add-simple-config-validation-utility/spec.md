# Spec: Simple Config Validation Utility (SPEC-KIT-040-add-simple-config-validation-utility)

## Context
- Codex CLI currently detects invalid configuration values late in execution, disrupting guardrail automation and confusing operators.
- The Rust workspace already centralises config types under `codex-rs/core/src/config.rs` and related modules, providing a foundation for reusable validation logic.

## Objectives
- Introduce a reusable validator that can be invoked both as a standalone CLI command and during startup to catch structural, enum, file path, and environment reference issues early.
- Ensure validator outcomes integrate with existing guardrail telemetry and present actionable remediation guidance in both TUI and headless flows.

## Acceptance Criteria
- Running `codex config validate` against default configs exits zero with a success summary.
- Invalid values, missing files, or unresolved environment keys produce descriptive diagnostics and appropriate exit codes/severity.
- `/spec-auto SPEC-KIT-040` records validator telemetry compliant with schema v1.
- Documentation updates describing validator usage pass doc-structure and tracker lint checks.
