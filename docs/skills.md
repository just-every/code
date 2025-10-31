# Claude Skills in Code

Code can load Claude Skills authored in Markdown and expose them to the agent runtime. Each skill
is a directory that contains a `SKILL.md` manifest plus any supporting assets.

## Authoring `SKILL.md`

A manifest is Markdown with a YAML frontmatter block. The frontmatter **must** include:

```markdown
---
name: financial-modeling
description: Build 3-statement financial models and scenario analyses.
allowed-tools:
  - browser
  - bash
metadata:
  owner: finance
---

# Financial Modeling Skill

Step-by-step instructions for the agent…
```

Key rules:

- `name` must match the directory name exactly and contain only lowercase letters, digits, or
  hyphens.
- `description` should be a concise sentence the model can read in the skill picker.
- `allowed-tools` is optional. If present, it restricts which Code tools the skill may call.
  Supported entries today are `browser`, `agents`, `bash`, or a custom label that downstream logic
  can inspect.
- `metadata` is an optional map for your own bookkeeping.

The Markdown body can include playbooks, command snippets, or references to bundled scripts. Code
keeps these files on disk and streams content only when requested.

## Skill discovery paths

On startup Code scans these locations for skills:

- `~/.claude/skills/…`
- `<project root>/.claude/skills/…`

Extend the search paths via `config.toml`:

```toml
[skills]
user_paths = ["/opt/skills", "~/work/skills"]
project_paths = [".dev/skills"]
per_skill."financial-modeling" = true
anthropic_skills = ["pdf", "xlsx"]
```

`user_paths` resolve relative to `$HOME` when not absolute; `project_paths` resolve relative to the
workspace root.

### Example manifest

A starter skill lives under `examples/skills/hello-web/`. Copy the entire `hello-web` directory into
`~/.claude/skills/` (user scope) or `<project>/.claude/skills/` (project scope) to try it:

```shell
mkdir -p ~/.claude/skills
cp -r examples/skills/hello-web ~/.claude/skills/
```

Restart Code or reload `/settings` → Skills and the example will appear with a browser-only action.

## Enabling and using skills

Open `/settings` → **Skills** to toggle the global skills switch or individual manifests. Enabled
skills are advertised in Anthropic requests via `container.skills`, allowing Claude to auto-select
them when relevant.

When the model calls the `skill` tool, Code enforces the manifest’s `allowed-tools` and delegates
actions to existing surfaces:

| Skill action | Delegates to                  |
|--------------|-------------------------------|
| `browser`    | Unified browser tooling        |
| `agents`     | Agent orchestration subsystem |
| `bash`       | (reserved for future work)     |

Unknown actions return a clear “not implemented” response.

## Validation checklist

- `SKILL.md` exists, parses, and `name` equals the folder name.
- `allowed-tools` only lists supported entries (`browser`, `agents`, `bash`, or documented custom
  labels).
- The Skills settings pane lists the skill and toggles persist across restarts.
- Invoking the `skill` tool with a browser action routes through the browser handler without
  violating `allowed-tools`.
- Disabled or malformed skills fail gracefully and log helpful warnings.

For deeper integrations (remote catalogs, execution engines) start with
`code-rs/core/src/skills/` and extend as needed.
