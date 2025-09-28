# HAL MCP Profile Templates

The concrete HAL configuration files live inside each product repository (for
Kavedarr they reside under `~/kavedarr/docs/hal/`). This directory documents the
expected structure so downstream projects can keep their own copies in the
appropriate repo.

Required project files:

- `docs/hal/hal_config.toml` in the product repo: merged into
  `~/.code/config.toml` so HAL points at the local API host.
- `docs/hal/hal_profile.json` in the product repo: defines the smoke requests
  (health/list_movies/indexer_test/graphql_ping) invoked through the HAL MCP
  server.

Remember to generate the API key once (watch the server bootstrap output) and
store it as `HAL_SECRET_KAVEDARR_API_KEY` (or the project-specific equivalent)
in the Codex secret store. Never commit the actual key.
