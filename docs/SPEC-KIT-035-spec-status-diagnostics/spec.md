# Spec: SPEC-KIT-035 spec-status diagnostics
## Inputs
- PRD: docs/SPEC-KIT-035-spec-status-diagnostics/PRD.md (2025-10-07)
- Plan: docs/SPEC-KIT-035-spec-status-diagnostics/plan.md (2025-10-07)
- Constitution: memory/constitution.md (version 1.1, hash 4e159c7eccd2cba0114315e385584abc0106834c)

## Work Breakdown
1. **T47.1 – Telemetry ingestion & data model.** Create `codex-rs/tui/src/spec_status.rs` (or equivalent module) defining `SpecStatusReport`, `StageHealth`, `EvidenceMetrics`, and parsers that load guardrail, policy, HAL, and consensus telemetry (schema v1/v2). Gracefully classify missing HAL/policy blocks as `skipped` with reason strings. Add helpers to enumerate agent artifacts and detect stale timestamps.
2. **T47.2 – Evidence footprint & checksum sentinel.** Extend ingestion to total evidence directory sizes and verify artifact presence/optional checksum manifests. Emit warning banners (without altering exit codes) at ≥20 MB and critical alerts at ≥25 MB, referencing `/spec-evidence-stats`. Share helpers with `scripts/spec_ops_004/common.sh` where feasible to avoid drift.
3. **T47.3 – CLI fallback parity.** Upgrade `scripts/spec_ops_004/commands/spec_ops_status.sh` to emit structured JSON (schemaVersion 1.1) mirroring the TUI aggregation, preserving human-readable tables. Ensure fallback automatically triggers when Rust ingestion errors and annotate degraded mode in output.
4. **T47.4 – TUI dashboard rendering.** Wire `/spec-status <SPEC-ID>` through `codex-rs/tui/src/slash_command.rs` and `chatwidget.rs`, rendering a markdown-style dashboard (core packet health, stage table, blockers, evidence footprint, recent agents). Provide expandable sections or compact summaries that respect 80×24 fallback while keeping 1024×768 layout polished.
5. **T47.5 – HAL & policy gap handling.** Default dashboard presentation assumes HAL collection is disabled unless telemetry exists; add defensive logic that highlights HAL failures (`hal.summary.status="failed"` with failed_checks list) and policy prefilter/final outcomes. Suggest remediation commands (`SPEC_OPS_TELEMETRY_HAL=1`, policy reruns) directly in blockers.
6. **T47.6 – Fixture library & regression tests.** Create fixtures under `codex-rs/tui/tests/fixtures/spec_status/` (healthy, HAL skipped, consensus conflict, oversized evidence) and author unit/integration tests (`cargo test -p codex-tui spec_status::*`, `spec_status_integration.rs`) asserting parsing, rendering, CLI parity, and warnings.
7. **T47.7 – Documentation & operator playbook.** Author `docs/SPEC-KIT-035-spec-status-diagnostics/operator-guide.md`, update `docs/slash-commands.md`, `docs/spec-kit/spec-status-diagnostics.md`, and `docs/SPEC-OPS-004-integrated-coder-hooks/baseline.md` with usage, troubleshooting, and evidence management guidance. Capture representative TUI screenshots and CLI samples as evidence.
8. **T47.8 – Release validation & telemetry evidence.** Run end-to-end validations (TUI smoke, CLI fallback, `scripts/spec_ops_004/baseline_audit.sh --check-footprint`) against SPEC-KIT-DEMO and new fixtures, storing logs under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-035/`. Confirm doc lint (`scripts/doc-structure-validate.sh --mode=templates`) and SPEC task lint (`python3 scripts/spec-kit/lint_tasks.py`).

## Policy Decisions (2025-10-07)
- Evidence footprint alerts remain warnings only; `/spec-status` and the CLI fallback do not change exit codes when thresholds are crossed.
- Dashboard surfaces only the latest telemetry snapshot per stage; historical retention and pruning are future scope.
- HAL diagnostics are disabled by default; the dashboard reports `HAL: skipped` unless telemetry is present (operators enable collection via `SPEC_OPS_TELEMETRY_HAL=1`).

## Acceptance Mapping
| Requirement (PRD) | Validation Step | Test / Evidence |
| --- | --- | --- |
| R1: Stage/guardrail/consensus visibility | `cargo test -p codex-tui spec_status::tests::stage_snapshot`; `/spec-status SPEC-KIT-DEMO` screenshot recorded | Unit & TUI evidence bundle |
| R2: Telemetry schema v1/v2 compatibility | Fixture replay via `scripts/spec_ops_004/run_fixture.sh` + `cargo test -p codex-tui spec_status::tests::parse_schema_v2`; CLI fallback JSON diff | Fixture set + golden outputs |
| R3: Execution timeline & evidence footprint | `spec_status_integration.rs` threshold test + `spec_ops_status.sh --json` verifying footprint fields | Integration test log, CLI sample |
| R4: Failure diagnostics & remediation hints | Fixture with malformed HAL/policy telemetry; ensure blockers list commands; manual TUI capture | TUI screenshot + blocker log |
| R5: Agent tracking summaries | Unit test covering agent artifact enumeration; TUI rendering includes counts + failure links | Test report + TUI screenshot |
| R6: CLI parity and degraded mode | `spec_ops_status.sh --spec SPEC-KIT-035 --json` vs TUI output diff stored in evidence | CLI/TUI parity markdown |
| R7: Fixture & regression coverage | `cargo test -p codex-tui spec_status_integration -- --nocapture`; fixtures stored under docs/SPEC-KIT-035-spec-status-diagnostics/fixtures/ | Test log + fixture manifest |
| R8: Documentation updates | `scripts/doc-structure-validate.sh --mode=templates`; peer review checklist for operator guide | Doc lint log + review ACK |

## Risks & Unknowns
- **Telemetry drift:** Schema changes beyond v2 could break parsing; mitigate with version detection and fixture updates.
- **Performance:** Large evidence trees may slow aggregation; consider caching latest file metadata and documenting retention expectations.
- **Evidence growth:** Warning-only stance means repositories can still exceed 25 MB; coordinate with T31 (archival) if alerts persist.
- **HAL visibility:** Default skip presentation must clearly explain how to enable telemetry so genuine failures are not overlooked.
- **CLI/tooling dependencies:** `jq` availability and shell environment must be documented to keep CLI fallback reliable.

## Consensus & Risks (Multi-AI)
- **Agreement:** All agents converged on eight thematic slices covering ingestion, evidence sentinel, TUI, CLI, fixtures, documentation, and HAL resilience. Shared emphasis on schema v1/v2 support and leveraging Byterover lessons about HAL/checksum coverage gaps.
- **Disagreement & resolution:**
  - Gemini proposed a standalone `spec_status_cli` binary; consensus kept existing bash script with JSON mode to minimise maintenance.
  - Claude suggested immediate CLI deprecation; group kept CLI for headless workflows pending operator adoption data.
  - Code agent advocated bundling evidence footprint checks inside dashboard while others preferred `/spec-evidence-stats` only; compromise displays lightweight warnings in dashboard and preserves detailed command for deep dives.
- **Degradations:** All three agents produced outputs; no models were unavailable. Consensus recorded above.

## Exit Criteria (Done)
- `/spec-status SPEC-ID` renders complete dashboard in TUI with guardrail, consensus, agent, HAL/policy, and evidence footprint information.
- CLI fallback emits JSON/text parity and flags degraded runs clearly.
- Unit/integration suites, doc lint, and SPEC task lint pass on CI; evidence artifacts (logs, screenshots, fixture manifests) stored under SPEC-KIT-035 directory.
- Operator documentation published with troubleshooting, and SPEC.md tracker reflects completion evidence (status moved beyond Backlog during /implement).
