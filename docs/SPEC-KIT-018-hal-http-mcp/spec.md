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
