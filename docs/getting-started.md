# Getting started with Codex CLI

### CLI usage

| Command          | Purpose                            | Example                       |
| ---------------- | ---------------------------------- | ----------------------------- |
| `code`           | Interactive TUI                    | `code`                        |
| `code "..."`     | Initial prompt for interactive TUI | `code "fix lint errors"`      |
| `code exec "..."` | Non-interactive "automation mode"  | `code exec "explain utils.ts"` |

Key flags: `--model/-m`, `--ask-for-approval/-a`.

### Running with a prompt as input

You can also run the Code CLI with a prompt as input:

```shell
code "explain this codebase to me"
```

```shell
code --full-auto "create the fanciest todo-list app"
```

That's it - Code will scaffold a file, run it inside a sandbox, install any
missing dependencies, and show you the live result. Approve the changes and
they'll be committed to your working directory.

### Example prompts

Below are a few bite-size examples you can copy-paste. Replace the text in quotes with your own task.

| ✨  | What you type                                                              | What happens                                                               |
| --- | ---------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| 1   | `code "Refactor the Dashboard component to React Hooks"`                     | Code rewrites the class component, runs `npm test`, and shows the diff.    |
| 2   | `code "Generate SQL migrations for adding a users table"`                    | Infers your ORM, creates migration files, and runs them in a sandboxed DB. |
| 3   | `code "Write unit tests for utils/date.ts"`                                  | Generates tests, executes them, and iterates until they pass.              |
| 4   | `code "Bulk-rename *.jpeg -> *.jpg with git mv"`                             | Safely renames files and updates imports/usages.                           |
| 5   | `code "Explain what this regex does: ^(?=.*[A-Z]).{8,}$"`                    | Outputs a step-by-step human explanation.                                  |
| 6   | `code "Carefully review this repo, and propose 3 high impact well-scoped PRs"` | Suggests impactful PRs in the current codebase.                            |
| 7   | `code "Look for vulnerabilities and create a security review report"`        | Finds and explains security bugs.                                          |

### Memory with AGENTS.md

You can give Every Code extra instructions and guidance using `AGENTS.md` files. Code looks for `AGENTS.md` files in the following places, and merges them top-down:

1. `~/.code/AGENTS.md` - personal global guidance (Code will also read a legacy `~/.codex/AGENTS.md` if present)
2. `AGENTS.md` at repo root - shared project notes
3. `AGENTS.md` in the current working directory - sub-folder/feature specifics

For more information on how to use AGENTS.md, see the [official AGENTS.md documentation](https://agents.md/).

### Tips & shortcuts

#### Use `@` for file search

Typing `@` triggers a fuzzy-filename search over the workspace root. Use up/down to select among the results and Tab or Enter to replace the `@` with the selected path. You can use Esc to cancel the search.

#### Image input

Paste images directly into the composer (Ctrl+V / Cmd+V) to attach them to your prompt. You can also attach files via the CLI using `-i/--image` (comma‑separated):

```bash
code -i screenshot.png "Explain this error"
code --image img1.png,img2.jpg "Summarize these diagrams"
```

#### Esc–Esc to edit a previous message

When the chat composer is empty, press Esc to prime “backtrack” mode. Press Esc again to open a transcript preview highlighting the last user message; press Esc repeatedly to step to older user messages. Press Enter to confirm and Code will fork the conversation from that point, trim the visible transcript accordingly, and pre‑fill the composer with the selected user message so you can edit and resubmit it.

In the transcript preview, the footer shows an `Esc edit prev` hint while editing is active.

#### Shell completions

Generate shell completion scripts via:

```shell
code completion bash
code completion zsh
code completion fish
```

#### `--cd`/`-C` flag

Sometimes it is not convenient to `cd` to the directory you want Code to use as the "working root" before running. Fortunately, `code` supports a `--cd` option so you can specify whatever folder you want. You can confirm that Code is honoring `--cd` by double-checking the **workdir** it reports in the TUI at the start of a new session.
