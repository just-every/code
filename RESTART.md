# Restart Plan: Spec Kit Multi-Agent Pipeline

## Status
Checked MCP stack: `repo_search`, `doc_index`, `shell_lite`, and `git_status` all respond via `codex-mcp-client`. Kept the optional `spec_registry` proxy slot; hit `mcp-proxy https://registry.modelcontextprotocol.io/api/sse` (or `/v0/api/sse` once live) to detect when the public registry feed comes online. SPEC-kit T2, T9, T11 are complete; remaining backlog covers T10 local-memory migration, T12 consensus diff reviewer, T13 telemetry schema guard, T14 docs refresh, and T15 nightly sync.

## Validation Commands
CODEX_HOME=.github/codex/home code mcp list --json
(cd codex-rs && target/debug/codex-mcp-client uvx awslabs.git-repo-research-mcp-server@latest)
(cd codex-rs && target/debug/codex-mcp-client npx -y open-docs-mcp --docsDir /home/thetu/code/docs)
(cd codex-rs && target/debug/codex-mcp-client npx -y super-shell-mcp)
(cd codex-rs && target/debug/codex-mcp-client uvx mcp-server-git --repository /home/thetu/code)
(cd codex-rs && target/debug/codex-mcp-client npx -y mcp-proxy https://registry.modelcontextprotocol.io/api/sse)  # expect timeout/404 until SSE feed is live

## Next Steps
1. Deliver T10 local-memory migration (mirror Byterover domains, wire read/write hooks).
2. Build T12 consensus diff reviewer MCP tool and integrate into `/spec-auto` gating.
3. Enforce telemetry schema validation (T13) so `/spec-auto` fails on malformed evidence.
4. Refresh docs/onboarding for new Spec Kit workflow (T14).
5. Design nightly sync drift detector comparing local-memory vs evidence logs (T15).
