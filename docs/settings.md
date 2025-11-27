# TUI Settings Overlay

Full-screen settings panel for Every Code’s TUI. Use it to change models, themes, Auto Drive defaults, agents, notifications, and more without leaving the chat.

## Open & navigate
- `/settings` opens the overview; `/settings <section>` jumps directly (section names below). `/auto settings` and `/update` route into their sections.
- Keys: `↑/↓` or `j/k` to move, `Tab/Shift+Tab` to cycle sections, `Home/End` jump list ends. Enter opens/activates; Esc closes the current section, Esc again closes the overlay. `?` toggles inline help. Paste is forwarded to the active section when allowed.
- Overlay is modal: chat input is blocked while it is visible. It remembers the last active section on reopen (`pending_settings_return`).

## Persistence
- Changes write to `CODE_HOME/config.toml` when available; if that directory is missing you’ll see a warning and the changes remain session-only.
- Access mode can be stored per workspace; other settings apply globally unless your config file overrides them per project.
- Agent and MCP edits also live in the same config directory.

## Sections
- **Model**: pick the default chat model and reasoning effort.
- **Theme**: choose a theme and spinner; applies immediately.
- **Updates**: view upgrade channel/status. `/update` opens here before running installers.
- **Agents**: see built-in/custom agents, enable/disable, force read-only, add per-agent instructions. Open the Subagent editor to configure `/plan`/`/solve`/`/code` or custom slash commands.
- **Prompts**: edit saved prompt snippets.
- **Auto Drive**: set review/agents/QA/cross-check toggles, continue mode (manual/immediate/ten-seconds/sixty-seconds), model override, or “use chat model.” Updates apply to active runs.
- **Review**: choose a review model (or reuse chat), toggle auto-resolve, and set the max auto-resolve attempts.
- **Planning**: pick model/effort for planning turns or reuse the chat model.
- **Validation**: toggle validation groups and tools; see install status; trigger install help.
- **Limits**: read-only view of rate limits and context/auto-compact usage.
- **Chrome**: shown when browser attach fails; choose to retry, use a temp profile, switch to the internal browser, or cancel.
- **MCP**: enable/disable MCP servers.
- **Notifications**: toggle all or set filters. `/notifications on|off|status` also lands here.

## Overlay lifecycle
- All keys route through `handle_settings_key` while open; composer/history ignore input until closed.
- Help overlay (`?`) closes before the main overlay when Esc is pressed.
- Sections mark completion via their content structs; the overlay closes when a section reports `is_complete` (e.g., Chrome option chosen).

## Scope reminders
- Global defaults live in `CODE_HOME/config.toml`.
- Workspace overrides are honored where setters accept `cwd` (access mode) or when project-level config files exist. The UI always renders merged effective values.
- Agent commands and MCP servers are stored under `CODE_HOME` and apply to all workspaces unless overridden by project config.

## Commands
- `/settings [section]`
- `/auto settings`
- `/update` (or `/update settings`)
- `/notifications [on|off|status]`
