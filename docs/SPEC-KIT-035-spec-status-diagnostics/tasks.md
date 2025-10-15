# Tasks: SPEC-KIT-035 spec-status diagnostics (T47)
## Inputs
- Spec: docs/SPEC-KIT-035-spec-status-diagnostics/spec.md (2025-10-08 synthesis)
- Plan: docs/SPEC-KIT-035-spec-status-diagnostics/plan.md (2025-10-08 refresh)
- Constitution & product scope: memory/constitution.md, product-requirements.md

## Task Slices (Consensus 2025-10-08)

### 1. Telemetry Aggregator & Staleness Guard
- **Goal**: Implement `codex-rs/tui/src/spec_status.rs` to load the latest guardrail/consensus telemetry (schema v1/v2), compute stage health, and flag stale snapshots (24 h default, configurable).
- **Dependencies**: `docs/spec-kit/telemetry-schema-v2.md`, `scripts/spec_ops_004/common.sh`, evidence roots under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`.
- **Validation**: `cargo test -p codex-tui spec_status::tests::parse_*`; targeted unit test for stale detection.
- **Evidence**: Parser logs, fixture outputs (healthy vs stale), local-memory note covering schema handling.
- **Docs**: Add architecture note to `docs/spec-kit/spec-status-diagnostics.md` explaining data sources and freshness rule.

### 2. Evidence Footprint Sentinel
- **Goal**: Calculate combined evidence footprint, raise ⚠ (≥20 MB) and ❌ (≥25 MB) banners, and list top directories without affecting exit codes.
- **Dependencies**: Filesystem metadata via Rust stdlib, `/spec-evidence-stats`, footprint policy in product requirements.
- **Validation**: `cargo test -p codex-tui spec_status::tests::footprint_thresholds`; dry-run `scripts/spec_ops_004/baseline_audit.sh --check-footprint SPEC-KIT-DEMO`.
- **Evidence**: Warning/critical dashboard screenshots, oversized fixture manifest (`tests/fixtures/spec_status/oversized_evidence/`).
- **Docs**: Expand footprint guidance in `docs/spec-kit/spec-status-diagnostics.md` and cross-link to cleanup procedures.

### 3. TUI Dashboard Rendering
- **Goal**: Wire `/spec-status <SPEC-ID>` through `slash_command.rs` and `chatwidget.rs`, rendering packet health, stage table, blockers, agent mix, and footprint alerts with 80×24 fallback support.
- **Dependencies**: Task slices 1–2, TUI renderer components, slash command registry.
- **Validation**: `cargo test -p codex-tui spec_status_integration`; manual `/spec-status SPEC-KIT-DEMO` and `/spec-status SPEC-KIT-035` runs.
- **Evidence**: Screenshots (healthy, consensus conflict, stale telemetry) stored under `docs/SPEC-KIT-035-spec-status-diagnostics/evidence/screenshots/`.
- **Docs**: Update `docs/slash-commands.md` with syntax and sample output; include annotated dashboard in operator guide.

### 4. CLI Fallback JSON Parity
- **Goal**: Update `scripts/spec_ops_004/commands/spec_ops_status.sh` to emit schemaVersion 1.1 JSON while preserving human-readable output and logging degraded mode when Rust ingestion fails.
- **Dependencies**: Existing bash helpers in `scripts/spec_ops_004/common.sh`, jq availability documentation.
- **Validation**: `bash scripts/spec_ops_004/commands/spec_ops_status.sh --spec SPEC-KIT-DEMO --format json | jq '.schemaVersion'`; `shellcheck scripts/spec_ops_004/commands/spec_ops_status.sh`.
- **Evidence**: JSON sample and parity diff stored under `docs/SPEC-KIT-035-spec-status-diagnostics/evidence/cli/`.
- **Docs**: Document CLI options in `docs/slash-commands.md`; add JSON schema excerpt to operator guide.

### 5. HAL & Policy Messaging
- **Goal**: Surface HAL/policy states with remediation hints (`SPEC_OPS_TELEMETRY_HAL=1`, `/spec-ops-validate`) while defaulting to warning-only messaging when telemetry is skipped.
- **Dependencies**: HAL telemetry artifacts when available, policy results emitted by guardrail scripts, credentials (`HAL_SECRET_KAVEDARR_API_KEY`).
- **Validation**: Dedicated fixtures (`hal_failed.json`, `policy_failed.json`), `cargo test -p codex-tui spec_status::tests::hal_policy_blockers`, manual HAL-enabled run.
- **Evidence**: Dashboard capture showing HAL failure vs skipped; log snippet documenting remediation guidance.
- **Docs**: Extend troubleshooting sections in operator guide and `docs/spec-kit/spec-status-diagnostics.md` with HAL/policy remediation steps.

### 6. Fixture Library & Regression Coverage
- **Goal**: Create fixtures covering healthy, stale, missing-consensus, missing-doc, oversized evidence scenarios and wire them into unit & integration tests.
- **Dependencies**: Aggregator + sentinel logic; fixture helpers (consider `scripts/spec_ops_004/spec_status_fixtures.sh`).
- **Validation**: `cargo test -p codex-tui spec_status_integration -- --nocapture`; ensure fixtures load deterministically.
- **Evidence**: Fixture manifest and test logs archived under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-035/fixtures/`.
- **Docs**: Add developer notes for fixtures in `docs/SPEC-KIT-035-spec-status-diagnostics/operator-guide.md` appendices.

### 7. Documentation & Operator Playbook
- **Goal**: Publish operator guidance, update slash-command reference, and embed screenshots illustrating success/failure cases.
- **Dependencies**: Completion of slices 3–5 for accurate screenshots.
- **Validation**: `scripts/doc-structure-validate.sh --mode=templates`; `python3 scripts/spec-kit/lint_tasks.py` (run dry-run first if needed).
- **Evidence**: Doc lint output, merged documentation diffs, stored screenshots.
- **Docs**: Deliver completed `docs/SPEC-KIT-035-spec-status-diagnostics/operator-guide.md`, refresh `docs/slash-commands.md`, and augment `docs/spec-kit/spec-status-diagnostics.md`.

### 8. Release Validation & Evidence Ledger
- **Goal**: Execute fmt/clippy/test suite, demonstrate `/spec-status` (TUI + CLI), run footprint audit, and update SPEC.md tracker with dated evidence references.
- **Dependencies**: All prior slices, clean branch `feat/spec-auto-telemetry`.
- **Validation**:
  ```bash
  scripts/env_run.sh cargo fmt --all -- --check
  scripts/env_run.sh cargo clippy --workspace --all-targets --all-features -- -D warnings
  scripts/env_run.sh cargo test -p codex-tui spec_status::tests::*
  scripts/env_run.sh cargo test -p codex-tui spec_status_integration -- --nocapture
  scripts/spec-kit/lint_tasks.py
  scripts/doc-structure-validate.sh --mode=templates
  scripts/spec_ops_004/evidence_stats.sh --spec SPEC-KIT-035
  ```
- **Evidence**: Validation logs, TUI/CLI captures, footprint report archived in `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-035/validation/`.
- **Docs**: Update SPEC.md row for T47 (status notes, evidence references) and capture summary in release notes.

## Unresolved Risks
- Telemetry schema drift beyond v2 will require new fixtures and parser extensions; schedule periodic schema audits.
- Large evidence trees may impact dashboard latency—monitor runtime and consider caching if responses exceed 2 s.
- Evaluate configurability of evidence thresholds on a per-SPEC basis during release validation.
