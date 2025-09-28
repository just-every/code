# Plan: T18 HAL HTTP MCP Integration
## Inputs
- Spec: docs/SPEC-KIT-018-hal-http-mcp/spec.md (16cfdc66)
- Constitution: memory/constitution.md (1.1)

## Work Breakdown
1. Author `docs/hal/hal_config.toml` inside the product repo (for Kavedarr: `~/kavedarr/docs/hal/hal_config.toml`) pointing HAL at `http://127.0.0.1:7878` with default headers that pull `HAL_SECRET_KAVEDARR_API_KEY` from the secret store.
2. Create `docs/hal/hal_profile.json` in the product repo containing the smoke requests (health, movie list, indexer test, GraphQL) and ensure they align with current API routes.
3. Update operator docs (README/slash command guidance) so `/spec-*` flows include HAL smoke checks, reference the per-project config location, and remind operators to keep secrets out of this repo.
4. Run the HAL profile locally from the product repo, archive outputs under that repo's `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/`, update SPEC tracker, and lint tasks.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| HAL config committed | Product repo contains `docs/hal/hal_config.toml` with host + secret reference | File review |
| HAL profile committed | Product repo contains `docs/hal/hal_profile.json` with required requests | `code mcp call hal health` |
| Docs/prompts updated | README + guardrail prompts mention HAL usage | Doc diff + `cargo test -p codex-tui spec_auto` |
| Evidence captured | JSON stored under product repo `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/` | Evidence files |

## Risks & Unknowns
- Local API must be running with migrations applied; HAL calls will fail otherwise.
- API key bootstrap is one-time; losing the generated key requires rotation.
- Evidence may contain sensitive IDs—scrub before sharing externally.

## Consensus & Risks (Multi-AI)
- Solo Codex planning pending full multi-agent `/plan`; rerun with additional agents if required by governance.

## Exit Criteria (Done)
- HAL config + docs merged; sample smoke evidence stored under SPEC-KIT-018 in product repo.
- `/spec-*` flows mention HAL checks and succeed locally.
- SPEC tracker row T18 updated with evidence link and status.
- `scripts/spec-kit/lint_tasks.py` passes after tracker update.
