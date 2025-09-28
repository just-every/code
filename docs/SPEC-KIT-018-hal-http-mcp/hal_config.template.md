# HAL MCP Configuration Template

```
# ~/.code/config.toml
[mcp_servers.hal]
command = "npx"
args = ["-y", "hal-mcp"]
startup_timeout_sec = 20
tool_timeout_sec = 120

env = {
    # Project supplies concrete URL (e.g., http://127.0.0.1:7878)
    "HAL_BASE_URL" = "${HAL_BASE_URL}",
    # Secret placeholder resolved by MCP secret store (e.g., HAL_SECRET_KAVEDARR_API_KEY)
    "HAL_DEFAULT_HEADERS" = "{\"X-Api-Key\": \"{secrets.HAL_SECRET_API_KEY}\"}"
}

# Optional profile override: HAL_PROFILE points to project-specific JSON payloads
# export HAL_PROFILE="$HOME/<project>/config/hal_profile.json"
```

> Replace `${HAL_BASE_URL}` and `HAL_SECRET_API_KEY` with project-provided values in your own repository (e.g., `/home/thetu/kavedarr`). This template stays generic inside the just-every/code repo.
