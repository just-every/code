# Prompt Architecture Overview

Updated: 2025-10-05

This document captures the current prompt surface area for `code-rs`, notes
where each prompt is consumed, and flags any fork-specific guidance that must
be preserved when we sync with upstream Codex.

## Prompt Inventory

| Category | File | Runtime Usage | Notes |
| --- | --- | --- | --- |
| Base instructions | `code-rs/core/prompt.md` | Loaded by default for non–GPT‑5 models | Mirrors upstream, kept in sync during merges. |
| Base instructions | `code-rs/core/gpt_5_codex_prompt.md` | Selected when GPT‑5/Codex family is active | Upstream content; differences tracked in `UPSTREAM_PARITY_CHECKLIST.md`. |
| Fork overlay | `code-rs/core/prompt_coder.md` | Appended after base prompt in fork builds | Holds browser/tooling UX guidance and approve/sandbox policies. |
| Review flows | `code-rs/core/review_prompt.md` | Used by review executor | Shared with upstream; lightweight customisations allowed. |
| Compact summaries | `code-rs/core/templates/compact/prompt.md` | Generated during conversation compaction | Template strings only; keep deterministic for snapshots. |
| Onboarding | `code-rs/tui/prompt_for_init_command.md` | `/init` command and AGENTS.md bootstrap | Fork-only instructions for building agent manifests. |
| Operational playbooks | `prompts/MERGE.md` | Release & upstream merge checklist | Human-facing; live in repo root for quick reference. |
| Operational playbooks | `prompts/TRIAGE.md` | Issue triage workflow | Human-facing; fork-specific wording. |

## Layering Strategy

1. **Base prompt** is selected by model family (`prompt.md` or
   `gpt_5_codex_prompt.md`).
2. **Fork overlay** (`prompt_coder.md`) is appended to inject tool UX, sandbox
   policy, and parallel-agent expectations. All edits here must remain fork
   scoped.
3. **Scenario prompts** (review, compact, onboarding) are loaded as-needed.

This layering lets upstream change base instructions without colliding with our
fork guidance. During merges, diff base prompts first, then verify overlay
still reads correctly.

## Maintenance Guidelines

- When upstream updates a base prompt, record the delta in
  `UPSTREAM_PARITY_CHECKLIST.md` and regenerate local snapshots if wording
  changes.
- Fork-specific copy belongs in `prompt_coder.md` or the operational playbooks;
  avoid patching upstream files unless we intend to upstream the change.
- Avoid introducing new prompt files without documenting them here; stale
  prompts should move to `docs/archive/` so the active surface stays small.
- Tests that rely on prompt text should treat strings as data fixtures to avoid
  tying unit tests to copy changes.

## Open Follow-ups

- Evaluate whether onboarding instructions in
  `code-rs/tui/prompt_for_init_command.md` can be slimmed down now that the
  `/init` command defaults to the fork UX.
- For new agents or tooling flows, extend `prompt_coder.md` rather than forking
  additional prompt variants unless we need per-feature toggles.
