# Plan: Simple Config Validation Utility (SPEC-KIT-040-add-simple-config-validation-utility)
## Inputs
- Spec: docs/SPEC-KIT-040-add-simple-config-validation-utility/spec.md (git 2b85a37592a4b936a58dc8e7bad97650b6073ae5)
- Constitution: memory/constitution.md (git 4e159c7eccd2cba0114315e385584abc0106834c)

## Work Breakdown
1. Scaffold reusable validator module inside `codex-rs/core`, reusing existing config types and wiring placeholder API able to load default config; add smoke tests for happy-path validation.
2. Implement rule set covering structural, enum/range, filesystem, and environment checks with deterministic fixtures plus developer-facing helper functions for reuse across CLI/TUI layers.
3. Expose validator via `codex config validate` CLI/TUI surfaces with flags (`--config-path`, `--profile`, `--json`, `--strict`, `--skip-path-checks`) and wire optional startup warning hook toggled by `CODEX_CONFIG_STRICT`.
4. Add CLI integration and core unit tests for clean, warning, and strict failure scenarios, ensuring fixtures under `codex-rs/cli/tests` and `codex-rs/core/tests` remain deterministic and CI-safe.
5. Integrate validator into guardrail telemetry by invoking it during `spec_ops_validate` execution, emitting schema v1 artifact `validator.json`, updating SPEC.md T48 status/evidence, and capturing tool.status metadata.
6. Update documentation (`docs/config.md`, `config.toml.example`, hook guidance) and rerun doc/tracker lint plus `/spec-ops-validate` smoke to confirm evidence; record summary in changelog.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| AC1: `codex config validate` exits zero on default config | `cargo test -p codex-cli --test config_validate_default` | `codex-rs/cli/tests/config_validate.rs` |
| AC2: Invalid values surface descriptive diagnostics | `cargo test -p codex-core config_validator::invalid_values` | `codex-rs/core/src/config_validator.rs` |
| AC3: Missing files/env keys produce actionable messages | `cargo test -p codex-cli --test config_validate_strict -- --ignored` | `codex-rs/cli/tests/config_validate.rs` |
| AC4: `/spec-auto SPEC-KIT-040` captures validator telemetry | `SPEC_OPS_ALLOW_DIRTY=0 scripts/spec_ops_004/commands/spec_ops_validate.sh SPEC-KIT-040-add-simple-config-validation-utility` | `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-040-add-simple-config-validation-utility/validator.json` |
| AC5: Documentation updates pass validation scripts | `scripts/doc-structure-validate.sh --mode=templates && python3 scripts/spec-kit/lint_tasks.py` | `docs/config.md`, `config.toml.example` |

## Risks & Unknowns
- Validator/runtime divergence causing false positives; mitigate by sharing parsing structures and adding regression coverage across core + CLI tests.
- Startup hook latency or unexpected failures; default to warning mode, allow `--skip-path-checks`, and gate strict failure with `CODEX_CONFIG_STRICT` to protect existing workflows.
- Telemetry schema mismatch or missing fields; validate JSON via `scripts/spec_ops_004/common.sh` helpers before committing evidence and capture schema version in artifact.

## Consensus & Risks (Multi-AI)
- Agreement: Gemini 2.5 Pro, Claude Sonnet 4.5, and GPT-5 Codex concur on core module approach, six-step breakdown, telemetry artifact location, and coverage plan for AC1–AC5.
- Disagreement & resolution: Agents raised open questions on module placement, telemetry stage, and startup scope; resolved by keeping validator in `codex-rs/core`, running it during validate-stage guardrails with `validator.json`, and implementing warning-mode startup hook gated by `CODEX_CONFIG_STRICT`. Consensus flagged as degraded because gpt_pro/gpt_codex baseline agents were unavailable (substituted by GPT-5 Codex output).

## Exit Criteria (Done)
- All acceptance checks pass
- Docs updated (docs/config.md, config.toml.example, SPEC.md evidence note)
- Changelog/PR prepared
