# HAL MCP Profile

This directory contains the concrete HAL configuration for Kavedarr.

- `hal_config.toml`: merge into `~/.code/config.toml` so HAL points at `http://127.0.0.1:7878`.
- `hal_profile.json`: list of smoke request definitions. Run via `code mcp call hal <name>`.

Remember to generate the API key once (watch the server bootstrap output) and store it as `HAL_SECRET_KAVEDARR_API_KEY` in the Codex secret store. Never commit the actual key.
