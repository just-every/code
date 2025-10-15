# Tasks: SPEC-KIT-040 Simple Config Validation Utility (T48)
## Inputs
- Spec: docs/SPEC-KIT-040-add-simple-config-validation-utility/spec.md (git 2b85a37592a4b936a58dc8e7bad97650b6073ae5)
- Plan: docs/SPEC-KIT-040-add-simple-config-validation-utility/plan.md (2025-10-11 consensus)
- Constitution & product scope: memory/constitution.md, product-requirements.md, PLANNING.md

## Task Slices (Consensus 2025-10-11)

### 1. Validator Core Module & Error Model
- **Goal**: Create `codex-rs/core/src/config_validator.rs` providing reusable `validate(Config, Options) -> ValidationReport` API sharing existing `config.rs` data types.
- **Dependencies**: `codex-rs/core/src/config.rs`, serde TOML loaders, `anyhow`/`thiserror` stack, plan step 1.
- **Validation**: `cargo test -p codex-core config_validator::smoke`; targeted unit tests for happy-path config.
- **Evidence**: Unit test output, API docs comment describing report schema, local-memory entry capturing rule coverage.
- **Docs**: Developer note in `docs/spec-kit/implementation-notes.md` (or equivalent) summarising validator interface for downstream use.

### 2. Rule Coverage & Deterministic Fixtures
- **Goal**: Implement structural, enum/range, filesystem, and environment validations with deterministic fixtures.
- **Dependencies**: Slice 1 API, fixture directory `codex-rs/core/tests/fixtures/config_validation/`, env/path helpers.
- **Validation**: `cargo test -p codex-core config_validator::invalid_values`, `cargo test -p codex-core config_validator::missing_assets`.
- **Evidence**: Fixture manifest, regression logs proving enum/flag errors surface with actionable messages; snapshot diff for warnings vs errors.
- **Docs**: Append validation matrix to `docs/config.md` showing rule coverage.

### 3. CLI & Startup Surfaces
- **Goal**: Expose `codex config validate` command with JSON/strict/skip-path flags and warning-mode startup hook toggled by `CODEX_CONFIG_STRICT`.
- **Dependencies**: `codex-rs/cli/src/main.rs`, new module `codex-rs/cli/src/commands/config.rs`, TUI slash command wiring, plan step 3.
- **Validation**: `cargo test -p codex-cli config_validate_default`, `cargo test -p codex-cli config_validate_strict -- --nocapture`, manual startup run with and without `CODEX_CONFIG_STRICT`.
- **Evidence**: CLI snapshots (success, warning, strict failure), exit-code matrix, local-memory UX note.
- **Docs**: Update `docs/slash-commands.md` and `docs/config.md` with usage examples and environment toggles.

### 4. Guardrail & Telemetry Integration
- **Goal**: Invoke validator during `spec_ops_validate` to emit schema v1 artifact `validator.json` with `tool.status` and optional `hal.summary`.
- **Dependencies**: `scripts/spec_ops_004/commands/spec_ops_validate.sh`, `scripts/spec_ops_004/common.sh`, evidence directory `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-040-add-simple-config-validation-utility/`.
- **Validation**: `SPEC_OPS_TELEMETRY_HAL=1 scripts/spec_ops_004/spec_ops_validate.sh SPEC-KIT-040-add-simple-config-validation-utility`, `scripts/spec_ops_004/evidence_stats.sh --spec SPEC-KIT-040`.
- **Evidence**: `validator.json` artifact with schema metadata, telemetry log snippet, HAL summary capture when enabled.
- **Docs**: Add validator telemetry reference to `docs/spec-kit/model-strategy.md` appendix and guardrail runbook.

### 5. Documentation, Examples, and Tracker Hygiene
- **Goal**: Refresh operator docs, config examples, and SPEC.md evidence entry while keeping single In Progress task per constitution.
- **Dependencies**: Prior slices for accurate instructions, `config.toml.example`, SPEC.md row T48.
- **Validation**: `scripts/doc-structure-validate.sh --mode=templates`, `python3 scripts/spec-kit/lint_tasks.py` (dry-run first if needed), manual doc review.
- **Evidence**: Doc validation logs, diffs summarised in changelog, SPEC.md note referencing validator artifacts.
- **Docs**: Update `docs/config.md`, `config.toml.example`, changelog entry, and ensure tasks.md cross-links to plan & spec.

### 6. Release Validation & Evidence Ledger
- **Goal**: Run workspace fmt/clippy/tests, perform `/spec-auto SPEC-KIT-040` smoke, archive logs, and prepare final PR body with acceptance mapping.
- **Dependencies**: Clean branch `feat/spec-auto-telemetry`, env secrets for HAL when available, slices 1–5 complete.
- **Validation**:
  ```bash
  cd codex-rs
  cargo fmt --all
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo test --workspace
  cd ..
  scripts/spec_ops_004/spec_auto.sh SPEC-KIT-040-add-simple-config-validation-utility
  scripts/spec_ops_004/evidence_stats.sh --spec SPEC-KIT-040-add-simple-config-validation-utility
  ```
- **Evidence**: Validation logs, spec-auto telemetry bundle, HAL summary (if run), dated SPEC.md note with tests executed.
- **Docs**: Add release note snippet and ensure PR body includes Acceptance Mapping referencing tests above.

## Acceptance Coverage
- **AC1**: Slices 1, 3, and 6 via CLI happy-path tests (`cargo test -p codex-cli config_validate_default`).
- **AC2**: Slice 2 ensures descriptive diagnostics for invalid enums/booleans with regression fixtures.
- **AC3**: Slice 2 + 3 cover missing files/env keys and strict-mode exit handling.
- **AC4**: Slice 4 runs validator inside `spec_ops_validate` and captures schema v1 telemetry artifact `validator.json` for `/spec-auto`.
- **AC5**: Slice 5 executes doc + tracker lint and documents validator usage in `docs/config.md` & `config.toml.example`.

## Command Plan & Checks
- `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test -p codex-core config_validator::*`
- `cargo test -p codex-cli config_validate_*`
- `SPEC_OPS_TELEMETRY_HAL=1 scripts/spec_ops_004/spec_auto.sh SPEC-KIT-040-add-simple-config-validation-utility`
- `scripts/doc-structure-validate.sh --mode=templates`
- `python3 scripts/spec-kit/lint_tasks.py`

## Consensus Notes (Multi-AI)
- Gemini 2.5 Pro, Claude Sonnet 4.5, and GPT-5 Codex aligned on six-slice breakdown, evidence commands, and telemetry artifact path.
- Disagreements resolved by: retaining validator in core crate (not new crate), running guardrail invocation during validate stage, and keeping startup hook warning-only with `CODEX_CONFIG_STRICT` opt-in.
- Consensus degraded: gpt_pro and gpt_codex baseline agents unavailable; GPT-5 Codex substitution recorded and outputs stored under evidence/consensus/spec-tasks.

## Follow-ups & Risks
- Monitor validator/runtime parity; add regression test when config schema evolves.
- Ensure HAL secrets available before telemetry-enabled runs; otherwise document skipped HAL summary in artifact.
- Confirm evidence footprint stays within policy (≤25 MB) after new telemetry artifacts land.
