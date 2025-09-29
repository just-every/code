## Getting started

### CLI usage

| Command            | Purpose                            | Example                         |
| ------------------ | ---------------------------------- | ------------------------------- |
| `codex`            | Interactive TUI                    | `codex`                         |
| `codex "..."`      | Initial prompt for interactive TUI | `codex "fix lint errors"`       |
| `codex exec "..."` | Non-interactive "automation mode"  | `codex exec "explain utils.ts"` |

Key flags: `--model/-m`, `--ask-for-approval/-a`.

### Running with a prompt as input

You can also run Codex CLI with a prompt as input:

```shell
codex "explain this codebase to me"
```

```shell
codex --full-auto "create the fanciest todo-list app"
```

That's it - Codex will scaffold a file, run it inside a sandbox, install any
missing dependencies, and show you the live result. Approve the changes and
they'll be committed to your working directory.

### Example prompts

Below are a few bite-size examples you can copy-paste. Replace the text in quotes with your own task.

| ✨  | What you type                                                                   | What happens                                                               |
| --- | ------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| 1   | `codex "Refactor the Dashboard component to React Hooks"`                       | Codex rewrites the class component, runs `npm test`, and shows the diff.   |
| 2   | `codex "Generate SQL migrations for adding a users table"`                      | Infers your ORM, creates migration files, and runs them in a sandboxed DB. |
| 3   | `codex "Write unit tests for utils/date.ts"`                                    | Generates tests, executes them, and iterates until they pass.              |
| 4   | `codex "Bulk-rename *.jpeg -> *.jpg with git mv"`                               | Safely renames files and updates imports/usages.                           |
| 5   | `codex "Explain what this regex does: ^(?=.*[A-Z]).{8,}$"`                      | Outputs a step-by-step human explanation.                                  |
| 6   | `codex "Carefully review this repo, and propose 3 high impact well-scoped PRs"` | Suggests impactful PRs in the current codebase.                            |
| 7   | `codex "Look for vulnerabilities and create a security review report"`          | Finds and explains security bugs.                                          |

### Memory with AGENTS.md

You can give Codex extra instructions and guidance using `AGENTS.md` files. Codex looks for `AGENTS.md` files in the following places, and merges them top-down:

1. `~/.code/AGENTS.md` - personal global guidance (Code will also read a legacy `~/.codex/AGENTS.md` if present)
2. `AGENTS.md` at repo root - shared project notes
3. `AGENTS.md` in the current working directory - sub-folder/feature specifics

For more information on how to use AGENTS.md, see the [official AGENTS.md documentation](https://agents.md/).

### Tips & shortcuts

### Spec Kit workflow quickstart

When working on SPEC tasks, run guardrail commands before multi-agent stages:

1. `/spec-ops-plan <SPEC-ID>` – prepares the workspace and writes telemetry JSON (schema v1) under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`.
2. `/spec-ops-tasks`, `/spec-ops-implement`, `/spec-ops-validate`, `/spec-ops-audit`, `/spec-ops-unlock` – each stage records telemetry with stage-specific fields (`tool.status`, `lock_status`, `scenarios`, etc.).
3. `/spec-auto <SPEC-ID>` orchestrates the full pipeline; it stops if telemetry fails schema validation or consensus degrades.

Set `SPEC_OPS_CARGO_MANIFEST` when the Rust workspace lives outside the repo root (defaults to `codex-rs/Cargo.toml`). Use `--allow-fail` sparingly when triaging baseline regressions; reset the flag once fixes land so failures propagate again.

#### HAL smoke & evidence checklist

- Configure HAL MCP via `docs/hal/hal_config.toml` + `docs/hal/hal_profile.json`, then run:
  - `cargo run -p codex-mcp-client --bin call_tool -- --tool health … -- npx -y hal-mcp`
  - `/spec-ops-validate SPEC-KIT-018` with HAL healthy and again with HAL offline.
- Store the resulting JSON/logs under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/` (include timestamps for healthy vs degraded runs, e.g., healthy `20250929-163421Z-hal-health.json`, degraded telemetry `spec-validate_2025-09-29T16:25:38Z-2828521850.json`).
- Export `SPEC_OPS_TELEMETRY_HAL=1` when running guardrails to add `hal.summary` (`status`, `failed_checks`, `artifacts`) to telemetry; this helps `/spec-auto` gate on HAL outcomes.

#### Telemetry & consensus troubleshooting

- Schema failures → open the newest JSON in the evidence directory and verify the common envelope fields (`command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts[]`) and stage payloads (Plan `baseline.*`, Tasks `tool.status`, Implement `lock_status`/`hook_status`, Validate/Audit `scenarios`, Unlock `unlock_status`). See docs/SPEC-KIT-013-telemetry-schema-guard/spec.md for the authoritative checklist.
- Degraded consensus → rerun the affected `/spec-*` stage with higher budgets (e.g., `/spec-plan --deep-research`) and confirm each agent response includes `model`, `model_release`, and `reasoning_mode` per docs/spec-kit/model-strategy.md.
- Persistent HAL failures → confirm `SPEC_OPS_CARGO_MANIFEST` is pointing to the correct workspace, then re-run the HAL smoke commands. Capture a degraded run (e.g., `spec-validate_2025-09-29T16:25:38Z-2828521850.json`) before toggling overrides so the evidence trail records the failure.
- For session-specific recovery steps, follow RESTART.md; it now links back to this troubleshooting section for canonical guidance.

Troubleshooting:

- If guardrail telemetry fails schema validation, open the latest JSON in the evidence directory and verify required fields per docs/SPEC-KIT-013-telemetry-schema-guard/spec.md.
- `scripts/spec-kit/lint_tasks.py` validates SPEC.md tracking after updating docs or task status.
- Use `/spec-audit` to re-run consensus after resolving degraded verdicts; consult docs/spec-kit/model-strategy.md for escalation rules.

#### Use `@` for file search

Typing `@` triggers a fuzzy-filename search over the workspace root. Use up/down to select among the results and Tab or Enter to replace the `@` with the selected path. You can use Esc to cancel the search.

#### Image input

Paste images directly into the composer (Ctrl+V / Cmd+V) to attach them to your prompt. You can also attach files via the CLI using `-i/--image` (comma‑separated):

```bash
codex -i screenshot.png "Explain this error"
codex --image img1.png,img2.jpg "Summarize these diagrams"
```

#### Esc–Esc to edit a previous message

When the chat composer is empty, press Esc to prime “backtrack” mode. Press Esc again to open a transcript preview highlighting the last user message; press Esc repeatedly to step to older user messages. Press Enter to confirm and Codex will fork the conversation from that point, trim the visible transcript accordingly, and pre‑fill the composer with the selected user message so you can edit and resubmit it.

In the transcript preview, the footer shows an `Esc edit prev` hint while editing is active.

#### Shell completions

Generate shell completion scripts via:

```shell
code completion bash
code completion zsh
code completion fish
```

#### `--cd`/`-C` flag

Sometimes it is not convenient to `cd` to the directory you want Codex to use as the "working root" before running Codex. Fortunately, `codex` supports a `--cd` option so you can specify whatever folder you want. You can confirm that Codex is honoring `--cd` by double-checking the **workdir** it reports in the TUI at the start of a new session.
