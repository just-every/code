# Workflow Strategy

Rust-specific `rust-ci*.yml` workflows are intentionally removed in this fork.

## Verification Paths

- `bazel.yml` is the primary Rust verification path for pull requests and for `main`.
- `release.yml` remains the post-merge release pipeline for `main`.

## Upstream Merge Guardrail

- `.github/workflows/**` is fork-owned during upstream merges.
- `.github/workflows/rust-ci.yml` and `.github/workflows/rust-ci-full.yml` are also listed in `.github/merge-policy.json` under `perma_removed_paths` so future upstream syncs keep them deleted.
