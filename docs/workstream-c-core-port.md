# Workstream C – Core Port Notes (2025-11-15)

This log captures the incremental plan, trade-offs, and open questions while
porting upstream `core` crate changes into the forked `code-rs` tree.

## Token Usage / Rate Limits

- **Current fork status**: `code-rs/core/src/account_usage.rs` already embeds the
  full token aggregation and rate-limit tracking stack (hourly → daily → monthly
  buckets, rate-limit snapshots, warning dedupe). Upstream `codex-rs` still
  lacks this module, so the fork remains ahead.
- **Port delta**: Added regression tests covering the new
  `record_usage_limit_hint` path and the compaction behaviour so we can safely
  align with future upstream shapes. This gives us parity with the behaviour
  highlighted in `core-critical.md` while keeping the implementation fork-only
  for now.
- **Trade-off**: Instead of moving the feature into `codex-rs`, we keep storing
  usage locally to avoid regressing CLI telemetry. Downside is extra drift vs.
  upstream until they ship a matching feature set.
- **Open questions**:
  - Should we expose a `TokenUsageStore` abstraction so the upcoming agent
    lifecycle refactor can log usage without duplicating account lookups?
  - Do we need to rename the managed warning log (`rate_limit_warnings.log`)
    when we eventually allow per-account purge tooling?

## Config Loader

- **Strategy**: Adopt the upstream layered loader (base `config.toml` +
  managed_config + managed preferences) verbatim via a new `config_loader`
  module, then reuse it inside `Config::load`.
- **Implementation**: Copied the upstream module, added synchronous wrappers so
  the existing blocking callers can drive the async loader (with a dedicated
  helper that spawns a temporary Tokio runtime whenever we’re already inside
  one), and switched the system-managed path to `/etc/code/managed_config.toml`
  to match the fork’s branding.
- **Trade-offs**:
  - We spin up a lightweight Tokio runtime when no runtime exists. This keeps
    behaviour deterministic but does introduce a small startup cost. Follow-up
    option: thread the loader through existing async contexts.
  - macOS managed preferences still use the upstream bundle identifier
    (`com.openai.codex`). We need product input on whether enterprises expect a
    new ID (e.g., `com.just-every.code`).
- **Open questions**:
  - How should CODEX_HOME overrides interact with `/etc/code/managed_config` on
    Linux? Upstream hard-codes `/etc/codex`; we mirrored that with `/etc/code`.
  - Do we need to support device-profile provisioning on non-macOS platforms
    sooner than upstream?

## Agent Lifecycle Events

- **Status**: Investigation is in progress; upstream wires richer
  `AgentStatusUpdateEvent` payloads through `AgentManager` and `TurnContext`.
  We haven’t ported these yet because token usage + config loader work took
  priority.
- **Plan**:
  1. Diff `codex-rs/core/src/agent_tool.rs` vs. the fork to map the new enums
     (`AgentStatus::Initializing`, etc.).
  2. Update `code-rs` protocol models and event emitters to match, ensuring the
     TUI keeps strict ordering guarantees.
  3. Extend the VT100 snapshot tests to assert the additional lifecycle rows.
- **Open questions**:
  - Upstream stamps events with per-turn IDs via `TurnContext`; we need to
    confirm whether our Auto Drive overrides still work when those IDs change.
  - Should fork-specific UX (browser approvals, theming) emit extra fields on
    the new events, or stick to the upstream schema for now?
