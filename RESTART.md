# Restart Plan: Spec Kit Multi-Agent Pipeline

## Status
Checked MCP stack: `repo_search`, `doc_index`, `shell_lite`, and `git_status` all respond via `codex-mcp-client`. SPEC-kit T2, T9, T11, and T12 are complete; remaining backlog covers T10 local-memory migration, T13 telemetry schema guard, T14 docs refresh, and T15 nightly sync.

## Validation Commands
CODEX_HOME=.github/codex/home code mcp list --json
(cd codex-rs && target/debug/codex-mcp-client uvx awslabs.git-repo-research-mcp-server@latest)
(cd codex-rs && target/debug/codex-mcp-client npx -y open-docs-mcp --docsDir /home/thetu/code/docs)
(cd codex-rs && target/debug/codex-mcp-client npx -y super-shell-mcp)
(cd codex-rs && target/debug/codex-mcp-client uvx mcp-server-git --repository /home/thetu/code)

## Next Steps
1. Deliver T10 local-memory migration (mirror Byterover domains, wire read/write hooks).
2. Enforce telemetry schema validation (T13) so `/spec-auto` fails on malformed evidence.
3. Refresh docs/onboarding for new Spec Kit workflow (T14).
4. Design nightly sync drift detector comparing local-memory vs evidence logs (T15).
5. Pilot MCP expansions (T16–T19):
   - HAL (basic HTTP tools working; add staging OpenAPI + secrets, whitelist).
   - Postgres (select read-only DSN, stdio/proxy server).
   - Confirm `just start-*/stop-*` recipes under `/spec-auto` and archive logs.
