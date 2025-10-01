# History State JSON Schema

This document captures the canonical JSON schema for the serialized history
state used by the TUI. Each record stored in `HistoryState.records` is a
tagged `HistoryRecord` enum. The schema follows the common structure below:

```json
{
  "id": <number>,
  "type": <string>,
  "payload": <variant specific object>
}
```

* `id` — 64-bit unsigned integer derived from `HistoryId`.
* `type` — string enum discriminant (e.g. `"plain_message"`).
* `payload` — variant data described in the sections that follow.

All timestamps are serialized as RFC 3339 strings, durations as ISO 8601
duration strings, and enums as snake_case strings unless otherwise noted.

---

## Variant Reference

### PlainMessage
- `type`: `"plain_message"`
- `payload`:
  - `role`: `"system" | "user" | "assistant" | "tool" | "error" | "background_event"`
  - `kind`: `"plain" | "user" | "assistant" | "tool" | "error" | "background" | "notice"`
  - `header`: optional object `{ "label": string, "badge": string? }`
  - `lines`: array of `MessageLine` objects
  - `metadata`: optional `MessageMetadata`

`MessageLine` objects:
- `kind`: `"paragraph" | "bullet" | "code" | "quote" | "separator" | "metadata" | "blank"`
- `spans`: array of `{ "text": string, "tone": TextTone, "emphasis": Emphasis, "entity": Entity? }`

`MessageMetadata`:
- `citations`: array of citation strings
- `token_usage`: optional `{ "prompt": number, "completion": number, "total": number }`

`TextTone`: `"default" | "dim" | "primary" | "success" | "warning" | "error" | "info"`

`Emphasis`: `{ "bold": bool, "italic": bool, "dim": bool, "strike": bool, "underline": bool }`

`Entity`: `null | { "type": "link", "href": string } | { "type": "code" }`

### WaitStatus
- `type`: `"wait_status"`
- `payload`:
  - `header`: `{ "title": string, "title_tone": TextTone, "summary": string?, "summary_tone": TextTone }`
  - `details`: array of `{ "label": string, "value": string?, "tone": TextTone }`

### Loading
- `type`: `"loading"`
- `payload`: `{ "message": string }`

### RunningTool
- `type`: `"running_tool"`
- `payload`:
  - `title`: string
  - `started_at`: timestamp
  - `arguments`: `ToolArgument[]`
  - `wait_cap_ms`: number?
  - `wait_has_target`: bool
  - `wait_has_call_id`: bool

### ToolCall
- `type`: `"tool_call"`
- `payload`:
  - `status`: `"running" | "success" | "failed"`
  - `title`: string
  - `duration_ms`: number?
  - `arguments`: `ToolArgument[]`
  - `result_preview`: `{ "lines": string[], "truncated": bool }?`
  - `error_message`: string?

`ToolArgument`:
- `{ "name": string, "value": Text | Json | Secret }`
- `Text`: `{ "type": "text", "text": string }`
- `Json`: `{ "type": "json", "value": any }`
- `Secret`: `{ "type": "secret" }`

### PlanUpdate
- `type`: `"plan_update"`
- `payload`:
  - `name`: string
  - `icon`: `"light_bulb" | "rocket" | "clipboard" | { "custom": string }`
  - `progress`: `{ "completed": number, "total": number }`
  - `steps`: array of `{ "description": string, "status": StepStatus }`

`StepStatus`: `"pending" | "in_progress" | "complete" | "skipped"`

### UpgradeNotice
- `type`: `"upgrade_notice"`
- `payload`: `{ "current_version": string, "latest_version": string, "message": string }`

### Reasoning
- `type`: `"reasoning"`
- `payload`:
  - `in_progress`: bool
  - `sections`: array of `{ "heading": string?, "summary": InlineSpan[], "blocks": ReasoningBlock[] }`
  - `hide_when_collapsed`: bool

`ReasoningBlock`: one of
- `{ "type": "paragraph", "spans": InlineSpan[] }`
- `{ "type": "bullet", "indent": number, "marker": string, "spans": InlineSpan[] }`
- `{ "type": "code", "language": string?, "content": string }`
- `{ "type": "quote", "spans": InlineSpan[] }`
- `{ "type": "separator" }`

### Exec
- `type`: `"exec"`
- `payload`:
  - `call_id`: string?
  - `command`: string[]
  - `parsed`: array of `ParsedCommand`
  - `action`: `"read" | "search" | "list" | "run"`
  - `status`: `"running" | "success" | "error"`
  - `stdout_chunks`: array of `{ "offset": number, "content": string }`
  - `stderr_chunks`: array of `{ "offset": number, "content": string }`
  - `exit_code`: number?
  - `wait_total_ms`: number?
  - `wait_active`: bool
  - `wait_notes`: array of `{ "message": string, "tone": TextTone, "timestamp": timestamp }`
  - `started_at`: timestamp
  - `completed_at`: timestamp?

`ParsedCommand` mirrors the parsed shell command structure emitted by
`codex_core::parse_command`.

### AssistantStream
- `type`: `"assistant_stream"`
- `payload`:
  - `stream_id`: string
  - `preview_markdown`: string
  - `deltas`: array of `{ "delta": string, "sequence": number?, "received_at": timestamp }`
  - `citations`: string[]
  - `metadata`: `MessageMetadata?`
  - `in_progress`: bool
  - `last_updated_at`: timestamp

### AssistantMessage
- `type`: `"assistant_message"`
- `payload`:
  - `stream_id`: string?
  - `markdown`: string
  - `citations`: string[]
  - `metadata`: `MessageMetadata?`
  - `token_usage`: `{ "prompt": number, "completion": number, "total": number }?`
  - `created_at`: timestamp

### Diff
- `type`: `"diff"`
- `payload`:
  - `hunks`: array of `{ "header": string, "lines": DiffLine[] }`

`DiffLine`: `{ "kind": "context" | "added" | "removed", "text": string }`

### Image
- `type`: `"image"`
- `payload`: `{ "path": string, "caption": string?, "metadata": object? }`

### Explore
- `type`: `"explore"`
- `payload`: `{ "entries": ExploreEntry[], "trailing": bool }`

`ExploreEntry`: `{ "command": string[], "cwd": string?, "status": ExploreStatus }`

`ExploreStatus`: `"running" | "success" | "not_found" | { "error": { "exit_code": number? } }`

### RateLimits
- `type`: `"rate_limits"`
- `payload`: mirrors `RateLimitSnapshotEvent` with primary/secondary usage percentages
  and reset timestamps.

### Patch
- `type`: `"patch"`
- `payload`:
  - `event`: `"apply_begin" | "apply_success" | "apply_failure" | "proposed"`
  - `auto_approved`: bool?
  - `changes`: map of path → `FileChange`
  - `failure`: `{ "message": string, "stdout_excerpt": string?, "stderr_excerpt": string? }?`

### BackgroundEvent
- `type`: `"background_event"`
- `payload`: `{ "title": string, "description": string }`

### Notice
- `type`: `"notice"`
- `payload`: `{ "title": string?, "body": MessageLine[] }`

---

## Snapshot Structure

A serialized history snapshot is the object returned by
`HistoryState::snapshot()`:

```json
{
  "records": [ <HistoryRecord>... ],
  "next_id": <number>,
  "exec_call_lookup": { "call_id": HistoryId },
  "tool_call_lookup": { "call_id": HistoryId },
  "stream_lookup": { "stream_id": HistoryId }
}
```

Applications consuming the schema should treat unknown fields as forward
compatible extensions and must not rely on enum ordering.
