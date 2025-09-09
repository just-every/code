# JustEvery Code

&ensp;

<p align="center">
  <img src="docs/logo.png" alt="Code Logo" width="400">
</p>

&ensp;

**Code** is a fast, local coding agent for your terminal. It's a community-driven fork of `openai/codex` focused on real developer ergonomics: Browser integration, multi-agents, theming, and reasoning control — all while staying compatible with upstream.

&ensp;
## Why Code

  - 🌐 **Browser Integration** - CDP support, headless browsing, screenshots
  - 📝 **Diff Viewer** - Side-by-side diffs with syntax highlighting
  - 🤖 **Multi-Agent Commands** - /plan, /solve, /code with agent panels
  - 🎨 **Theme System** - /themes with live preview and accessibility
  - 🧠 **Reasoning Control** - /reasoning for dynamic effort adjustment
  - 🔌 **MCP support** – Extend with filesystem, DBs, APIs, or your own tools.
  - 🔒 **Safety modes** – Read-only, approvals, and workspace sandboxing.
  - 🔁 **Backwards compatible** – Supports `~/.codex/*` or default `~/.coder/*`

&ensp;
| <img src="docs/screenshots/simple.png" alt="Simple interface" width="100%"><br>Simple interface | <img src="docs/screenshots/diff.png" alt="Unified diff viewer" width="100%"><br>Unified diffs |
|:--:|:--:|

| <br><img src="docs/screenshots/browser.png" alt="Browser control" width="100%"><br>Browser control | <br><img src="docs/screenshots/agents.png" alt="Assist with Claude & Gemini" width="100%"><br>Assist with Claude & Gemini |
|:--:|:--:|


&ensp;
## Quickstart

### Run

```bash
npx -y @just-every/code
```

### Install & Run

```bash
npm install -g @just-every/code
code // or `coder` if you're using VS Code
```

Note: If another tool already provides a `code` command (e.g. VS Code), our CLI is also installed as `coder`. Use `coder` to avoid conflicts.

**Authenticate** (one of the following):
- **Sign in with ChatGPT** (Plus/Pro/Team; uses models available to your plan)
  - Run `code` and pick "Sign in with ChatGPT"
  - Stores creds locally at `~/.coder/auth.json` (also reads legacy `~/.codex/auth.json`)
- **API key** (usage-based)
  - Set `export OPENAI_API_KEY=xyz` and run `code`

### Install Claude & Gemini (optional)

Code supports orchestrating other AI CLI tools. Install these and config to use alongside Code.

```bash

npm install -g @anthropic-ai/claude-code @google/gemini-cli && claude "Just checking you're working! Let me know how I can exit." && gemini -i "Just checking you're working! Let me know how I can exit."
```

&ensp;
## Commands

### Browser
```bash
# Connect code to external Chrome browser (running CDP)
/chrome        # Connect with auto-detect port
/chrome 9222   # Connect to specific port

# Switch to internal browser mode
/browser       # Use internal headless browser
/browser https://example.com  # Open URL in internal browser
```

### Agents
```bash
# Plan code changes (Claude, Gemini and GPT-5 consensus)
# All agents review task and create a consolidated plan
/plan "Stop the AI from ordering pizza at 3AM"

# Solve complex problems (Claude, Gemini and GPT-5 race)
# Fastest preferred (see https://arxiv.org/abs/2505.17813)
/solve "Why does deleting one user drop the whole database?"

# Write code! (Claude, Gemini and GPT-5 consensus)
# Creates multiple worktrees then implements the optimal solution
/code "Show dark mode when I feel cranky"
```

### General
```bash
# Try a new theme!
/themes

# Change reasoning level
/reasoning low|medium|high

# Start new conversation
/new
```

## CLI reference

```shell
code [options] [prompt]

Options:
  --model <name>        Override the model (gpt-5, claude-opus, etc.)
  --read-only          Prevent file modifications
  --no-approval        Skip approval prompts (use with caution)
  --config <key=val>   Override config values
  --oss                Use local open source models
  --sandbox <mode>     Set sandbox level (read-only, workspace-write, etc.)
  --help              Show help information
  --debug             Log API requests and responses to file
  --version           Show version number
```

&ensp;
## Memory & project docs

Code can remember context across sessions:

1. **Create an `AGENTS.md` or `CLAUDE.md` file** in your project root:
```markdown
# Project Context
This is a React TypeScript application with:
- Authentication via JWT
- PostgreSQL database
- Express.js backend

## Key files:
- `/src/auth/` - Authentication logic
- `/src/api/` - API client code  
- `/server/` - Backend services
```

2. **Session memory**: Code maintains conversation history
3. **Codebase analysis**: Automatically understands project structure

&ensp;
## Non-interactive / CI mode

For automation and CI/CD:

```shell
# Run a specific task
code --no-approval "run tests and fix any failures"

# Generate reports
code --read-only "analyze code quality and generate report"

# Batch processing
code --config output_format=json "list all TODO comments"
```

&ensp;
## Model Context Protocol (MCP)

Code supports MCP for extended capabilities:

- **File operations**: Advanced file system access
- **Database connections**: Query and modify databases
- **API integrations**: Connect to external services
- **Custom tools**: Build your own extensions

Configure MCP in `~/.codex/config.toml`:

```toml
[[mcp_servers]]
name = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/project"]
```

&ensp;
## Configuration

Main config file: `~/.codex/config.toml`

```toml
# Model settings
model = "gpt-5"
model_provider = "openai"

# Behavior
approval_policy = "on_request"  # untrusted | on-failure | on-request | never
model_reasoning_effort = "medium" # low | medium | high
sandbox_mode = "workspace_write"

# UI preferences see THEME_CONFIG.md
[tui.theme]
name = "light-photon"

# Add config for specific models
[profiles.gpt-5]
model = "gpt-5"
model_provider = "openai"
approval_policy = "never"
model_reasoning_effort = "high"
model_reasoning_summary = "detailed"
```

### Environment variables

- `CODEX_HOME`: Override config directory location
- `OPENAI_API_KEY`: Use API key instead of ChatGPT auth
- `OPENAI_BASE_URL`: Use alternative API endpoints

&ensp;
## FAQ

**How is this different from the original?**
> This fork adds browser integration, multi-agent commands (`/plan`, `/solve`, `/code`), theme system, and enhanced reasoning controls while maintaining full compatibility.

**Can I use my existing Codex configuration?**
> Yes! This fork is fully backward compatible with existing `~/.codex/` configurations.

**Does this work with ChatGPT Plus?**
> Absolutely. Use the same "Sign in with ChatGPT" flow as the original.

**Is my data secure?**
> Yes. Authentication stays on your machine, and we don't proxy your credentials or conversations.

&ensp;
## Contributing

We welcome contributions! This fork maintains compatibility with upstream while adding community-requested features.

### Development workflow

```bash
# Clone and setup
git clone https://github.com/just-every/code.git
cd code
npm install

# Build (use fast build for development)
./build-fast.sh

# Run locally
./codex-rs/target/dev-fast/code
```

### Opening a pull request

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Make your changes
4. Run tests: `cargo test`
5. Build successfully: `./build-fast.sh`
6. Submit a pull request


&ensp;
## Legal & Use

### License & attribution
- This project is a community fork of `openai/codex` under **Apache-2.0**. We preserve upstream LICENSE and NOTICE files.
- **Code** is **not** affiliated with, sponsored by, or endorsed by OpenAI.

### Your responsibilities
Using OpenAI, Anthropic or Google services through Code means you agree to **their Terms and policies**. In particular:
- **Don't** programmatically scrape/extract content outside intended flows.
- **Don't** bypass or interfere with rate limits, quotas, or safety mitigations.
- Use your **own** account; don't share or rotate accounts to evade limits.
- If you configure other model providers, you're responsible for their terms.

### Privacy
- Your auth file lives at `~/.codex/auth.json`.
- Inputs/outputs you send to AI providers are handled under their Terms and Privacy Policy; consult those documents (and any org-level data-sharing settings).

### Subject to change
AI providers can change eligibility, limits, models, or authentication flows. Code supports **both** ChatGPT sign-in and API-key modes so you can pick what fits (local/hobby vs CI/automation).

&ensp;
## License

Apache 2.0 - See [LICENSE](LICENSE) file for details.

This project is a community fork of the original Codex CLI. We maintain compatibility while adding enhanced features requested by the developer community.

&ensp;
---
**Need help?** Open an issue on [GitHub](https://github.com/just-every/code/issues) or check our documentation.
