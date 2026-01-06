# Non-interactive mode

Use Every Code in non-interactive mode to automate common workflows.

```shell
code exec "count the total number of lines of code in this project"
```

In non-interactive mode, Code does not ask for command or edit approvals. By default it runs in `read-only` mode, so it cannot edit files or run commands that require network access.

Use `code exec --full-auto` to allow file edits. Use `code exec --sandbox danger-full-access` to allow edits and networked commands.

### Default output mode

By default, Code streams its activity to stderr and only writes the final message from the agent to stdout. This makes it easier to pipe `code exec` into another tool without extra filtering.

To write the output of `code exec` to a file, in addition to using a shell redirect like `>`, there is also a dedicated flag to specify an output file: `-o`/`--output-last-message`.

### JSON output mode

`code exec` supports a `--json` mode that streams events to stdout as JSON Lines (JSONL) while the agent runs.

Supported event types:

- `thread.started` - when a thread is started or resumed.
- `turn.started` - when a turn starts. A turn encompasses all events between the user message and the assistant response.
- `turn.completed` - when a turn completes; includes token usage.
- `turn.failed` - when a turn fails; includes error details.
- `item.started`/`item.updated`/`item.completed` - when a thread item is added/updated/completed.

Supported item types:

- `assistant_message` - assistant message.
- `reasoning` - a summary of the assistant's thinking.
- `command_execution` - assistant executing a command.
- `file_change` - assistant making file changes.
- `mcp_tool_call` - assistant calling an MCP tool.
- `web_search` - assistant performing a web search.

Typically, an `assistant_message` is added at the end of the turn.

Sample output:

```jsonl
{"type":"thread.started","thread_id":"0199a213-81c0-7800-8aa1-bbab2a035a53"}
{"type":"turn.started"}
{"type":"item.completed","item":{"id":"item_0","item_type":"reasoning","text":"**Searching for README files**"}}
{"type":"item.started","item":{"id":"item_1","item_type":"command_execution","command":"bash -lc ls","aggregated_output":"","status":"in_progress"}}
{"type":"item.completed","item":{"id":"item_1","item_type":"command_execution","command":"bash -lc ls","aggregated_output":"AGENTS.md\nCHANGELOG.md\nREADME.md\ncode-rs\ncodex-rs\ncodex-cli\ndocs\nscripts\nsdk\n","exit_code":0,"status":"completed"}}
{"type":"item.completed","item":{"id":"item_2","item_type":"reasoning","text":"**Checking repository root for README**"}}
{"type":"item.completed","item":{"id":"item_3","item_type":"assistant_message","text":"Yep — there’s a `README.md` in the repository root."}}
{"type":"turn.completed","usage":{"input_tokens":24763,"cached_input_tokens":24448,"output_tokens":122}}
```

### Structured output

By default, the agent responds with natural language. Use `--output-schema` to provide a JSON Schema that defines the expected JSON output.

The JSON Schema must follow the [strict schema rules](https://platform.openai.com/docs/guides/structured-outputs).

Sample schema:

```json
{
  "type": "object",
  "properties": {
    "project_name": { "type": "string" },
    "programming_languages": { "type": "array", "items": { "type": "string" } }
  },
  "required": ["project_name", "programming_languages"],
  "additionalProperties": false
}
```

```shell
code exec "Extract details of the project" --output-schema ~/schema.json
...

{"project_name":"Every Code CLI","programming_languages":["Rust","TypeScript","Shell"]}
```

Combine `--output-schema` with `-o` to only print the final JSON output. You can also pass a file path to `-o` to save the JSON output to a file.

### Git repository requirement

Code requires a Git repository to avoid destructive changes. To disable this check, use `code exec --skip-git-repo-check`.

### Resuming non-interactive sessions

Resume a previous non-interactive session with `code exec resume <SESSION_ID>` or `code exec resume --last`. This preserves conversation context so you can ask follow-up questions or give new tasks to the agent.

```shell
code exec "Review the change, look for use-after-free issues"
code exec resume --last "Fix use-after-free issues"
```

Only the conversation context is preserved; you must still provide flags to customize Code behavior.

```shell
code exec --model gpt-5.1-codex --json "Review the change, look for use-after-free issues"
code exec --model gpt-5.1 --json resume --last "Fix use-after-free issues"
```

## Authentication

By default, `code exec` uses the same authentication method as the TUI and VSCode extension. You can override the API key by setting the `CODEX_API_KEY` environment variable.

```shell
CODEX_API_KEY=your-api-key-here code exec "Fix merge conflict"
```

NOTE: `CODEX_API_KEY` is only supported in `code exec`.
