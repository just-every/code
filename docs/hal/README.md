# HAL MCP Profile Templates

The concrete HAL configuration files live inside each product repository (for
Kavedarr they reside under `~/kavedarr/docs/hal/`). This directory documents the
expected structure so downstream projects can keep their own copies in the
appropriate repo.

Required project files:

- `docs/hal/hal_config.toml` in the product repo: merged into
  `~/.code/config.toml` so HAL points at the local API host. Use the
  `hal_config.toml.example` shipped in this repo as a starting point.
- `docs/hal/hal_profile.json` in the product repo: defines the smoke requests
  (health/list_movies/indexer_test/graphql_ping) invoked through the HAL MCP
  server. The example in this repo already wires `{secrets.kavedarr.api_key}`
  placeholders so credentials stay in the Codex secret store.

Remember to generate the API key once (watch the server bootstrap output) and
store it as `HAL_SECRET_KAVEDARR_API_KEY` (or the project-specific equivalent)
in the Codex secret store. The bundled `hal_profile.json` references
`{secrets.HAL_SECRET_KAVEDARR_API_KEY}`, so the MCP client will inject the
value automatically. Never commit the actual key.

### Manual smoke example

From the template repo you can drive HAL via the new MCP client utility:

```bash
HAL_KEY=$(grep HAL_SECRET_KAVEDARR_API_KEY /path/to/project/.env | cut -d"=" -f2 | tr -d "'\r\n")
cargo run -p codex-mcp-client --bin call_tool -- \
  --tool http-get \
  --args '{"url":"http://127.0.0.1:7878/health"}' \
  --env HAL_SECRET_KAVEDARR_API_KEY=$HAL_KEY \
  -- npx -y hal-mcp
```

Pipe the JSON output to project-local evidence files (see the SPEC-KIT-018 tasks
for the canonical locations). Capture both a healthy and an induced-failure run
and commit them under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/`
with timestamped filenames, for example:

```
20250929-114636Z-hal-health.json      # healthy baseline
20250929-114708Z-hal-health.json      # HAL offline failure
```

When recording guardrail telemetry, export `SPEC_OPS_TELEMETRY_HAL=1` so `/spec-ops-validate`
adds `hal.summary` metadata pointing to each artifact. Downstream docs (slash commands,
AGENTS, getting started) reference these filenames directly, so refresh the captures whenever
HAL behavior changes.
