# Spec: HAL HTTP MCP Integration (T18)

## Context
- Task ID: T18 (SPEC.md tracker)
- HAL MCP client is already discoverable via `config.toml` (`[mcp_servers.hal]`), but it is not yet wired to our workflows.
- Codex slash commands and guardrails currently rely on ad-hoc curl scripts to verify the Kavedarr API; secrets are injected manually.
- Our primary API instance runs locally (default `HOST=127.0.0.1`, `PORT=7878`) via axum; it exposes health checks, REST endpoints under `/api/v3`, and the GraphQL service at `/graphql` guarded by the API key middleware.
- We have a one-time API key bootstrap flow (keys prefixed `kvd_`) backed by `ApiKeyService` and the `API_KEY_MASTER_SECRET` env var.

## Objectives
1. Provision a HAL MCP profile that targets the local Kavedarr API (`http://127.0.0.1:7878`) with authenticated requests using the generated `kvd_` API key.
2. Provide concrete HAL request definitions (health, movie listing, indexer test, GraphQL ping) stored in the product repository so guardrails can reuse them.
3. Document how operators bootstrap/rotate `HAL_SECRET_KAVEDARR_API_KEY` and where to keep the generated key (Codex secret store, not committed).
4. Update `/spec-*` flows and runbooks so HAL smoke checks become part of the validation evidence (stored under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/`).

## Scope
- Concrete HAL MCP configuration snippet (`docs/hal/hal_config.toml`) checked into the product repo referencing the Kavedarr base URL and secret placeholder.
- HAL request profile (`docs/hal/hal_profile.json`) in the product repo covering the smoke checks.
- Operator documentation covering API key bootstrap/rotation and Hal usage.
- Guardrail integration that captures HAL responses under the product repo's `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/`.

## Non-Goals
- Standing up a hosted staging environment (we continue to hit the local/development instance).
- Replacing existing integration tests; HAL is complementary smoke coverage.
- Automating API key rotation (document manual steps only).

## Acceptance Criteria
- HAL MCP entry registered and working against the local API (manual `cargo run -p codex-mcp-client --bin call_tool -- --tool … -- npx -y hal-mcp` succeeds).
- HAL evidence (health + authenticated call + GraphQL) stored under the product repo's SPEC-KIT-018 evidence directory.
- `/spec-*` prompts mention HAL usage and evidence requirement.
- SPEC tracker row T18 updated with evidence path and status.

## Task Breakdown (2025-09-28)
### Task Slices
- **Guardrail engineer (Code)** – Pair with T20 owners to verify baseline and HAL failure propagation fixes by forcing failing `/spec-ops-plan SPEC-KIT-018` runs and confirming telemetry now marks `baseline.status`/`hal.summary.status` as `failed`.
- **HAL integrator (Gemini)** – Finalize `docs/hal/hal_config.toml.example` and `docs/hal/hal_profile.json` templates with secret placeholders and manifest-aware instructions for syncing into the product repo.
- **HAL integrator (Gemini)** – Execute `/spec-ops-validate SPEC-KIT-018` against unhealthy and healthy HAL states; archive artifacts under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/` with scenario metadata.
- **Docs lead (Claude)** – Update `/spec-*` documentation (slash commands, AGENTS, onboarding) to embed HAL smoke prerequisites, evidence expectations, and consensus telemetry reminders.
- **Tracker steward (Code)** – Update SPEC.md row T18 with refreshed evidence links, rerun lint scripts, and document telemetry artifacts in review notes.

**Dependencies**
- T20 Step 2 (HAL failure propagation) and Step 3 (manifest awareness / GraphQL fix) from docs/SPEC-OPS-004-integrated-coder-hooks/plan.md.
- Local Kavedarr API reachable with valid `HAL_SECRET_KAVEDARR_API_KEY`.
- T14 documentation placeholders ready to receive HAL guidance.

**Validation**
- `cargo run --manifest-path codex-rs/Cargo.toml -p codex-mcp-client --bin call_tool -- --tool http-get --args '{"url":"http://127.0.0.1:7878/health"}' -- npx -y hal-mcp` (healthy run).
- `/spec-ops-validate SPEC-KIT-018` in degraded & healthy modes with telemetry inspection for `hal.summary`.
- `scripts/doc-structure-validate.sh --mode=templates --dry-run` and `python3 scripts/spec-kit/lint_tasks.py` before finalizing docs/tracker updates.

**Docs**
- `docs/hal/hal_config.toml.example`, `docs/hal/hal_profile.json`, `docs/slash-commands.md`, `AGENTS.md`, `docs/getting-started.md`, SPEC.md notes.

**Risks & Assumptions**
- HAL API downtime can delay evidence capture; consider fallback mock or scheduled maintenance window.
- Improper handling of `HAL_SECRET_KAVEDARR_API_KEY` could leak secrets; documentation must stress Codex secret store usage and rotation cadence.
- Template repo vs product repo drift; assume a sync checklist will accompany handoff.

**Consensus**
- Agreement (Claude/Gemini/Code): Treat guardrail fixes as gating, capture both failure and success evidence, and align documentation with template delivery.
- Divergence: Gemini preferred deferring all execution until T20 merges; consensus allows template prep while flagging execution steps as blocked pending guardrail verification.
- Degraded participation: Only GPT-5 Codex responded directly; plan to re-confirm with full agent quorum before locking timelines.
