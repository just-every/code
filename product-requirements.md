# Kavedarr Product Requirements

> Status: draft v0.1 (2025-10-02) — fills the missing foundation referenced by the constitution and guardrail docs.

## 1. Product Summary
- **Product name:** Kavedarr
- **Domain:** Media automation and asset management
- **Mission:** Provide a reliable pipeline for ingesting, enriching, organising, and distributing media assets while enforcing evidence-driven engineering through the Spec Kit workflow.

## 2. Primary Users & Goals
- **Content Operations Engineers** – automate ingest, tagging, and distribution of large media batches; require deterministic workflows and audit trails.
- **Media Librarians** – browse, search, and update rich metadata; need confidence that automated transformations are reversible.
- **Quality & Compliance Teams** – verify HAL HTTP endpoints and guardrail telemetry before promoting changes; need transparent evidence packs.

## 3. Current Capabilities (October 2025)
- Media asset ingest using existing Rust services (kavedarr-core, kavedarr-downloaders, kavedarr-infrastructure, kavedarr-api). Services live in the sibling Kavedarr repository and are consumed through the Codex CLI tooling in this repo.
- HAL HTTP MCP integration (SPEC-KIT-018) covering health, list_movies, indexer_test, and graphql_ping endpoints for validation.
- Spec Kit guardrail stages (`/spec-ops-plan|tasks|implement|validate|audit|unlock`) producing schema v1 telemetry in `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/`.
- Local-memory knowledge base for Spec Kit artefacts, backed by optional Byterover MCP for legacy notes.

## 4. Planned / Incomplete Capabilities
- Automated multi-agent consensus for `/spec-plan`, `/spec-tasks`, `/spec-implement`, `/spec-validate`, `/spec-audit` (currently manual via TUI prompts; automation tracked in future tasks).
- End-to-end `/spec-auto` orchestration that invokes consensus flows, records run summaries, and retries failures automatically.
- Evidence archival strategy once per-SPEC telemetry exceeds 25 MB.
- HAL offline mocks for CI and local smoke tests.

## 5. Functional Requirements
1. **Spec Kit Guardrail Stages**
   - Each stage must emit telemetry conforming to schema v1 with stage-specific fields (baseline status, tool.status, lock_status, scenarios, unlock_status).
   - Failures in guardrail scripts must propagate non-zero exit codes and halt `/spec-auto`.
2. **Consensus Artefacts (planned)**
   - Every multi-agent stage records three agent outputs plus synthesis JSON with agreements/conflicts.
   - Missing agents or unresolved conflicts block progression.
3. **HAL Validation**
   - Default behaviour: run HAL smoke checks unless explicitly skipped (decision pending).
   - HAL telemetry must include `hal.summary` with status, failed_checks, and evidence artifacts.
4. **Task Tracking**
   - SPEC.md keeps exactly one `In Progress` entry per active thread and records evidence notes for each status change.
5. **Documentation**
   - CLAUDE.md, AGENTS.md, and SPEC-KIT docs stay synchronized with actual capabilities; aspirational features must be clearly marked.

## 6. Non-Functional Requirements
- **Reliability:** Guardrail scripts and `/spec-auto` must fail fast on dirty git trees unless explicitly overridden.
- **Auditability:** Evidence directories must contain timestamped telemetry, logs, and consensus artefacts for each SPEC ID.
- **Observability:** Run `scripts/spec_ops_004/evidence_stats.sh` after major changes to monitor repository footprint.
- **Operator Experience:** Slash commands provide terse confirmations and point to telemetry/evidence locations.
- **Security:** No secrets committed to git. HAL secrets provided via environment (`HAL_SECRET_KAVEDARR_API_KEY`).

## 7. Guardrail & Telemetry Dependencies
- `docs/spec-kit/prompts.json` defines prompt versions consumed by consensus runners (pending automation).
- `docs/spec-kit/model-strategy.md` maps each stage to Gemini, Claude, and GPT-5 variants.
- `scripts/spec_ops_004/common.sh` supplies shared logic for telemetry, HAL, and policy gating.
- MCP servers (repo-search, doc-index, shell-lite, git-status, hal) must be configured before invoking slash workflows.

## 8. Current Gap Log (from 2025-10-02 assessment)
- product-requirements.md and PLANNING.md were missing; this document and the companion planning file address the blocker.
- Multi-agent consensus is only described in docs; implementation pending.
- Cargo workspace pathing issues prevent `cargo test -p codex-tui spec_auto` from running at repo root.
- SPEC.md lacks in-progress entries and consistent evidence references.
- CLAUDE.md overstated automation; requires update after this baseline is in place.

## 9. Acceptance & Measurement
- Guardrail stages considered functional when `/spec-ops-plan|...|unlock SPEC-KIT-DEMO` succeed on a clean tree and produce valid telemetry.
- `/spec-auto SPEC-KIT-DEMO` must complete all six stages (with retries logged) before feature work resumes.
- Consensus automation milestone when `/spec-plan SPEC-KIT-DEMO --consensus` writes 3 agent artefacts and synthesis, halting on conflict.

## 10. Open Questions
- Should HAL telemetry default to enabled, or remain opt-in via `SPEC_OPS_TELEMETRY_HAL=1`?
- Do we extract spec-kit tooling into a dedicated repository, or keep it embedded alongside Codex CLI?
- What is the archival policy for evidence once repo size exceeds agreed thresholds?
- How do we measure cost impact of high-reasoning runs (`gpt-5 --reasoning high`, `gemini-2.5-pro thinking`)?

## 11. Review Notes

**Reviewed by:** Project maintainer + Claude Code analysis (Sonnet 4.5)
**Review date:** 2025-10-05
**Review verdict:** Draft v0.1 approved for consensus prompt usage
**Changes from review:**
- Added section 11 (this review section) to document validation
- Confirmed alignment with PLANNING.md and constitution.md
- Validated that prompt linkage now loads this doc into agent context

**Next review trigger:** After first 5 SPECs use these foundation docs in consensus runs, or when product scope significantly changes

**Prompt linkage status:** ✅ Linked via `scripts/spec_ops_004/consensus_runner.sh` `collect_context()` function as of 2025-10-05

Document owner: @just-every/automation

