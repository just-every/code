# Config

<!-- markdownlint-disable MD012 MD013 MD028 MD033 -->

Codex supports several mechanisms for setting config values:

- Config-specific command-line flags, such as `--model o3` (highest precedence).
- A generic `-c`/`--config` flag that takes a `key=value` pair, such as `--config model="o3"`.
  - The key can contain dots to set a value deeper than the root, e.g. `--config model_providers.openai.wire_api="chat"`.
  - For consistency with `config.toml`, values are a string in TOML format rather than JSON format, so use `key='{a = 1, b = 2}'` rather than `key='{"a": 1, "b": 2}'`.
    - The quotes around the value are necessary, as without them your shell would split the config argument on spaces, resulting in `codex` receiving `-c key={a` with (invalid) additional arguments `=`, `1,`, `b`, `=`, `2}`.
  - Values can contain any TOML object, such as `--config shell_environment_policy.include_only='["PATH", "HOME", "USER"]'`.
  - If `value` cannot be parsed as a valid TOML value, it is treated as a string value. This means that `-c model='"o3"'` and `-c model=o3` are equivalent.
    - In the first case, the value is the TOML string `"o3"`, while in the second the value is `o3`, which is not valid TOML and therefore treated as the TOML string `"o3"`.
    - Because quotes are interpreted by one's shell, `-c key="true"` will be correctly interpreted in TOML as `key = true` (a boolean) and not `key = "true"` (a string). If for some reason you needed the string `"true"`, you would need to use `-c key='"true"'` (note the two sets of quotes).
- The `$CODE_HOME/config.toml` configuration file. `CODE_HOME` defaults to `~/.code`; Code also reads from `$CODEX_HOME`/`~/.codex` for backwards compatibility but only writes to `~/.code`. (Logs and other state use the same directory.)

Both the `--config` flag and the `config.toml` file support the following options:

## model

The model that Codex should use.

```toml
model = "o3"  # overrides the default of "gpt-5.1"
```

## model_providers

This option lets you override and amend the default set of model providers bundled with Codex. This value is a map where the key is the value to use with `model_provider` to select the corresponding provider.

For example, if you wanted to add a provider that uses the OpenAI 4o model via the chat completions API, then you could add the following configuration:

```toml
# Recall that in TOML, root keys must be listed before tables.
model = "gpt-4o"
model_provider = "openai-chat-completions"

[model_providers.openai-chat-completions]
# Name of the provider that will be displayed in the Codex UI.
name = "OpenAI using Chat Completions"
# The path `/chat/completions` will be amended to this URL to make the POST
# request for the chat completions.
base_url = "https://api.openai.com/v1"
# If `env_key` is set, identifies an environment variable that must be set when
# using Codex with this provider. The value of the environment variable must be
# non-empty and will be used in the `Bearer TOKEN` HTTP header for the POST request.
env_key = "OPENAI_API_KEY"
# Valid values for wire_api are "chat" and "responses". Defaults to "chat" if omitted.
wire_api = "chat"
# If necessary, extra query params that need to be added to the URL.
# See the Azure example below.
query_params = {}
```

Note this makes it possible to use Codex CLI with non-OpenAI models, so long as they use a wire API that is compatible with the OpenAI chat completions API. For example, you could define the following provider to use Codex CLI with Ollama running locally:

```toml
[model_providers.ollama]
name = "Ollama"
base_url = "http://localhost:11434/v1"
```

Or a third-party provider (using a distinct environment variable for the API key):

```toml
[model_providers.mistral]
name = "Mistral"
base_url = "https://api.mistral.ai/v1"
env_key = "MISTRAL_API_KEY"
```

It is also possible to configure a provider to include extra HTTP headers with a request. These can be hardcoded values (`http_headers`) or values read from environment variables (`env_http_headers`):

```toml
[model_providers.example]
# name, base_url, ...

# This will add the HTTP header `X-Example-Header` with value `example-value`
# to each request to the model provider.
http_headers = { "X-Example-Header" = "example-value" }

# This will add the HTTP header `X-Example-Features` with the value of the
# `EXAMPLE_FEATURES` environment variable to each request to the model provider
# _if_ the environment variable is set and its value is non-empty.
env_http_headers = { "X-Example-Features" = "EXAMPLE_FEATURES" }
```

### Azure model provider example

Note that Azure requires `api-version` to be passed as a query parameter, so be sure to specify it as part of `query_params` when defining the Azure provider:

```toml
[model_providers.azure]
name = "Azure"
# Make sure you set the appropriate subdomain for this URL.
base_url = "https://YOUR_PROJECT_NAME.openai.azure.com/openai"
env_key = "AZURE_OPENAI_API_KEY"  # Or "OPENAI_API_KEY", whichever you use.
query_params = { api-version = "2025-04-01-preview" }
wire_api = "responses"
```

Export your key before launching Codex: `export AZURE_OPENAI_API_KEY=…`

### Per-provider network tuning

The following optional settings control retry behaviour and streaming idle timeouts **per model provider**. They must be specified inside the corresponding `[model_providers.<id>]` block in `config.toml`. (Older releases accepted top‑level keys; those are now ignored.)

Example:

```toml
[model_providers.openai]
name = "OpenAI"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
# network tuning overrides (all optional; falls back to built‑in defaults)
request_max_retries = 4            # retry failed HTTP requests
stream_max_retries = 10            # retry dropped SSE streams
stream_idle_timeout_ms = 300000    # 5m idle timeout
```

#### request_max_retries

How many times Codex will retry a failed HTTP request to the model provider. Defaults to `4`.

#### stream_max_retries

Number of times Codex will attempt to reconnect when a streaming response is interrupted. Defaults to `5`.

#### stream_idle_timeout_ms

How long Codex will wait for activity on a streaming response before treating the connection as lost. Defaults to `300_000` (5 minutes).

## model_provider

Identifies which provider to use from the `model_providers` map. Defaults to `"openai"`. You can override the `base_url` for the built-in `openai` provider via the `OPENAI_BASE_URL` environment variable and force the wire protocol (`"responses"` or `"chat"`) with `OPENAI_WIRE_API`.

Note that if you override `model_provider`, then you likely want to override
`model`, as well. For example, if you are running ollama with Mistral locally,
then you would need to add the following to your config in addition to the new entry in the `model_providers` map:

```toml
model_provider = "ollama"
model = "mistral"
```

## approval_policy

Determines when the user should be prompted to approve whether Codex can execute a command:

```toml
# Codex has hardcoded logic that defines a set of "trusted" commands.
# Setting the approval_policy to `untrusted` means that Codex will prompt the
# user before running a command not in the "trusted" set.
#
# See https://github.com/openai/codex/issues/1260 for the plan to enable
# end-users to define their own trusted commands.
approval_policy = "untrusted"
```

If you want to be notified whenever a command fails, use "on-failure":

```toml
# If the command fails when run in the sandbox, Codex asks for permission to
# retry the command outside the sandbox.
approval_policy = "on-failure"
```

If you want the model to run until it decides that it needs to ask you for escalated permissions, use "on-request":

```toml
# The model decides when to escalate
approval_policy = "on-request"
```

Alternatively, you can have the model run until it is done, and never ask to run a command with escalated permissions:

```toml
# User is never prompted: if the command fails, Codex will automatically try
# something out. Note the `exec` subcommand always uses this mode.
approval_policy = "never"
```

## agents

Use `[[agents]]` blocks to register additional CLI programs that Codex can launch as peers. Each block maps a short `name` (referenced elsewhere in the config) to the command to execute, optional default flags, and environment variables.

> **Note:** Built-in model slugs (for example `code-gpt-5.1-codex`, `claude-sonnet-4.5`) automatically inject the correct `--model` or `-m` flag. To avoid conflicting arguments, Codex strips any `--model`/`-m` flags you place in `args`, `args_read_only`, or `args_write` before launching the agent. If you need a new model variant, add a slug in `code-rs/core/src/agent_defaults.rs` (or set an environment variable consumed by the CLI) rather than pinning the flag here.

```toml
[[agents]]
name = "context-collector"
command = "gemini"
enabled = true
read-only = true
description = "Gemini long-context helper that summarizes large repositories"
args = ["-y"]
env = { GEMINI_API_KEY = "..." }
```

When `enabled = true`, the agent is surfaced in the TUI picker and any sub-agent commands that reference it. Setting `read-only = true` forces the agent to request approval before modifying files even if the primary session permits writes.

## profiles

A _profile_ is a collection of configuration values that can be set together. Multiple profiles can be defined in `config.toml` and you can specify the one you
want to use at runtime via the `--profile` flag.

Here is an example of a `config.toml` that defines multiple profiles:

```toml
model = "o3"
approval_policy = "untrusted"
disable_response_storage = false

# Setting `profile` is equivalent to specifying `--profile o3` on the command
# line, though the `--profile` flag can still be used to override this value.
profile = "o3"

[model_providers.openai-chat-completions]
name = "OpenAI using Chat Completions"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "chat"

[profiles.o3]
model = "o3"
model_provider = "openai"
approval_policy = "never"
model_reasoning_effort = "high"
model_reasoning_summary = "detailed"

[profiles.gpt3]
model = "gpt-3.5-turbo"
model_provider = "openai-chat-completions"

[profiles.zdr]
model = "o3"
model_provider = "openai"
approval_policy = "on-failure"
disable_response_storage = true
```

Users can specify config values at multiple levels. Order of precedence is as follows:

1. custom command-line argument, e.g., `--model o3`
2. as part of a profile, where the `--profile` is specified via a CLI (or in the config file itself)
3. as an entry in `config.toml`, e.g., `model = "o3"`
4. the default value that comes with Codex CLI (i.e., Codex CLI defaults to `gpt-5.1`)

## model_reasoning_effort

If the selected model is known to support reasoning (for example: `o3`, `o4-mini`, `codex-*`, `gpt-5.1`, `gpt-5.1-codex`), reasoning is enabled by default when using the Responses API. As explained in the [OpenAI Platform documentation](https://platform.openai.com/docs/guides/reasoning?api-mode=responses#get-started-with-reasoning), this can be set to:

- `"minimal"`
- `"low"`
- `"medium"` (default)
- `"high"`

Note: to minimize reasoning, choose `"minimal"`.

## model_reasoning_summary

If the model name starts with `"o"` (as in `"o3"` or `"o4-mini"`) or `"codex"`, reasoning is enabled by default when using the Responses API. As explained in the [OpenAI Platform documentation](https://platform.openai.com/docs/guides/reasoning?api-mode=responses#reasoning-summaries), this can be set to:

- `"auto"` (default)
- `"concise"`
- `"detailed"`

To disable reasoning summaries, set `model_reasoning_summary` to `"none"` in your config:

```toml
model_reasoning_summary = "none"  # disable reasoning summaries
```

## model_verbosity

Controls output length/detail on GPT‑5 family models when using the Responses API. Supported values:

- `"low"`
- `"medium"` (default when omitted)
- `"high"`

When set, Codex includes a `text` object in the request payload with the configured verbosity, for example: `"text": { "verbosity": "low" }`.

Example:

```toml
model = "gpt-5.1"
model_verbosity = "low"
```

Note: This applies only to providers using the Responses API. Chat Completions providers are unaffected.

## model_supports_reasoning_summaries

By default, `reasoning` is only set on requests to OpenAI models that are known to support them. To force `reasoning` to set on requests to the current model, you can force this behavior by setting the following in `config.toml`:

```toml
model_supports_reasoning_summaries = true
```

## sandbox_mode

Codex executes model-generated shell commands inside an OS-level sandbox.

In most cases you can pick the desired behaviour with a single option:

```toml
# same as `--sandbox read-only`
sandbox_mode = "read-only"
```

The default policy is `read-only`, which means commands can read any file on
disk, but attempts to write a file or access the network will be blocked.

A more relaxed policy is `workspace-write`. When specified, the current working directory for the Codex task will be writable (as well as `$TMPDIR` on macOS). Note that the CLI defaults to using the directory where it was spawned as `cwd`, though this can be overridden using `--cwd/-C`.

Historically, Codex allowed writes inside the top‑level `.git/` folder when using `workspace-write`. That permissive behavior is the default again. If you want to protect `.git` under `workspace-write`, you can opt out via `[sandbox_workspace_write].allow_git_writes = false`.

```toml
# same as `--sandbox workspace-write`
sandbox_mode = "workspace-write"

# Extra settings that only apply when `sandbox = "workspace-write"`.
[sandbox_workspace_write]
# By default, the cwd for the Codex session will be writable as well as $TMPDIR
# (if set) and /tmp (if it exists). Setting the respective options to `true`
# will override those defaults.
exclude_tmpdir_env_var = false
exclude_slash_tmp = false

# Optional list of _additional_ writable roots beyond $TMPDIR and /tmp.

# Protect top-level .git under writable roots (default is true = allow writes)
allow_git_writes = true
writable_roots = ["/Users/YOU/.pyenv/shims"]

# Allow the command being run inside the sandbox to make outbound network
# requests. Disabled by default.
network_access = false
```

To disable sandboxing altogether, specify `danger-full-access` like so:

```toml
# same as `--sandbox danger-full-access`
sandbox_mode = "danger-full-access"
```

This is reasonable to use if Codex is running in an environment that provides its own sandboxing (such as a Docker container) such that further sandboxing is unnecessary.

Though using this option may also be necessary if you try to use Codex in environments where its native sandboxing mechanisms are unsupported, such as older Linux kernels or on Windows.

## Approval presets

Codex provides three main Approval Presets:

- Read Only: Codex can read files and answer questions; edits, running commands, and network access require approval.
- Auto: Codex can read files, make edits, and run commands in the workspace without approval; asks for approval outside the workspace or for network access.
- Full Access: Full disk and network access without prompts; extremely risky.

You can further customize how Codex runs at the command line using the `--ask-for-approval` and `--sandbox` options.

## MCP Servers

You can configure Codex to use [MCP servers](https://modelcontextprotocol.io/about) to give Codex access to external applications, resources, or services such as [Playwright](https://github.com/microsoft/playwright-mcp), [Figma](https://www.figma.com/blog/design-context-everywhere-you-build/), [documentation](https://context7.com/), and [more](https://github.com/mcp?utm_source=blog-source&utm_campaign=mcp-registry-server-launch-2025).

### Server transport configuration

Each server may set `startup_timeout_sec` to adjust how long Codex waits for it to start and respond to a tools listing. The default is `10` seconds.
Similarly, `tool_timeout_sec` limits how long individual tool calls may run (default: `60` seconds), and Codex will fall back to the default when this value is omitted.

This config option is comparable to how Claude and Cursor define `mcpServers` in their respective JSON config files, though because Codex uses TOML for its config language, the format is slightly different. For example, the following config in JSON:

```json
{
  "mcpServers": {
    "server-name": {
      "command": "npx",
      "args": ["-y", "mcp-server"],
      "env": {
        "API_KEY": "value"
      }
    }
  }
}
```

Should be represented as follows in `~/.code/config.toml` (Code will also read the legacy `~/.codex/config.toml` if it exists):

```toml
# The top-level table name must be `mcp_servers`
# The sub-table name (`server-name` in this example) can be anything you would like.
[mcp_servers.server-name]
command = "npx"
# Optional
args = ["-y", "mcp-server"]
# Optional: propagate additional env vars to the MVP server.
# A default whitelist of env vars will be propagated to the MCP server.
# https://github.com/openai/codex/blob/main/codex-rs/rmcp-client/src/utils.rs#L82
env = { "API_KEY" = "value" }
```

#### Streamable HTTP

```toml
# Streamable HTTP requires the experimental rmcp client
experimental_use_rmcp_client = true
[mcp_servers.figma]
url = "http://127.0.0.1:3845/mcp"
# Optional bearer token to be passed into an `Authorization: Bearer <token>` header
# Use this with caution because the token is in plaintext.
bearer_token = "<token>"
```

Refer to the MCP CLI commands for oauth login

### Other configuration options

```toml
# Optional: override the default 10s startup timeout
startup_timeout_sec = 20
# Optional: override the default 60s per-tool timeout
tool_timeout_sec = 30
```

## subagents

Sub-agents are orchestrated helper workflows you can trigger with slash commands (for example `/plan`, `/solve`, `/code`). Each entry under `[[subagents.commands]]` defines the slash command name, whether spawned agents run in read-only mode, which `agents` to launch, and extra guidance for both the orchestrator (Code) and the individual agents.

By default (when no `[[agents]]` are configured) Code advertises these model slugs for multi-agent runs: `code-gpt-5.1`, `claude-sonnet-4.5`, `claude-opus-4.1`, `gemini-2.5-pro`, `gemini-2.5-flash`, `qwen-3-coder`, `code-gpt-5.1-codex`, and `code-gpt-5.1-codex-mini`. The cloud counterpart, `cloud-gpt-5.1-codex`, only appears when `CODE_ENABLE_CLOUD_AGENT_MODEL=1` is set. You can override the list by defining `[[agents]]` entries or by specifying `agents = [ … ]` on a given `[[subagents.commands]]` entry.

```toml
[[subagents.commands]]
name = "context"
read-only = true
agents = ["context-collector", "code-gpt-5.1"]
orchestrator-instructions = "Coordinate a context sweep before coding. Ask each agent to emit concise, linked summaries of relevant files and tooling the primary task might need."
agent-instructions = "Summarize the repository areas most relevant to the user's request. List file paths, rationale, and suggested follow-up scripts to run. Keep the reply under 2,000 tokens."
```

With the example above you can run `/context` inside the TUI to create a summary cell that the main `/code` turn can reference later. Because `context-collector` is an ordinary agent, any command-line static analysis utilities it invokes (such as your blast radius tool) should be described in the `agent-instructions` so the orchestrator launches the right workflow. You can also customise the built-in commands by providing an entry with the same `name` (`plan`, `solve`, or `code`) and pointing their `agents` list at your long-context helper.

## validation

Controls the quick validation harness that runs before applying patches. The
harness now activates automatically whenever at least one validation group is
enabled. Use `[validation.groups]` for high-level toggles and the nested
`[validation.tools]` table for per-tool overrides:

```toml
[validation.groups]
functional = true
stylistic = false

[validation.tools]
shellcheck = true
markdownlint = true
hadolint = true
yamllint = true
cargo-check = true
tsc = true
eslint = true
mypy = true
pyright = true
phpstan = true
psalm = true
golangci-lint = true
shfmt = true
prettier = true
```

Functional checks stay enabled by default to catch regressions in the touched
code, while stylistic linters default to off so teams can opt in when they want
formatting feedback.

With functional checks enabled, Codex automatically detects the languages
affected by a patch and schedules the appropriate tools:

- `cargo-check` for Rust workspaces (scoped to touched manifests)
- `tsc --noEmit` and `eslint --max-warnings=0` for TypeScript/JavaScript files
- `mypy` and `pyright` for Python modules
- `phpstan`/`psalm` for PHP projects with matching config or Composer entries
- `golangci-lint run ./...` for Go modules alongside the existing JSON/TOML/YAML
  syntax checks

Each entry under `[validation.tools]` can be toggled to disable a specific tool
or to opt particular checks back in after disabling the entire group.

When enabled, Codex can also run `actionlint` against modified workflows. This
is configured under `[github]`:

```toml
[github]
actionlint_on_patch = true
# Optional: provide an explicit binary path
actionlint_path = "/usr/local/bin/actionlint"
```

## disable_response_storage

Currently, customers whose accounts are set to use Zero Data Retention (ZDR) must set `disable_response_storage` to `true` so that Codex uses an alternative to the Responses API that works with ZDR:

```toml
disable_response_storage = true
```

### Managing MCP servers from CLI (experimental)

You can also manage these entries from the CLI:

```shell
# Add a server (env can be repeated; `--` separates the launcher command)
codex mcp add docs -- docs-server --port 4000

# List configured servers (pretty table or JSON)
codex mcp list
codex mcp list --json

# Show one server (table or JSON)
codex mcp get docs
codex mcp get docs --json

# Remove a server
codex mcp remove docs

# Log in to a streamable HTTP server that supports oauth
codex mcp login SERVER_NAME

# Log out from a streamable HTTP server that supports oauth
codex mcp logout SERVER_NAME
```

## shell_environment_policy

Codex spawns subprocesses (e.g. when executing a `local_shell` tool-call suggested by the assistant). By default it now passes **your full environment** to those subprocesses. You can tune this behavior via the **`shell_environment_policy`** block in `config.toml`:

```toml
[shell_environment_policy]
# inherit can be "all" (default), "core", or "none"
inherit = "core"
# set to true to *skip* the filter for `"*KEY*"` and `"*TOKEN*"`
ignore_default_excludes = false
# exclude patterns (case-insensitive globs)
exclude = ["AWS_*", "AZURE_*"]
# force-set / override values
set = { CI = "1" }
# if provided, *only* vars matching these patterns are kept
include_only = ["PATH", "HOME"]
```

| Field                     | Type                 | Default | Description                                                                                                                                     |
| ------------------------- | -------------------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `inherit`                 | string               | `all`   | Starting template for the environment:<br>`all` (clone full parent env), `core` (`HOME`, `PATH`, `USER`, …), or `none` (start empty).           |
| `ignore_default_excludes` | boolean              | `false` | When `false`, Codex removes any var whose **name** contains `KEY`, `SECRET`, or `TOKEN` (case-insensitive) before other rules run.              |
| `exclude`                 | array<string>        | `[]`    | Case-insensitive glob patterns to drop after the default filter.<br>Examples: `"AWS_*"`, `"AZURE_*"`.                                           |
| `set`                     | table<string,string> | `{}`    | Explicit key/value overrides or additions – always win over inherited values.                                                                   |
| `include_only`            | array<string>        | `[]`    | If non-empty, a whitelist of patterns; only variables that match _one_ pattern survive the final step. (Generally used with `inherit = "all"`.) |

The patterns are **glob style**, not full regular expressions: `*` matches any
number of characters, `?` matches exactly one, and character classes like
`[A-Z]`/`[^0-9]` are supported. Matching is always **case-insensitive**. This
syntax is documented in code as `EnvironmentVariablePattern` (see
`core/src/config_types.rs`).

If you just need a clean slate with a few custom entries you can write:

```toml
[shell_environment_policy]
inherit = "none"
set = { PATH = "/usr/bin", MY_FLAG = "1" }
```

Currently, `CODEX_SANDBOX_NETWORK_DISABLED=1` is also added to the environment, assuming network is disabled. This is not configurable.

## otel

Codex can emit [OpenTelemetry](https://opentelemetry.io/) **log events** that
describe each run: outbound API requests, streamed responses, user input,
tool-approval decisions, and the result of every tool invocation. Export is
**disabled by default** so local runs remain self-contained. Opt in by adding an
`[otel]` table and choosing an exporter.

```toml
[otel]
environment = "staging"   # defaults to "dev"
exporter = "none"          # defaults to "none"; set to otlp-http or otlp-grpc to send events
log_user_prompt = false    # defaults to false; redact prompt text unless explicitly enabled
```

Codex tags every exported event with `service.name = $ORIGINATOR` (the same
value sent in the `originator` header, `codex_cli_rs` by default), the CLI
version, and an `env` attribute so downstream collectors can distinguish
dev/staging/prod traffic. Only telemetry produced inside the `codex_otel`
crate—the events listed below—is forwarded to the exporter.

### Event catalog

Every event shares a common set of metadata fields: `event.timestamp`,
`conversation.id`, `app.version`, `auth_mode` (when available),
`user.account_id` (when available), `terminal.type`, `model`, and `slug`.

With OTEL enabled Codex emits the following event types (in addition to the
metadata above):

- `codex.conversation_starts`
  - `provider_name`
  - `reasoning_effort` (optional)
  - `reasoning_summary`
  - `context_window` (optional)
  - `max_output_tokens` (optional)
  - `auto_compact_token_limit` (optional)
  - `approval_policy`
  - `sandbox_policy`
  - `mcp_servers` (comma-separated list)
  - `active_profile` (optional)
- `codex.api_request`
  - `attempt`
  - `duration_ms`
  - `http.response.status_code` (optional)
  - `error.message` (failures)
- `codex.sse_event`
  - `event.kind`
  - `duration_ms`
  - `error.message` (failures)
  - `input_token_count` (responses only)
  - `output_token_count` (responses only)
  - `cached_token_count` (responses only, optional)
  - `reasoning_token_count` (responses only, optional)
  - `tool_token_count` (responses only)
- `codex.user_prompt`
  - `prompt_length`
  - `prompt` (redacted unless `log_user_prompt = true`)
- `codex.tool_decision`
  - `tool_name`
  - `call_id`
  - `decision` (`approved`, `approved_for_session`, `denied`, or `abort`)
  - `source` (`config` or `user`)
- `codex.tool_result`
  - `tool_name`
  - `call_id` (optional)
  - `arguments` (optional)
  - `duration_ms` (execution time for the tool)
  - `success` (`"true"` or `"false"`)
  - `output`

These event shapes may change as we iterate.

### Choosing an exporter

Set `otel.exporter` to control where events go:

- `none` – leaves instrumentation active but skips exporting. This is the
  default.
- `otlp-http` – posts OTLP log records to an OTLP/HTTP collector. Specify the
  endpoint, protocol, and headers your collector expects:

  ```toml
  [otel]
  exporter = { otlp-http = {
    endpoint = "https://otel.example.com/v1/logs",
    protocol = "binary",
    headers = { "x-otlp-api-key" = "${OTLP_TOKEN}" }
  }}
  ```

- `otlp-grpc` – streams OTLP log records over gRPC. Provide the endpoint and any
  metadata headers:

  ```toml
  [otel]
  exporter = { otlp-grpc = {
    endpoint = "https://otel.example.com:4317",
    headers = { "x-otlp-meta" = "abc123" }
  }}
  ```

If the exporter is `none` nothing is written anywhere; otherwise you must run or point to your
own collector. All exporters run on a background batch worker that is flushed on
shutdown.

If you build Codex from source the OTEL crate is still behind an `otel` feature
flag; the official prebuilt binaries ship with the feature enabled. When the
feature is disabled the telemetry hooks become no-ops so the CLI continues to
function without the extra dependencies.

## notify

Specify a program that will be executed to get notified about events generated by Codex. Note that the program will receive the notification argument as a string of JSON, e.g.:

```json
{
  "type": "agent-turn-complete",
  "turn-id": "12345",
  "input-messages": ["Rename `foo` to `bar` and update the callsites."],
  "last-assistant-message": "Rename complete and verified `cargo build` succeeds."
}
```

The `"type"` property will always be set. Currently, `"agent-turn-complete"` is the only notification type that is supported.

As an example, here is a Python script that parses the JSON and decides whether to show a desktop push notification using [terminal-notifier](https://github.com/julienXX/terminal-notifier) on macOS:

```python
#!/usr/bin/env python3

import json
import subprocess
import sys


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: notify.py <NOTIFICATION_JSON>")
        return 1

    try:
        notification = json.loads(sys.argv[1])
    except json.JSONDecodeError:
        return 1

    match notification_type := notification.get("type"):
        case "agent-turn-complete":
            assistant_message = notification.get("last-assistant-message")
            if assistant_message:
                title = f"Codex: {assistant_message}"
            else:
                title = "Codex: Turn Complete!"
            input_messages = notification.get("input_messages", [])
            message = " ".join(input_messages)
            title += message
        case _:
            print(f"not sending a push notification for: {notification_type}")
            return 0

    subprocess.check_output(
        [
            "terminal-notifier",
            "-title",
            title,
            "-message",
            message,
            "-group",
            "codex",
            "-ignoreDnD",
            "-activate",
            "com.googlecode.iterm2",
        ]
    )

    return 0


if __name__ == "__main__":
    sys.exit(main())
```

To have Codex use this script for notifications, you would configure it via `notify` in `~/.code/config.toml` (legacy `~/.codex/config.toml` is still read) using the appropriate path to `notify.py` on your computer:

```toml
notify = ["python3", "/Users/mbolin/.codex/notify.py"]
```

> [!NOTE]
> Use `notify` for automation and integrations: Codex invokes your external program with a single JSON argument for each event, independent of the TUI. If you only want lightweight desktop notifications while using the TUI, prefer `tui.notifications`, which uses terminal escape codes and requires no external program. You can enable both; `tui.notifications` covers in‑TUI alerts (e.g., approval prompts), while `notify` is best for system‑level hooks or custom notifiers. Currently, `notify` emits only `agent-turn-complete`, whereas `tui.notifications` supports `agent-turn-complete` and `approval-requested` with optional filtering.

## history

By default, Codex CLI records messages sent to the model in `$CODEX_HOME/history.jsonl`. Note that on UNIX, the file permissions are set to `o600`, so it should only be readable and writable by the owner.

To disable this behavior, configure `[history]` as follows:

```toml
[history]
persistence = "none"  # "save-all" is the default value
```

## Context timeline preview

The structured environment context timeline (baseline + deltas + browser
snapshots) is gated behind the `CTX_UI` environment flag. Set `CTX_UI=1`
before launching Codex to exercise the preview flow. Outside of this
developer flag the classic `== System Status ==` payload remains in place.

## file_opener

Identifies the editor/URI scheme to use for hyperlinking citations in model output. If set, citations to files in the model output will be hyperlinked using the specified URI scheme so they can be ctrl/cmd-clicked from the terminal to open them.

For example, if the model output includes a reference such as `【F:/home/user/project/main.py†L42-L50】`, then this would be rewritten to link to the URI `vscode://file/home/user/project/main.py:42`.

Note this is **not** a general editor setting (like `$EDITOR`), as it only accepts a fixed set of values:

- `"vscode"` (default)
- `"vscode-insiders"`
- `"windsurf"`
- `"cursor"`
- `"none"` to explicitly disable this feature

Currently, `"vscode"` is the default, though Codex does not verify VS Code is installed. As such, `file_opener` may default to `"none"` or something else in the future.

## hide_agent_reasoning

Codex intermittently emits "reasoning" events that show the model's internal "thinking" before it produces a final answer. Some users may find these events distracting, especially in CI logs or minimal terminal output.

Setting `hide_agent_reasoning` to `true` suppresses these events in **both** the TUI as well as the headless `exec` sub-command:

```toml
hide_agent_reasoning = true   # defaults to false
```

## show_raw_agent_reasoning

Surfaces the model’s raw chain-of-thought ("raw reasoning content") when available.

Notes:

- Only takes effect if the selected model/provider actually emits raw reasoning content. Many models do not. When unsupported, this option has no visible effect.
- Raw reasoning may include intermediate thoughts or sensitive context. Enable only if acceptable for your workflow.

Example:

```toml
show_raw_agent_reasoning = true  # defaults to false
```

## model_context_window

The size of the context window for the model, in tokens.

In general, Codex knows the context window for the most common OpenAI models, but if you are using a new model with an old version of the Codex CLI, then you can use `model_context_window` to tell Codex what value to use to determine how much context is left during a conversation.

## model_max_output_tokens

This is analogous to `model_context_window`, but for the maximum number of output tokens for the model.

## project_doc_max_bytes

Maximum number of bytes to read from an `AGENTS.md` file to include in the instructions sent with the first turn of a session. Defaults to 32 KiB.

## project_doc_fallback_filenames

Ordered list of additional filenames to look for when `AGENTS.md` is missing at a given directory level. The CLI always checks `AGENTS.md` first; the configured fallbacks are tried in the order provided. This lets monorepos that already use alternate instruction files (for example, `CLAUDE.md`) work out of the box while you migrate to `AGENTS.md` over time.

```toml
project_doc_fallback_filenames = ["CLAUDE.md", ".exampleagentrules.md"]
```

We recommend migrating instructions to AGENTS.md; other filenames may reduce model performance.

## tui

Options that are specific to the TUI.

```toml
[tui]
# Send desktop notifications when approvals are required or a turn completes.
# Defaults to false.
notifications = true

# You can optionally filter to specific notification types.
# Available types are "agent-turn-complete" and "approval-requested".
notifications = [ "agent-turn-complete", "approval-requested" ]

# Enable desktop notifications for approval requests only
notifications = [ "approval-requested" ]
```

> [!NOTE]
> Codex emits desktop notifications using terminal escape codes. Not all terminals support these (notably, macOS Terminal.app and VS Code's terminal do not support custom notifications. iTerm2, Ghostty and WezTerm do support these notifications).

> [!NOTE]
> `tui.notifications` is built‑in and limited to the TUI session. For programmatic or cross‑environment notifications—or to integrate with OS‑specific notifiers—use the top-level `notify` option to run an external program that receives event JSON. The two settings are independent and can be used together.

### Auto Drive Observer

Codex keeps long-running Auto Drive sessions in check with a lightweight observer thread. Configure its cadence with the top-level `auto_drive_observer_cadence` key (default `5`). After every *n* completed requests the observer reviews the coordinator/CLI transcript, emits telemetry, and—if necessary—suggests a corrected prompt or follow-up guidance. Setting the value to `0` disables the observer entirely.

```toml
# Run the observer after every third Auto Drive request
auto_drive_observer_cadence = 3
```

When the observer reports `status = "failing"`, the TUI banner highlights the intervention, updates the pending prompt when provided, and records guidance for future coordinator turns.

## Project Hooks

Use the `[projects]` table to scope settings to a specific workspace path. In addition to `trust_level`, `approval_policy`, and `always_allow_commands`, you can attach lifecycle hooks that run commands automatically when notable events occur.

```toml
[projects."/Users/me/src/my-app"]
trust_level = "trusted"

[[projects."/Users/me/src/my-app".hooks]]
name = "bootstrap"
event = "session.start"
run = ["./scripts/bootstrap.sh"]
timeout_ms = 60000

[[projects."/Users/me/src/my-app".hooks]]
event = "tool.after"
run = "npm run lint -- --changed"
```

Supported hook events:

- `session.start`: after the session is configured (once per launch)
- `session.end`: before shutdown completes
- `tool.before`: immediately before each exec/tool command runs
- `tool.after`: once an exec/tool command finishes (regardless of exit code)
- `file.before_write`: right before an `apply_patch` is applied
- `file.after_write`: after an `apply_patch` completes and diffs are emitted

Hook commands run inside the same sandbox mode as the session and appear in the TUI as their own exec cells. Failures are surfaced as background events but do not block the main task. Each invocation receives environment variables such as `CODE_HOOK_EVENT`, `CODE_HOOK_NAME`, `CODE_HOOK_INDEX`, `CODE_HOOK_CALL_ID`, `CODE_HOOK_PAYLOAD` (JSON describing the context), `CODE_SESSION_CWD`, and—when applicable—`CODE_HOOK_SOURCE_CALL_ID`. Hooks may also set `cwd`, provide additional `env` entries, and specify `timeout_ms`.

Example `tool.after` payload:

```json
{
  "event": "tool.after",
  "call_id": "tool_12",
  "cwd": "/Users/me/src/my-app",
  "command": ["npm", "test"],
  "exit_code": 1,
  "duration_ms": 1832,
  "stdout": "…output truncated…",
  "stderr": "…",
  "timed_out": false
}
```

## Project Commands

Define project-scoped commands under `[[projects."<path>".commands]]`. Each command needs a unique `name` and either an array (`command`) or string (`run`) describing how to invoke it. Optional fields include `description`, `cwd`, `env`, and `timeout_ms`.

```toml
[[projects."/Users/me/src/my-app".commands]]
name = "setup"
description = "Install dependencies"
run = ["pnpm", "install"]

[[projects."/Users/me/src/my-app".commands]]
name = "unit"
run = "cargo test --lib"
```

Project commands appear in the TUI via `/cmd <name>` and run through the standard execution pipeline. During execution Codex sets `CODE_PROJECT_COMMAND_NAME`, `CODE_PROJECT_COMMAND_DESCRIPTION` (when provided), and `CODE_SESSION_CWD` so scripts can tailor their behaviour.

## Config reference

| Key | Type / Values | Notes |
| --- | --- | --- |
| `model` | string | Model to use (e.g., `gpt-5.1-codex`). |
| `model_provider` | string | Provider id from `model_providers` (default: `openai`). |
| `model_context_window` | number | Context window tokens. |
| `model_max_output_tokens` | number | Max output tokens. |
| `approval_policy` | `untrusted` \| `on-failure` \| `on-request` \| `never` | When to prompt for approval. |
| `sandbox_mode` | `read-only` \| `workspace-write` \| `danger-full-access` | OS sandbox policy. |
| `sandbox_workspace_write.writable_roots` | array<string> | Extra writable roots in workspace‑write. |
| `sandbox_workspace_write.network_access` | boolean | Allow network in workspace‑write (default: false). |
| `sandbox_workspace_write.exclude_tmpdir_env_var` | boolean | Exclude `$TMPDIR` from writable roots (default: false). |
| `sandbox_workspace_write.exclude_slash_tmp` | boolean | Exclude `/tmp` from writable roots (default: false). |
| `disable_response_storage` | boolean | Required for ZDR orgs. |
| `notify` | array<string> | External program for notifications. |
| `instructions` | string | Currently ignored; use `experimental_instructions_file` or `AGENTS.md`. |
| `mcp_servers.<id>.command` | string | MCP server launcher command. |
| `mcp_servers.<id>.args` | array<string> | MCP server args. |
| `mcp_servers.<id>.env` | map<string,string> | MCP server env vars. |
| `mcp_servers.<id>.startup_timeout_sec` | number | Startup timeout in seconds (default: 10). Timeout is applied both for initializing MCP server and initially listing tools. |
| `mcp_servers.<id>.tool_timeout_sec` | number | Per-tool timeout in seconds (default: 60). Accepts fractional values; omit to use the default. |
| `model_providers.<id>.name` | string | Display name. |
| `model_providers.<id>.base_url` | string | API base URL. |
| `model_providers.<id>.env_key` | string | Env var for API key. |
| `model_providers.<id>.wire_api` | `chat` \| `responses` | Protocol used (default: `chat`). |
| `model_providers.<id>.query_params` | map<string,string> | Extra query params (e.g., Azure `api-version`). |
| `model_providers.<id>.http_headers` | map<string,string> | Additional static headers. |
| `model_providers.<id>.env_http_headers` | map<string,string> | Headers sourced from env vars. |
| `model_providers.<id>.request_max_retries` | number | Per‑provider HTTP retry count (default: 4). |
| `model_providers.<id>.stream_max_retries` | number | SSE stream retry count (default: 5). |
| `model_providers.<id>.stream_idle_timeout_ms` | number | SSE idle timeout (ms) (default: 300000). |
| `project_doc_max_bytes` | number | Max bytes to read from `AGENTS.md`. |
| `projects.<path>.trust_level` | string | Mark project/worktree as trusted (only `"trusted"` is recognized). |
| `projects.<path>.hooks` | array<table> | Lifecycle hooks for that workspace (see "Project Hooks"). |
| `projects.<path>.commands` | array<table> | Project commands exposed via `/cmd`. |
| `profile` | string | Active profile name. |
| `profiles.<name>.*` | various | Profile‑scoped overrides of the same keys. |
| `history.persistence` | `save-all` \| `none` | History file persistence (default: `save-all`). |
| `history.max_bytes` | number | Currently ignored (not enforced). |
| `file_opener` | `vscode` \| `vscode-insiders` \| `windsurf` \| `cursor` \| `none` | URI scheme for clickable citations (default: `vscode`). |
| `tui` | table | TUI‑specific options. |
| `tui.notifications` | boolean \| array<string> | Enable desktop notifications in the tui (default: false). |
| `hide_agent_reasoning` | boolean | Hide model reasoning events. |
| `show_raw_agent_reasoning` | boolean | Show raw reasoning (when available). |
| `model_reasoning_effort` | `minimal` \| `low` \| `medium` \| `high` | Responses API reasoning effort. |
| `model_reasoning_summary` | `auto` \| `concise` \| `detailed` \| `none` | Reasoning summaries. |
| `model_verbosity` | `low` \| `medium` \| `high` | GPT‑5 text verbosity (Responses API). |
| `model_supports_reasoning_summaries` | boolean | Force‑enable reasoning summaries. |
| `chatgpt_base_url` | string | Base URL for ChatGPT auth flow. |
| `experimental_resume` | string (path) | Resume JSONL path (internal/experimental). |
| `experimental_instructions_file` | string (path) | Replace built‑in instructions (experimental). |
| `experimental_use_exec_command_tool` | boolean | Use experimental exec command tool. |
| `use_experimental_reasoning_summary` | boolean | Use experimental summary for reasoning chain. |
| `responses_originator_header_internal_override` | string | Override `originator` header value. |
| `tools.web_search` | boolean | Enable web search tool (alias: `web_search_request`) (default: false). |
| `tools.web_search_allowed_domains` | array<string> | Optional allow-list for web search (filters.allowed_domains). |

<!-- markdownlint-enable MD012 MD013 MD028 MD033 -->
