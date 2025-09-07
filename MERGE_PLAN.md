# Upstream Merge Plan (by-bucket)

Mode: by-bucket
Upstream: openai/codex@main -> merge into branch `upstream-merge` from `origin/main`.

Policy
- Keep ours by default; never reintroduce locally-removed crates/dirs.
- Prefer ours for protected paths:
  - codex-rs/tui/**
  - codex-cli/**
  - .github/workflows/**
  - docs/**
  - AGENTS.md, README.md, CHANGELOG.md
- Purge any upstream reintroduction of: .github/codex-cli-*.png|jpg|jpeg|webp
- Do not use --allow-unrelated-histories.

Buckets and strategy
- TUI (codex-rs/tui/**): Keep ours entirely (strict history ordering and our UX).
- CLI (codex-cli/**): Keep ours; upstream changes dropped unless clearly compatibility fixes.
- Workflows/Docs: Keep ours; drop upstream re-orgs and images per purge policy.
- Rust Core (codex-rs/core/**, common, protocol, exec, etc.): Default to ours; selectively accept upstream when low-risk and improves correctness/security. Resolve conflicts in favor of ours when unsure.
- Other crates (browser, login, mcp-*, ollama, arg0, apply-patch, file-search): Keep ours unless upstream brings clear bug fixes compatible with our public API. For this pass, default to ours.

Notes from artifacts
- Upstream heavily edits TUI (tui=140) and Core (core=60). We will preserve our TUI and opt-out of those diffs.
- Upstream adds large repo images under .github; these will be purged.
- No perma-removed paths on our default detected; no reintroduced paths to block beyond purge_globs.

Validation
- Run ./build-fast.sh; treat warnings as failures and fix minimally/surgically.
