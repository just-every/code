# Upstream merge report

- Source: openai/codex@026909622 (upstream/main)
- Target branch: upstream-merge (from origin/main)
- Strategy: `-X ours` with `--allow-unrelated-histories`

## Incorporated
- Latest changes across `codex-rs` crates (core, exec, tui, login, protocol, mcp, ollama, file-search, protocol-ts).
- Upstream updates to workspace manifests (`Cargo.toml`, `Cargo.lock`) and docs under `docs/`.
- Non-conflicting improvements in CLI scripts and Node workspace manifests.

## Dropped
- Local versions kept for top-level docs: `AGENTS.md`, `CHANGELOG.md`, `README.md` (policy: keep ours).
- On conflicts (none detected), TUI defaults/themes/agents would prefer ours; merge had no manual overrides required.

## Other changes
- No code edits were required post-merge.
- Build validated via `./build-fast.sh` (dev-fast) with zero warnings or errors.

