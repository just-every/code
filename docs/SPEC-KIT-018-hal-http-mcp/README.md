# HAL Integration Templates

This directory contains reusable scaffolding for wiring the HAL HTTP MCP into projects that use the just-every/code toolchain. All values are placeholders—projects must copy these files into their own repositories (for example `~/kavedarr`) and fill in concrete hosts, secrets, and request payloads.

## Files
- `spec.md` / `plan.md` / `tasks.md`: governance artifacts describing the template workstream (no project-specific data).
- `hal_config.template.md`: example `[mcp_servers.hal]` block for `~/.code/config.toml` with placeholder environment variables.
- `requests.template.json`: sample HAL request definitions (health, authenticated REST, GraphQL) using `{secrets.HAL_SECRET_API_KEY}` and other placeholders.

## How to Use in a Project
1. Copy the template files into your project repo (e.g., `~/kavedarr/docs/hal/`).
2. Generate or retrieve the project API key. Store it in the MCP secret store as `HAL_SECRET_API_KEY` (never commit it) and record bootstrap/rotation steps in the project docs.
3. Set concrete values for `HAL_BASE_URL`, `HAL_DEFAULT_HEADERS`, or override via `HAL_PROFILE`. Update the copied request JSON with project-specific paths and bodies.
4. Register the HAL profile in the project’s Codex config (or export `HAL_PROFILE`).
5. Capture HAL evidence files alongside the project (not in this repo).

> Never commit real API keys, hosts, or project-specific payloads back into this template repository.
