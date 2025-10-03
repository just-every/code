# Kavedarr Architecture & Planning

> Status: draft v0.1 (2025-10-02). Complements `product-requirements.md` and fulfils the constitution’s mandatory context references.

## 1. Monorepo Overview
- **Repository:** just-every/code (feature branch: feat/spec-auto-telemetry)
- **Primary language:** Rust (Codex CLI + guardrail extensions)
- **Secondary tooling:** Bash guardrail scripts, MCP (Model Context Protocol) servers, Node-based helpers (package.json for aux scripts).
- **Key directories:**
  - `codex-rs/` – Rust workspace containing Codex CLI/TUI, guardrail integrations, MCP clients.
  - `scripts/spec_ops_004/` – Shell automation for Spec Kit guardrail stages and support utilities.
  - `docs/spec-kit/` – Prompt versions, model strategy, automation runbooks, evidence policies.
  - `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/` – Telemetry and evidence artefacts organised by SPEC ID.
  - `memory/` – Constitution and stored guidance for operating Spec Kit flows.

## 2. Component Architecture
### 2.1 Spec Kit Guardrail Layer
- **Purpose:** Enforce evidence-driven workflow across plan → tasks → implement → validate → audit → unlock stages.
- **Implementation:**
  - Stage-specific scripts (`spec_ops_004/commands/spec_ops_{stage}.sh`) share helpers via `common.sh`.
  - Telemetry emitted as JSON (schema v1) per stage with optional HAL payloads.
  - `/spec-ops-auto` orchestrates sequential execution; multi-agent consensus automation remains a roadmap item.

### 2.2 Multi-Agent Prompt Layer (planned automation)
- Prompt templates in `docs/spec-kit/prompts.json` for Gemini (research), Claude Sonnet 4.5 (synthesis), GPT-5 / GPT-5-Codex (arbiter/executor).
- Model allocation recorded in `docs/spec-kit/model-strategy.md` with escalation rules.
- Current state: prompts manually invoked from the Codex TUI; consensus runner and synthesis scripts not yet implemented (tracked in roadmap tasks).

### 2.3 Codex CLI / TUI Integration
- `/spec-*` slash commands defined in `codex-rs/tui/src/slash_command.rs` route to guardrail scripts or prompt flows.
- `/spec-ops-*` commands run shell automation via `scripts/env_run.sh` to preserve environment parity.
- Local-memory interactions and MCP connections mediated through `codex-rs/core` and `codex-rs/mcp-client` crates.

### 2.4 External Services
- **HAL HTTP MCP** (`docs/hal/hal_config.toml`): validates Kavedarr API endpoints; requires `HAL_SECRET_KAVEDARR_API_KEY`.
- **Local-memory store:** canonical knowledge base for Spec Kit; Byterover MCP used as fallback until migration completes.

## 3. Technology Stack & Dependencies
- **Rust Toolchain:** Stable 1.80+ (configured via `rust-toolchain.toml`).
- **Package Management:** Cargo workspaces (`codex-rs/Cargo.toml`), npm/pnpm for auxiliary scripts.
- **Shell Environment:** Bash 5+, `env_run.sh` ensures `.env` secrets respected.
- **MCP Servers:** repo-search, doc-index, shell-lite, git-status, hal (documented in AGENTS.md / CLAUDE.md).
- **Optional:** Node-based utilities (e.g., `scripts/spec_ops_004/evidence_stats.sh` uses standard Unix tooling).

## 4. Constraints & Assumptions
- Guardrail scripts expect to run from repository root with clean git status unless `SPEC_OPS_ALLOW_DIRTY=1`.
- `/spec-auto` currently only sequences shell stages; agent automation will introduce additional dependencies (Gemini, Claude, GPT-5 APIs via Codex CLI).
- Evidence is stored within git; growth is monitored via `evidence_stats.sh` with a soft threshold of 25 MB per SPEC.
- HAL service must be reachable for full validation runs; if unavailable, operators set `SPEC_OPS_HAL_SKIP=1` (decision pending).
- Cargo workspace root is `codex-rs/`. Commands like `cargo test -p codex-tui spec_auto` must run within that directory; plan includes documenting this in CLAUDE.md and adjusting `SPEC_OPS_CARGO_MANIFEST` defaults if necessary.

## 5. Build & Test Plan
- **Rust:** `cd codex-rs && cargo fmt && cargo clippy && cargo test` (ensure `spec_auto` target passes once workspace path fix is complete).
- **Guardrail validation:**
  1. `SPEC_OPS_ALLOW_DIRTY=0 ./scripts/spec_ops_004/spec_auto.sh SPEC-KIT-DEMO` (expect clean run).
  2. Simulate HAL failure (`SPEC_OPS_HAL_SKIP=0` with service offline) to confirm non-zero exit and telemetry logging.
  3. Manually trigger `/spec-plan`, `/spec-tasks`, etc. to capture consensus artefacts once automation is delivered.
- **Docs & Task linting:** `scripts/doc-structure-validate.sh --mode=templates`, `python3 scripts/spec-kit/lint_tasks.py`.

## 6. Roadmap Alignment (excerpt)
- **Foundation (Critical):** product-requirements.md, PLANNING.md (this document), workspace path correction, guardrail validation run.
- **Consensus Automation (High):** consensus runner script, prompt substitution, synthesis validator.
- **Testing (Medium):** add spec-auto integration tests in `codex-rs/tui` crate, confirm HAL failure propagation.
- **Documentation Sync (High):** update CLAUDE.md to reflect real capabilities and prerequisites; reconcile SPEC.md tasks with evidence.
- **Strategic:** Decide on cross-repo separation vs embedded spec-kit tooling; evaluate agent delegation wiring.

## 7. Risks & Mitigations
- **Doc Drift:** Mitigate by adding CLAUDE.md troubleshooting section and linking to this plan.
- **HAL Availability:** Provide skip flag and plan for mock service; document fallback procedures.
- **Evidence Sprawl:** Track via evidence-baseline.md updates; design archival strategy before exceeding threshold.
- **Automation Gap:** Until consensus runner exists, clearly label manual steps in docs to avoid false expectations.

## 8. Milestones & Owners
- **Gate 1 (Foundation)** – Owners: Spec Kit maintainers; due ASAP.
- **Gate 2 (Consensus Operational)** – Requires implementation of tasks 4–5 in the audit checklist.
- **Gate 3 (Production Ready)** – After HAL decision, doc sync, and integration tests pass.
- **Gate 4 (Architectural Clarity)** – Based on cross-repo decision and agent wiring outcome.

## 9. Open Questions
- Should `/spec-auto` automatically call consensus runner when it becomes available, or remain opt-in via flag?
- How do we capture cost telemetry for high-reasoning runs to feed into governance? (Mentioned in model strategy but not implemented.)
- Do we separate guardrail tooling into a dedicated repo to avoid coupling with Codex CLI updates?

Document owner: @just-every/automation

