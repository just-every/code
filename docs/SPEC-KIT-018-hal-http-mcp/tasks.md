# Tasks: T18 HAL HTTP MCP Integration

| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | Author project HAL config (`docs/hal/hal_config.toml` in product repo) pointing at local API | Code | Done | Manual review |
| 2 | Create HAL request profile (`docs/hal/hal_profile.json` in product repo) covering health/REST/GraphQL | Code | Done | `cargo run -p codex-mcp-client --bin call_tool -- --tool http-get …` |
| 3 | Update docs/prompts to include HAL smoke guidance | Code | Done | Doc diff + `cargo test -p codex-tui spec_auto` |
| 4 | Run HAL smoke + archive evidence under SPEC-KIT-018; update SPEC.md | Code | Done | `cargo run -p codex-mcp-client --bin call_tool …` evidence JSON in project repo + `cargo test -p codex-tui spec_auto` |
