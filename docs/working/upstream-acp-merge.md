# Upstream ACP Integration – Design Notes

## Context

Upstream PR [openai/codex#1707](https://github.com/openai/codex/pull/1707) introduces an
experimental Agent Client Protocol (ACP) bridge so Zed can drive Codex via the
`session/new` and `session/prompt` tool calls. The branch (fetched locally as
`upstream-pr-1707`) rewrites large portions of `code-rs/core`, the MCP server,
and the TypeScript CLI/TUI to accommodate the new workflow.

### Current Status (2025-09-22)

- **Core** – Apply-patch execution now runs in-process with the ACP filesystem
  shim, preserving Code’s validation harness and approval prompts while
  emitting ACP tool-call metadata for downstream consumers.
- **MCP Server** – `session/new` and `session/prompt` are available alongside
  existing Codex tools. The new `acp_tool_runner` bridges Codex events to ACP
  `session/update` notifications and reuses the existing conversation manager to track live
  sessions.
- **Configuration** – `ConfigOverrides` and the TOML schema understand
  `experimental_client_tools` plus inline MCP server definitions, allowing IDE
  clients to announce their ACP capabilities without dropping legacy settings.
- **Validation** – `./build-fast.sh` passes after the integration, and new MCP
  tests cover ACP list-tool discovery plus a prompt roundtrip.

Code diverges substantially from upstream in these same areas: we retain the
Rust TUI, trimmed CLI scripts, and extended core features such as confirm
guards, queued user input, plan tool updates, and sandbox policies tuned for our
workflow. Directly rebasing onto upstream would drop or regress many of these
capabilities.

## What Upstream Added

- **Core (`code-rs/core`)**
  - New `acp.rs` module with helpers for translating Codex exec/patch events
    into ACP `ToolCall` updates, including an ACP-backed `FileSystem` shim and a
    permission-request flow (`acp::request_permission`).
  - `codex.rs` rewritten around `agent_client_protocol`, removing many existing
    operations (`QueueUserInput`, `RegisterApprovedCommand`, validation toggles,
    etc.) and introducing new event wiring for ACP tool calls.
  - `protocol.rs` pared down to match the simplified upstream surface (fewer
    `Op` variants, different `AskForApproval` options, new MCP call events).
- **MCP server (`code-rs/mcp-server`)**
  - New `acp_tool_runner.rs` that spawns Codex sessions on demand, relays MCP
    notifications, and surfaces ACP updates.
  - `message_processor.rs` extended to expose `session/new` and
    `session/prompt`, and to translate Codex events into ACP notifications (`session/update`).
- **CLI/TUI (Node + Rust)**
  - TypeScript terminal UI completely replaced with ACP-first experience.
  - Rust TUI and associated tests removed.

## Code-Specific Functionality We Must Preserve

- **Protocol & Ops** – `QueueUserInput`, validation toggles, confirm guards,
  rich approval semantics (`ReviewDecision::ApprovedForSession`), and sandbox
  policy options currently in `code-rs/core/src/protocol.rs:1`.
- **Execution Safety & Logging** – Confirm-guard enforcement and the richer
  `EventMsg` variants emitted from our `code-rs/core/src/codex.rs:1`.
- **TUI** – Entire Rust TUI stack (`code-rs/tui/src/chatwidget.rs:1`,
  `code-rs/tui/src/history_cell.rs:1`, etc.) must remain functional and ignore
  unknown ACP events gracefully.
- **CLI Footprint** – Our trimmed `codex-cli` structure and scripts differ from
  upstream’s wholesale replacement; we will not adopt the TypeScript overhaul.
- **Config Schema** – Existing TOML fields (confirm guards, validation toggles,
  sandbox defaults) must stay intact.

## Integration Approach

1. **Introduce ACP Helpers Without Regressions**
   - Port `code-rs/core/src/acp.rs:1` (and dependent structs) into Code, but
     adapt it to reuse the current `FileChange` representations and respect
     confirm guard / approval flows.
   - Extend `code-rs/core/src/codex.rs:1` to emit ACP events while preserving
     the existing `Op` variants, queueing logic, and validation toggles.
   - Update `code-rs/core/src/util.rs:1`, `code-rs/core/src/apply_patch.rs:1`,
     and related modules so ACP tool-call generators can derive the same
     metadata our TUI already consumes.

2. **MCP Server Wiring**
   - Add `code-rs/mcp-server/src/acp_tool_runner.rs:1` and integrate it with
     `message_processor.rs:1`, ensuring we keep Code-specific auth/sandbox setup
     and error reporting.
   - Maintain existing MCP tools and ensure ACP tools are opt-in (guarded by
     config or feature flag) so Code retains current behavior when ACP is
     unused.

3. **Event & Frontend Compatibility**
   - Extend our event enums (`code-rs/core/src/protocol.rs:1`) with any new
     variants required by ACP while keeping deprecated ones for backward
     compatibility.
   - Teach the Rust TUI to ignore or minimally display ACP-specific events so
     terminal UX does not panic when ACP notifications flow through.

4. **Config & Build**
   - Add `agent-client-protocol` dependency where needed, updating
     `code-rs/core/Cargo.toml:1`, `code-rs/mcp-server/Cargo.toml:1`, and
     `Cargo.lock`.
   - Introduce configuration toggles (if any) in `code-rs/core/src/config.rs:1`
     and `code-rs/core/src/config_types.rs:1` without breaking existing TOML
     files.
   - Update documentation (targeted sections in `docs/experimental.md:1` or a
     new page) to describe ACP/Zed support plus Code-specific caveats.

5. **Testing & Validation**
   - Add focused tests covering ACP request flow (unit-level in core and MCP
     server). Reuse existing harnesses (e.g., `code-rs/mcp-server/tests`) to
     simulate a session.
   - Validate end-to-end via `./build-fast.sh` only, honoring our policy against
     running additional formatters automatically.

## Open Questions

- How should ACP be surfaced in Code’s configuration (auto-enabled vs per
  server flag)?
- Do we expose ACP status in the Rust TUI, or treat it as headless-only? (Lean
  toward headless-first with optional UI indicators.)
- Are there additional permission flows (e.g., review approvals) required for
  Zed beyond what `ReviewDecision` already provides?

Document last updated: 2025-09-22.
