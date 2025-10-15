# Plan: T13 Telemetry Schema Guard
## Inputs
- Spec: docs/SPEC-KIT-013-telemetry-schema-guard/spec.md (48f33923)
- Constitution: memory/constitution.md (not present in repo — pending restoration; using latest known template)

## Work Breakdown
1. Codify stage-aware telemetry schema in Rust (`codex-rs/tui/src/chatwidget.rs`), including reusable validators and error aggregation.
2. Update guardrail outcome handling to enforce schema prior to artifact checks; surface failures in `/spec-auto` history with actionable messages.
3. Extend unit tests for each stage (Plan/Tasks/Implement/Validate/Audit/Unlock) covering valid and malformed telemetry scenarios.
4. Document schema tables and troubleshooting guidance (spec.md already drafted) and wire future maintainers via TODOs/comments.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Schema validation halts malformed telemetry | `cargo test -p codex-tui spec_auto::schema_guard` (new module) | codex-rs/tui/src/chatwidget.rs::spec_auto_plan_schema_validation_fails_without_baseline |
| R2: Common metadata enforced | Unit test ensuring missing `specId` fails | codex-rs/tui/src/chatwidget.rs::spec_auto_common_metadata_required |
| R3: Stage payload requirements enforced | Stage-specific tests per guardrail | codex-rs/tui/src/chatwidget.rs::{spec_auto_tasks_schema_requires_status,spec_auto_implement_schema_requires_lock_and_hook,spec_auto_validate_schema_detects_bad_scenarios,spec_auto_unlock_schema_requires_status} |
| R4: Clear error messaging | Unit test asserts failure strings contain field + reason | codex-rs/tui/src/chatwidget.rs::spec_auto_audit_schema_rejects_invalid_status_values |
| R5: Documentation present | Review `spec.md` update & inline code comments | docs/SPEC-KIT-013-telemetry-schema-guard/spec.md |

## Risks & Unknowns
- Legacy telemetry files may lack newly required fields; decide whether to grandfather old artifacts or force re-run (leaning strict fail with guidance).
- Guardrail shells currently emit minimal payloads; may require coordinated updates if validation exposes gaps.
- Potential performance impact from repeated validation is expected to be negligible but should be observed in integration tests.

## Consensus & Risks (Multi-AI)
- Agreement: Solo Codex analysis; external agents (Claude/Gemini/Qwen) unavailable in this environment. Adopted conservative schema aligning with existing Rust evaluation logic.
- Disagreement & resolution: None (degraded multi-agent mode; document necessity to rerun with full stack if available).

## Exit Criteria (Done)
- All acceptance checks pass
- Docs updated (PRD/spec already authored)
- Changelog/PR prepared
