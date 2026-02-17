# Hermia Coder Ecosystem v1.0 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform the `just-every/code` fork into a fully branded Hermia Coder ecosystem powered by a local vLLM fleet (10 services, 8 GPUs, 784 GB VRAM, zero cloud), with three interfaces: Terminal CLI, Desktop GUI, and Messaging Gateway.

**Architecture:** The core change is adding an HTTP-native agent execution path to `agent_tool.rs` so that `[[agents]]` entries with an `http_endpoint` field call `stream_chat_completions()` directly against local vLLM endpoints instead of spawning CLI subprocesses. Everything else (branding, HCC metrics, Desktop GUI, PicoClaw gateway) builds on top of this foundation.

**Tech Stack:** Rust (core CLI), TypeScript/Electron (Desktop GUI), Go (PicoClaw), WebSocket (HCC metrics), vLLM (model serving), systemd (service management)

**Source document:** `Hermia_Coder_Ecosystem_Execution_Strategy_20260216.md`

**Decisions:**
- Priority: Core CLI first (Phases 0-3), then fan out
- Testing: Full TDD (failing tests first, unit + integration + regression)
- Sprint cadence: 2-day sprints (7 sprints total)
- ECC (Phase 5): Deferred to v1.1
- Desktop GUI + PicoClaw: Start after Core CLI stabilizes (Sprints 6-7)

---

## Sprint Map

| Sprint | Days | Tier | Phases | Deliverable |
|--------|------|------|--------|-------------|
| **S1** | 1-2 | Foundation | 0 + 1 | Verified environment + single-agent CLI working against fleet |
| **S2** | 3-4 | Foundation | 2A-2C | HTTP agent execution path in `agent_tool.rs` with TDD |
| **S3** | 5-6 | Complete | 2D-2F | Multi-agent `/plan`, `/code`, `/solve` with live fleet |
| **S4** | 7-8 | Brand | 3 + 4 | Branded `hermia-coder` binary + HCC metrics |
| **S5** | 9-10 | Validate | 6 | E2E tests, performance benchmarks, documentation |
| **S6** | 11-12 | Expand | 7 | Hermia Coder Desktop GUI |
| **S7** | 13-14 | Expand | 8 | PicoClaw Messaging Gateway + v1.0 release |

---

## Sprint 1: Environment Validation + Primary Model Config

**Duration:** 2 days | **Risk:** LOW | **Phases:** 0 + 1

### Task 1: Verify Toolchains

**Files:**
- Read: `code-rs/cli/Cargo.toml`

**Step 1: Check Rust toolchain**

```bash
rustup show
# Expected: stable or nightly toolchain, target x86_64-unknown-linux-gnu
```

**Step 2: Check Node.js**

```bash
node --version
# Expected: v18+ or v20+
```

**Step 3: Check Go**

```bash
go version
# Expected: go1.21+ (needed for PicoClaw in Sprint 7)
```

---

### Task 2: Verify Fleet Health (All 10 Services)

**Files:**
- Read: `Hermia_Coder_Ecosystem_Execution_Strategy_20260216.md` (port map section)

**Step 1: Check WS1 services**

```bash
# Main Brain (MiniMax-M2.5)
curl -s http://192.168.1.50:8000/v1/models | jq '.data[0].id'
# Expected: "hermia-main-brain" or "MiniMax-M2.5"

# Router (Qwen3-0.6B)
curl -s http://192.168.1.50:8010/health
# Expected: 200 OK
```

**Step 2: Check WS2 GPU 0 services**

```bash
# Embedding
curl -s http://192.168.1.51:8001/v1/models | jq '.data[0].id'

# Reranker
curl -s http://192.168.1.51:8002/v1/models | jq '.data[0].id'

# Granite Micro
curl -s http://192.168.1.51:8003/v1/models | jq '.data[0].id'

# Guardian
curl -s http://192.168.1.51:8060/v1/models | jq '.data[0].id'
```

**Step 3: Check WS2 GPU 1,2 services**

```bash
# Qwen3-Next-80B (Coder)
curl -s http://192.168.1.51:8021/v1/models | jq '.data[0].id'
# Expected: "qwen3-next-80b"

# Qwen3-VL-32B (Vision)
curl -s http://192.168.1.51:8024/v1/models | jq '.data[0].id'
```

**Step 4: Document any services that are down**

Record results. All 10 must be healthy to proceed. If any are down, use `fleet-manager.sh start` on the appropriate workstation.

---

### Task 3: Read Critical Source Files

**Files:**
- Read: `code-rs/core/src/agent_tool.rs` (3290 lines — focus on lines 1300-1800)
- Read: `code-rs/core/src/agent_defaults.rs` (485 lines)
- Read: `code-rs/core/src/model_provider_info.rs` (776 lines)
- Read: `code-rs/core/src/chat_completions.rs` (1236 lines)
- Read: `code-rs/core/src/config_types.rs` (1634 lines — focus on `AgentConfig` at line 392)
- Read: `code-rs/core/src/slash_commands.rs` (lines 1-100)
- Read: `code-rs/core/src/config/sources.rs` (lines 1520-1580)

**Step 1: Confirm `AgentConfig` struct location**

In `config_types.rs`, find the `AgentConfig` struct (expected ~line 392). Confirm fields: `name`, `command`, `args`, `read_only`, `enabled`, `description`, `env`, `args_read_only`, `args_write`, `instructions`. Confirm there is NO `http_endpoint` field yet.

**Step 2: Confirm agent execution path**

In `agent_tool.rs`, find `execute_model_with_permissions()` (expected ~line 1555). Confirm the `match family` block (expected ~line 1682) dispatches on `"claude"`, `"gemini"`, `"qwen"`, `"codex"`, `"code"`, `"cloud"`, and `_`.

**Step 3: Confirm `create_oss_provider()`**

In `model_provider_info.rs`, find `create_oss_provider()` (expected ~line 547). Confirm it uses `WireApi::Chat` and `requires_openai_auth: false`. This is the pattern for Hermia providers.

**Step 4: Confirm `stream_chat_completions` signature**

In `chat_completions.rs`, find the function (expected ~line 41). Note the required parameters: `Prompt`, `ModelFamily`, model slug, `reqwest::Client`, `ModelProviderInfo`, `DebugLogger`, optional auth/otel.

**Step 5: Confirm slash command routing**

In `slash_commands.rs`, find `agent_is_runnable()` (expected ~line 25). Confirm that `"code"`, `"codex"`, `"cloud"` bypass the PATH check. A `"hermia"` family will need to be added here.

**Step 6: Commit a note**

No code changes. Just document your findings in a scratch note for reference.

---

### Task 4: Cold-Cache Build

**Step 1: Run build**

```bash
cd /home/hermia/Documents/VS-Code-Claude/Hermia-Coder
./build-fast.sh
# WARNING: 20+ minutes from cold cache. Use 30-minute timeout.
```

**Step 2: Verify binary location**

```bash
ls -la code-rs/target/dev-fast/code
# Expected: executable binary
```

**Step 3: Smoke test**

```bash
./code-rs/target/dev-fast/code --version
```

---

### Task 5: Test Main Brain (MiniMax-M2.5)

**Files:**
- Read: `code-rs/core/src/model_provider_info.rs` (line 547 — `create_oss_provider`)

**Step 1: Launch against Main Brain**

```bash
cd /home/hermia/Documents/VS-Code-Claude/Hermia-Coder
CODEX_OSS_BASE_URL="http://192.168.1.50:8000/v1" \
  ./code-rs/target/dev-fast/code --model "hermia-main-brain" --model-provider oss
```

**Step 2: Test SSE streaming**

Send a simple prompt: "What is 2+2? Reply in one word."
Verify: text streams token-by-token, not all-at-once.

**Step 3: Test tool calling**

Send: "List the files in the current directory."
Verify: the model invokes a tool call (MiniMax-M2.5 uses `--tool-call-parser minimax_m2`).

**Step 4: Exit and document results**

Record TTFT (time to first token) and whether tool calling succeeded.

---

### Task 6: Test Apollo (Qwen3-Next-80B)

**Step 1: Launch against Coder**

```bash
CODEX_OSS_BASE_URL="http://192.168.1.51:8021/v1" \
  ./code-rs/target/dev-fast/code --model "qwen3-next-80b" --model-provider oss
```

**Step 2: Test code generation**

Send: "Write a Python function that checks if a number is prime. Just the function, no explanation."
Verify: clean code output, reasonable quality.

**Step 3: Exit and document results**

Record TTFT and code quality assessment.

---

### Task 7: Create Hermia Config File

**Files:**
- Create: `~/.hermia-coder/config.toml`

**Step 1: Create config directory**

```bash
mkdir -p ~/.hermia-coder
```

**Step 2: Write config**

```toml
# Hermia Coder Configuration
# This file is read by hermia-coder (code fork) when CODE_HOME=~/.hermia-coder

model = "hermia-main-brain"
model_provider = "hermia-main"

[model_providers.hermia-main]
name = "Hermia Main Brain (MiniMax-M2.5)"
base_url = "http://192.168.1.50:8000/v1"
wire_api = "chat"

[model_providers.hermia-apollo]
name = "Hermia Apollo (Qwen3-Next-80B MoE)"
base_url = "http://192.168.1.51:8021/v1"
wire_api = "chat"

[model_providers.hermia-router]
name = "Hermia Router (Qwen3-0.6B)"
base_url = "http://192.168.1.50:8010/v1"
wire_api = "chat"

[model_providers.hermia-vision]
name = "Hermia Vision (Qwen3-VL-32B)"
base_url = "http://192.168.1.51:8024/v1"
wire_api = "chat"

[model_providers.hermia-micro]
name = "Hermia Micro (Granite 4.0)"
base_url = "http://192.168.1.51:8003/v1"
wire_api = "chat"
```

**Step 3: Test with CODE_HOME override**

```bash
CODE_HOME=~/.hermia-coder \
CODEX_OSS_BASE_URL="http://192.168.1.50:8000/v1" \
  ./code-rs/target/dev-fast/code --model "hermia-main-brain" --model-provider oss
```

Verify it reads from `~/.hermia-coder/config.toml`.

**Step 4: Commit**

```bash
git add docs/plans/2026-02-16-hermia-coder-ecosystem.md
git commit -m "docs(plans): add Hermia Coder Ecosystem v1.0 implementation plan"
```

### Sprint 1 Exit Criteria

- [ ] All 10 fleet services confirmed healthy
- [ ] `./build-fast.sh` passes clean
- [ ] Single-agent CLI works against MiniMax-M2.5 (ws1:8000)
- [ ] Single-agent CLI works against Qwen3-Next-80B (ws2:8021)
- [ ] SSE streaming verified
- [ ] Tool calling verified (MiniMax-M2.5)
- [ ] `~/.hermia-coder/config.toml` created with all 5 providers
- [ ] TTFT baselines documented for both models

---

## Sprint 2: Subagent HTTP Rewiring (Core Rust TDD)

**Duration:** 2 days | **Risk:** HIGH | **Phase:** 2A-2C

This is the hardest sprint. All changes are in `code-rs/core/src/`.

### Task 8: Write Failing Test — AgentHttpConfig Deserialization

**Files:**
- Modify: `code-rs/core/src/config_types.rs:392-440` (AgentConfig struct)
- Test: `code-rs/core/tests/config_types_test.rs` (or inline `#[cfg(test)]` module)

**Step 1: Write the failing test**

Add a test that deserializes a TOML `[[agents]]` entry with `http_endpoint`:

```rust
#[test]
fn test_agent_config_http_endpoint_deserialization() {
    let toml_str = r#"
        [[agents]]
        name = "hermia-athena"
        command = ""
        enabled = true
        description = "Hermia Main Brain"
        http-endpoint = "http://192.168.1.50:8000/v1"
        http-model = "MiniMax-M2.5"
        http-max-tokens = 32768
        http-temperature = 0.7
        http-system-prompt = "You are Athena."
    "#;

    #[derive(Deserialize)]
    struct Wrapper {
        agents: Vec<AgentConfig>,
    }

    let parsed: Wrapper = toml::from_str(toml_str).unwrap();
    let agent = &parsed.agents[0];
    assert_eq!(agent.name, "hermia-athena");
    assert_eq!(
        agent.http_endpoint.as_deref(),
        Some("http://192.168.1.50:8000/v1")
    );
    assert_eq!(agent.http_model.as_deref(), Some("MiniMax-M2.5"));
    assert_eq!(agent.http_max_tokens, Some(32768));
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test -p code-core test_agent_config_http_endpoint -- --nocapture
# Expected: FAIL — no field `http_endpoint` on `AgentConfig`
```

---

### Task 9: Implement AgentHttpConfig Fields

**Files:**
- Modify: `code-rs/core/src/config_types.rs:392-440`

**Step 1: Add HTTP fields to `AgentConfig`**

After the existing `instructions` field (~line 440), add:

```rust
    // HTTP-native agent fields (for local vLLM fleet)
    pub http_endpoint: Option<String>,
    pub http_model: Option<String>,
    pub http_max_tokens: Option<u32>,
    pub http_temperature: Option<f32>,
    pub http_system_prompt: Option<String>,
```

All are `Option<T>` so existing TOML configs without these fields still deserialize.

**Step 2: Run test to verify it passes**

```bash
cargo test -p code-core test_agent_config_http_endpoint -- --nocapture
# Expected: PASS
```

**Step 3: Run existing tests to confirm no regression**

```bash
cargo test -p code-core -- --nocapture
# Expected: all existing tests PASS
```

**Step 4: Commit**

```bash
git add code-rs/core/src/config_types.rs
git commit -m "feat(core/config): add http_endpoint fields to AgentConfig for local fleet agents"
```

---

### Task 10: Write Failing Test — Agent HTTP Routing

**Files:**
- Test: `code-rs/core/src/agent_tool.rs` (inline test module or separate test file)

**Step 1: Write the failing test**

Test that when an `AgentConfig` has `http_endpoint` set, the execution path calls the HTTP function instead of spawning a subprocess. This requires a helper function to extract the routing decision:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn should_use_http_path(config: &AgentConfig) -> bool {
        config.http_endpoint.is_some()
    }

    #[test]
    fn test_http_agent_routes_to_http_path() {
        let config = AgentConfig {
            name: "hermia-athena".into(),
            command: String::new(),
            args: vec![],
            read_only: true,
            enabled: true,
            description: Some("Test".into()),
            env: None,
            args_read_only: None,
            args_write: None,
            instructions: None,
            http_endpoint: Some("http://192.168.1.50:8000/v1".into()),
            http_model: Some("MiniMax-M2.5".into()),
            http_max_tokens: Some(32768),
            http_temperature: Some(0.7),
            http_system_prompt: Some("You are Athena.".into()),
        };
        assert!(should_use_http_path(&config));
    }

    #[test]
    fn test_subprocess_agent_does_not_route_to_http() {
        let config = AgentConfig {
            name: "claude-sonnet".into(),
            command: "claude".into(),
            args: vec![],
            read_only: true,
            enabled: true,
            description: None,
            env: None,
            args_read_only: None,
            args_write: None,
            instructions: None,
            http_endpoint: None,
            http_model: None,
            http_max_tokens: None,
            http_temperature: None,
            http_system_prompt: None,
        };
        assert!(!should_use_http_path(&config));
    }
}
```

**Step 2: Run tests**

```bash
cargo test -p code-core test_http_agent_routes -- --nocapture
cargo test -p code-core test_subprocess_agent_does_not -- --nocapture
# Expected: initially FAIL (function doesn't exist), then PASS after adding it
```

---

### Task 11: Add create_hermia_provider() to model_provider_info.rs

**Files:**
- Modify: `code-rs/core/src/model_provider_info.rs:547-566`

**Step 1: Write the failing test**

```rust
#[test]
fn test_create_hermia_provider() {
    let provider = create_hermia_provider("http://192.168.1.50:8000/v1");
    assert_eq!(provider.base_url.as_deref(), Some("http://192.168.1.50:8000/v1"));
    assert_eq!(provider.wire_api, WireApi::Chat);
    assert!(!provider.requires_openai_auth);
    assert!(provider.env_key.is_none());
}
```

**Step 2: Run to verify it fails**

```bash
cargo test -p code-core test_create_hermia_provider -- --nocapture
# Expected: FAIL — function doesn't exist
```

**Step 3: Implement**

Add after `create_oss_provider()` (~line 566):

```rust
/// Create a ModelProviderInfo for a Hermia local fleet endpoint.
/// Uses WireApi::Chat (OpenAI-compatible /v1/chat/completions).
pub fn create_hermia_provider(base_url: &str) -> ModelProviderInfo {
    ModelProviderInfo {
        name: format!("hermia-{}", base_url.split(':').last().unwrap_or("local")),
        base_url: Some(base_url.to_string()),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Chat,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(2),
        stream_max_retries: Some(2),
        stream_idle_timeout_ms: Some(60_000),
        requires_openai_auth: false,
        openrouter: None,
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test -p code-core test_create_hermia_provider -- --nocapture
# Expected: PASS
```

**Step 5: Commit**

```bash
git add code-rs/core/src/model_provider_info.rs
git commit -m "feat(core/provider): add create_hermia_provider() for local fleet endpoints"
```

---

### Task 12: Implement HTTP Execution Path in agent_tool.rs

**Files:**
- Modify: `code-rs/core/src/agent_tool.rs:1555-1800`

This is the critical change. Inside `execute_model_with_permissions()`:

**Step 1: Add HTTP path branch**

Before the existing `match family` block (~line 1682), add an early return for HTTP agents:

```rust
// HTTP-native agent path (Hermia local fleet)
if let Some(ref http_endpoint) = config.as_ref().and_then(|c| c.http_endpoint.as_ref()) {
    return execute_http_agent(
        agent_id,
        http_endpoint,
        config.as_ref().unwrap(),
        prompt,
        read_only,
        working_dir.as_deref(),
    ).await;
}
```

**Step 2: Implement `execute_http_agent()`**

Add a new async function that:
1. Creates a `ModelProviderInfo` via `create_hermia_provider(http_endpoint)`
2. Builds a `Prompt` from `http_system_prompt` + user prompt
3. Calls `stream_chat_completions()` from `chat_completions.rs`
4. Collects the streamed response into a `String`
5. Returns `Ok(response_text)`

```rust
async fn execute_http_agent(
    agent_id: &str,
    http_endpoint: &str,
    config: &AgentConfig,
    prompt: &str,
    read_only: bool,
    working_dir: Option<&Path>,
) -> Result<String, String> {
    let provider = create_hermia_provider(http_endpoint);
    let model_slug = config.http_model.as_deref().unwrap_or("unknown");
    let system_prompt = config.http_system_prompt.as_deref().unwrap_or("");
    let max_tokens = config.http_max_tokens.unwrap_or(16384);
    let temperature = config.http_temperature.unwrap_or(0.7);

    // Build the chat completions request
    let client = reqwest::Client::new();

    let messages = vec![
        serde_json::json!({"role": "system", "content": system_prompt}),
        serde_json::json!({"role": "user", "content": prompt}),
    ];

    let body = serde_json::json!({
        "model": model_slug,
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": temperature,
        "stream": true,
    });

    let url = format!("{}/chat/completions", http_endpoint.trim_end_matches('/'));

    // Stream SSE response
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP agent {agent_id} request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("HTTP agent {agent_id} returned {status}: {text}"));
    }

    // Collect SSE stream
    let mut result = String::new();
    let mut stream = response.bytes_stream();
    use futures::StreamExt;
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Stream error: {e}"))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim().to_string();
            buffer = buffer[line_end + 1..].to_string();

            if line.starts_with("data: ") {
                let data = &line[6..];
                if data == "[DONE]" {
                    break;
                }
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(content) = parsed["choices"][0]["delta"]["content"].as_str() {
                        result.push_str(content);
                    }
                }
            }
        }
    }

    Ok(result)
}
```

**Note:** This is a simplified direct implementation. The actual code may need to integrate more deeply with the existing `stream_chat_completions()` pipeline depending on how the TUI expects to receive events. Examine the actual call sites and adapt. The key principle is: HTTP agents use `WireApi::Chat` against a local endpoint, no subprocess.

**Step 3: Run all tests**

```bash
cargo test -p code-core -- --nocapture
# Expected: PASS (unit tests for routing + no regression)
```

**Step 4: Build**

```bash
./build-fast.sh
# Expected: clean build, no warnings
```

**Step 5: Commit**

```bash
git add code-rs/core/src/agent_tool.rs
git commit -m "feat(core/agent): add HTTP execution path for Hermia local fleet agents"
```

### Sprint 2 Exit Criteria

- [ ] `AgentHttpConfig` fields added to `AgentConfig` in `config_types.rs`
- [ ] `create_hermia_provider()` in `model_provider_info.rs`
- [ ] HTTP execution path in `agent_tool.rs` — routes when `http_endpoint` present
- [ ] Unit tests: config deserialization, routing decision, provider creation
- [ ] Regression: subprocess agent tests still pass
- [ ] `./build-fast.sh` passes clean with zero warnings

---

## Sprint 3: Agent Defaults + Slash Commands + Integration Testing

**Duration:** 2 days | **Risk:** MEDIUM | **Phase:** 2D-2F

### Task 13: Add Hermia Agent Specs to agent_defaults.rs

**Files:**
- Modify: `code-rs/core/src/agent_defaults.rs:89-265` (AGENT_MODEL_SPECS array)

**Step 1: Write the failing test**

```rust
#[test]
fn test_hermia_athena_spec_exists() {
    let spec = agent_model_spec("hermia-athena");
    assert!(spec.is_some());
    let spec = spec.unwrap();
    assert_eq!(spec.family, "hermia");
    assert_eq!(spec.slug, "hermia-athena");
}

#[test]
fn test_hermia_apollo_spec_exists() {
    let spec = agent_model_spec("hermia-apollo");
    assert!(spec.is_some());
    let spec = spec.unwrap();
    assert_eq!(spec.family, "hermia");
    assert_eq!(spec.slug, "hermia-apollo");
}
```

**Step 2: Run to verify failure**

```bash
cargo test -p code-core test_hermia_athena_spec -- --nocapture
# Expected: FAIL — no spec with slug "hermia-athena"
```

**Step 3: Add specs to AGENT_MODEL_SPECS**

Add to the `AGENT_MODEL_SPECS` static array:

```rust
AgentModelSpec {
    slug: "hermia-athena",
    family: "hermia",
    cli: "",
    read_only_args: &[],
    write_args: &[],
    model_args: &[],
    description: "Hermia Main Brain - MiniMax-M2.5 (131K ctx, tool calling)",
    enabled_by_default: false,
    aliases: &["athena"],
    gating_env: None,
    is_frontline: false,
},
AgentModelSpec {
    slug: "hermia-apollo",
    family: "hermia",
    cli: "",
    read_only_args: &[],
    write_args: &[],
    model_args: &[],
    description: "Apollo Coder - Qwen3-Next-80B MoE (256K ctx, 3B active/token)",
    enabled_by_default: false,
    aliases: &["apollo"],
    gating_env: None,
    is_frontline: false,
},
```

**Step 4: Run tests**

```bash
cargo test -p code-core test_hermia_ -- --nocapture
# Expected: PASS
```

**Step 5: Commit**

```bash
git add code-rs/core/src/agent_defaults.rs
git commit -m "feat(core/agents): add hermia-athena and hermia-apollo agent specs"
```

---

### Task 14: Add "hermia" Family to Slash Command Routing

**Files:**
- Modify: `code-rs/core/src/slash_commands.rs:25` (agent_is_runnable)

**Step 1: Write the failing test**

```rust
#[test]
fn test_hermia_agent_is_runnable_without_binary() {
    let config = AgentConfig {
        name: "hermia-athena".into(),
        command: String::new(),
        // ... (all fields, http_endpoint: Some(...))
        ..Default::default()
    };
    assert!(agent_is_runnable(&config));
}
```

**Step 2: Add "hermia" to the bypass list**

In `agent_is_runnable()` (~line 25), change:

```rust
// Before:
"code" | "codex" | "cloud" => true,

// After:
"code" | "codex" | "cloud" | "hermia" => true,
```

**Step 3: Run tests**

```bash
cargo test -p code-core -- --nocapture
# Expected: PASS
```

**Step 4: Commit**

```bash
git add code-rs/core/src/slash_commands.rs
git commit -m "feat(core/slash): add hermia family to agent_is_runnable bypass"
```

---

### Task 15: Write Hermia [[agents]] Config Entries

**Files:**
- Modify: `~/.hermia-coder/config.toml`

**Step 1: Add agent entries**

Append to the config file:

```toml
[[agents]]
name = "hermia-athena"
command = ""
enabled = true
description = "Hermia Main Brain - MiniMax-M2.5 (131K ctx, tool calling)"
http-endpoint = "http://192.168.1.50:8000/v1"
http-model = "hermia-main-brain"
http-max-tokens = 32768
http-temperature = 0.7
http-system-prompt = "You are Athena, Hermia's planning and reasoning agent. Use tool calling for code exploration and system commands. Think step by step."

[[agents]]
name = "hermia-apollo"
command = ""
enabled = true
description = "Apollo Coder - Qwen3-Next-80B MoE (256K ctx, 512 experts, 10 active)"
http-endpoint = "http://192.168.1.51:8021/v1"
http-model = "qwen3-next-80b"
http-max-tokens = 16384
http-temperature = 0.3
http-system-prompt = "You are Apollo, Hermia's code implementation specialist. Write clean, production-ready code. Be precise and concise."

[subagents]
[[subagents.commands]]
name = "plan"
read-only = true
agents = ["hermia-athena", "hermia-apollo"]
orchestrator-instructions = "Athena handles architecture and risk analysis. Apollo handles implementation details and code structure."

[[subagents.commands]]
name = "code"
read-only = false
agents = ["hermia-apollo", "hermia-athena"]
orchestrator-instructions = "Apollo leads implementation. Athena reviews for correctness and edge cases."

[[subagents.commands]]
name = "solve"
read-only = false
agents = ["hermia-athena", "hermia-apollo"]
orchestrator-instructions = "Both agents collaborate. Synthesize the best approach."
```

---

### Task 16: Integration Test — /plan Against Live Fleet

**Step 1: Run /plan**

```bash
CODE_HOME=~/.hermia-coder \
  ./code-rs/target/dev-fast/code
```

Then type: `/plan Build a REST API for a simple todo list application`

**Step 2: Verify**

- Athena (MiniMax-M2.5 on ws1:8000) provides architecture/planning output
- Apollo (Qwen3-Next-80B on ws2:8021) provides implementation details
- Both responses stream via SSE
- No errors in terminal

**Step 3: Test /code and /solve similarly**

```
/code Implement a Python function to merge two sorted arrays
/solve Fix: "TypeError: Cannot read properties of undefined (reading 'map')"
```

---

### Task 17: Regression Test — Existing Subprocess Agents

**Step 1: Verify existing agents still work**

If any cloud agents are available (claude, gemini), test them:

```bash
# Only if these CLIs are installed locally
which claude && echo "claude CLI available" || echo "skip"
which gemini && echo "gemini CLI available" || echo "skip"
```

**Step 2: Run full test suite**

```bash
cargo test -p code-core -- --nocapture
# Expected: ALL PASS
```

**Step 3: Build**

```bash
./build-fast.sh
# Expected: clean, zero warnings
```

**Step 4: Commit**

```bash
git add -A
git commit -m "feat(core/agents): complete HTTP agent integration with live fleet testing"
```

### Sprint 3 Exit Criteria

- [ ] `hermia-athena` and `hermia-apollo` specs in `agent_defaults.rs`
- [ ] `"hermia"` family bypasses PATH check in `slash_commands.rs`
- [ ] `[[agents]]` config entries with `http-endpoint` in `config.toml`
- [ ] `/plan` works against MiniMax-M2.5 + Qwen3-Next-80B
- [ ] `/code` and `/solve` work
- [ ] MiniMax-M2.5 tool calling works through HTTP agent path
- [ ] Existing subprocess agents unaffected
- [ ] `./build-fast.sh` passes clean

---

## Sprint 4: Branding + HCC Integration

**Duration:** 2 days | **Risk:** MEDIUM | **Phases:** 3 + 4

### Task 18: Binary Rename

**Files:**
- Modify: `code-rs/cli/Cargo.toml` (binary name)

**Step 1: Change binary name**

```toml
# Before:
[[bin]]
name = "code"

# After:
[[bin]]
name = "hermia-coder"
path = "src/main.rs"

[[bin]]
name = "hcode"
path = "src/main.rs"
```

**Step 2: Build and verify**

```bash
./build-fast.sh
ls code-rs/target/dev-fast/hermia-coder
ls code-rs/target/dev-fast/hcode
```

**Step 3: Commit**

```bash
git add code-rs/cli/Cargo.toml
git commit -m "feat(cli): rename binary to hermia-coder / hcode"
```

---

### Task 19: Config Directory Default

**Files:**
- Modify: `code-rs/core/src/config/sources.rs:1557-1576` (find_code_home)

**Step 1: Change default config home**

In `find_code_home()`, change the default fallback from `~/.code` to `~/.hermia-coder`:

```rust
// Before:
home.push(".code");

// After:
home.push(".hermia-coder");
```

Keep the `CODE_HOME` and `CODEX_HOME` env var overrides intact for backwards compatibility.

**Step 2: Add HERMIA_HOME env var**

Add before the existing env var checks:

```rust
if let Some(path) = env_path("HERMIA_HOME")? {
    return Ok(path);
}
```

**Step 3: Test**

```bash
cargo test -p code-core -- --nocapture
./build-fast.sh
```

**Step 4: Commit**

```bash
git add code-rs/core/src/config/sources.rs
git commit -m "feat(core/config): default config home to ~/.hermia-coder, add HERMIA_HOME env var"
```

---

### Task 20: TUI Branding

**Files:**
- Modify: `code-rs/tui/src/` (grep for "Every Code", "Codex", splash text)

**Step 1: Find branding strings**

```bash
grep -rn "Every Code\|Codex\|codex" code-rs/tui/src/ --include="*.rs" | head -30
```

**Step 2: Replace branding**

Change:
- "Every Code" -> "Hermia Coder"
- Splash/greeting text as appropriate
- Keep internal identifiers (crate names, module names) unchanged

**Step 3: Build and verify TUI**

```bash
./build-fast.sh
./code-rs/target/dev-fast/hermia-coder
# Verify: splash shows "Hermia Coder", not "Every Code"
```

**Step 4: Commit**

```bash
git add code-rs/tui/
git commit -m "feat(tui): rebrand to Hermia Coder"
```

---

### Task 21: Create HCC Crate

**Files:**
- Create: `code-rs/hcc/Cargo.toml`
- Create: `code-rs/hcc/src/lib.rs`
- Modify: `code-rs/Cargo.toml` (workspace members)

**Step 1: Create crate structure**

```bash
mkdir -p code-rs/hcc/src
```

**Step 2: Write Cargo.toml**

```toml
[package]
name = "code-hcc"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
tokio-tungstenite = "0.24"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
```

**Step 3: Write lib.rs**

```rust
//! Hermia Command Center (HCC) integration.
//!
//! Sends metrics (token usage, latency, agent status) to the HCC
//! dashboard via WebSocket at ws://192.168.1.50:9220/ws.

use serde::Serialize;
use tokio::sync::mpsc;

const HCC_DEFAULT_URL: &str = "ws://192.168.1.50:9220/ws";

#[derive(Debug, Clone, Serialize)]
pub struct HccMetric {
    pub timestamp: u64,
    pub metric_type: HccMetricType,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum HccMetricType {
    TokenUsage {
        agent_id: String,
        model: String,
        prompt_tokens: u64,
        completion_tokens: u64,
    },
    Latency {
        agent_id: String,
        model: String,
        ttft_ms: u64,
        total_ms: u64,
    },
    AgentStatus {
        agent_id: String,
        status: String,
    },
    EndpointHealth {
        endpoint: String,
        healthy: bool,
        response_ms: Option<u64>,
    },
}

/// Handle for sending metrics to HCC.
#[derive(Clone)]
pub struct HccClient {
    tx: mpsc::UnboundedSender<HccMetric>,
}

impl HccClient {
    /// Spawn the HCC WebSocket connection and return a client handle.
    pub fn spawn(url: Option<&str>) -> Self {
        let url = url.unwrap_or(HCC_DEFAULT_URL).to_string();
        let (tx, mut rx) = mpsc::unbounded_channel::<HccMetric>();

        tokio::spawn(async move {
            match tokio_tungstenite::connect_async(&url).await {
                Ok((mut ws, _)) => {
                    use futures_util::SinkExt;
                    while let Some(metric) = rx.recv().await {
                        if let Ok(json) = serde_json::to_string(&metric) {
                            let msg = tokio_tungstenite::tungstenite::Message::Text(json);
                            if ws.send(msg).await.is_err() {
                                tracing::warn!("HCC WebSocket send failed");
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("HCC connection failed: {e}. Metrics will be dropped.");
                    // Drain the channel to avoid blocking senders
                    while rx.recv().await.is_some() {}
                }
            }
        });

        Self { tx }
    }

    /// Send a metric to HCC. Non-blocking, drops if disconnected.
    pub fn send(&self, metric: HccMetric) {
        let _ = self.tx.send(metric);
    }
}
```

**Step 4: Add to workspace**

In `code-rs/Cargo.toml`, add `"hcc"` to the workspace members list.

**Step 5: Build and test**

```bash
cargo build -p code-hcc
cargo test -p code-hcc -- --nocapture
./build-fast.sh
```

**Step 6: Commit**

```bash
git add code-rs/hcc/ code-rs/Cargo.toml
git commit -m "feat(hcc): add Hermia Command Center metrics crate"
```

---

### Task 22: Hook HCC into Agent Execution

**Files:**
- Modify: `code-rs/core/src/agent_tool.rs` (add HCC metric sends after completions)

**Step 1: Add code-hcc dependency to code-core**

In `code-rs/core/Cargo.toml`:
```toml
code-hcc = { path = "../hcc" }
```

**Step 2: Send metrics after HTTP agent completion**

In `execute_http_agent()`, after collecting the response, send latency and token metrics:

```rust
if let Some(hcc) = hcc_client {
    hcc.send(HccMetric {
        timestamp: now_millis(),
        metric_type: HccMetricType::Latency {
            agent_id: agent_id.to_string(),
            model: model_slug.to_string(),
            ttft_ms,
            total_ms,
        },
    });
}
```

**Step 3: Build and test**

```bash
./build-fast.sh
```

**Step 4: Commit**

```bash
git add code-rs/core/ code-rs/hcc/
git commit -m "feat(core/hcc): send agent metrics to Hermia Command Center"
```

### Sprint 4 Exit Criteria

- [ ] Binary builds as `hermia-coder` and `hcode`
- [ ] Config reads from `~/.hermia-coder/` by default
- [ ] `HERMIA_HOME` env var override works
- [ ] TUI shows "Hermia Coder" branding
- [ ] `code-hcc` crate compiles and connects to HCC WebSocket
- [ ] Agent completions send metrics to HCC
- [ ] `./build-fast.sh` passes clean

---

## Sprint 5: End-to-End Validation + Performance Benchmarks

**Duration:** 2 days | **Risk:** LOW | **Phase:** 6

### Task 23: E2E Test Suite

**Step 1: Test /plan with real prompt**

```bash
./code-rs/target/dev-fast/hermia-coder
# Type: /plan Build REST API for inventory management with auth, CRUD, and search
```

Verify: coherent multi-agent planning output.

**Step 2: Test /code with real prompt**

```
/code Implement auth middleware with JWT token validation in Express.js
```

Verify: working code output.

**Step 3: Test /solve with real bug**

```
/solve TypeError: Cannot read properties of undefined (reading 'map') in React component that fetches data from API
```

Verify: diagnosis and fix provided.

---

### Task 24: Network Isolation Test

**Step 1: Monitor network during session**

```bash
# In a separate terminal, monitor outbound connections
sudo ss -tnp | grep -v '192.168.1\.' | grep hermia-coder
```

**Step 2: Verify zero external calls**

Run a `/plan` command and confirm all connections are to `192.168.1.50` or `192.168.1.51` only.

---

### Task 25: Performance Benchmarks

**Step 1: TTFT benchmarks**

Run 5 identical prompts against each model and record time-to-first-token:

```bash
# Main Brain (MiniMax-M2.5)
time curl -s http://192.168.1.50:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"hermia-main-brain","messages":[{"role":"user","content":"Hello"}],"max_tokens":1,"stream":false}'

# Coder (Qwen3-Next-80B)
time curl -s http://192.168.1.51:8021/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"qwen3-next-80b","messages":[{"role":"user","content":"Hello"}],"max_tokens":1,"stream":false}'
```

**Step 2: Tool calling reliability**

Send 20 tool-calling prompts to MiniMax-M2.5, count successes:
```
Success rate = successful_tool_calls / 20 * 100
Target: >90%
```

**Step 3: Document results**

Create `docs/benchmarks/2026-02-XX-baseline.md` with all measurements.

---

### Task 26: Documentation

**Files:**
- Create: `ARCHITECTURE.md`
- Create: `FLEET.md`
- Create: `SETUP.md`

**Step 1: Write ARCHITECTURE.md**

Cover: codebase structure, agent execution flow (subprocess vs HTTP), config system, HCC integration.

**Step 2: Write FLEET.md**

Copy the fleet reference from the strategy document (all 10 services, ports, GPU layout).

**Step 3: Write SETUP.md**

Quick-start: install, configure `~/.hermia-coder/config.toml`, verify fleet, first run.

**Step 4: Tag v1.0-rc1**

```bash
git add -A
git commit -m "docs: add ARCHITECTURE, FLEET, SETUP documentation"
git tag -a v1.0-rc1 -m "Hermia Coder Ecosystem v1.0 Release Candidate 1"
```

### Sprint 5 Exit Criteria

- [ ] E2E tests pass: `/plan`, `/code`, `/solve` with real prompts
- [ ] Network isolation verified (zero external calls)
- [ ] TTFT baselines documented for both models
- [ ] Tool calling reliability >90%
- [ ] ARCHITECTURE.md, FLEET.md, SETUP.md written
- [ ] v1.0-rc1 tagged

---

## Sprint 6: CodePilot Desktop GUI

**Duration:** 2 days | **Risk:** LOW | **Phase:** 7

### Task 27: Clone and Analyze CodePilot

**Step 1: Clone**

```bash
cd /home/hermia/Documents/VS-Code-Claude
git clone https://github.com/op7418/CodePilot.git Hermia-Coder-Desktop
cd Hermia-Coder-Desktop && npm install
```

**Step 2: Map Anthropic SDK calls**

```bash
grep -rn "anthropic\|claude\|@anthropic-ai" src/ --include="*.ts" --include="*.tsx" | head -30
```

Document every file and function that calls the Anthropic API.

---

### Task 28: Rewire to Hermia Fleet

**Files:**
- Create: `src/main/hermia-client.ts`
- Modify: wherever `claude-client.ts` is imported

**Step 1: Create hermia-client.ts**

```typescript
const ENDPOINTS = {
  main:   'http://192.168.1.50:8000/v1',
  coder:  'http://192.168.1.51:8021/v1',
  vision: 'http://192.168.1.51:8024/v1',
  micro:  'http://192.168.1.51:8003/v1',
};

const MODELS = {
  main:   'hermia-main-brain',
  coder:  'qwen3-next-80b',
  vision: 'qwen3-vl-32b',
  micro:  'granite-4.0-micro',
};
```

**Step 2: Replace Anthropic SDK with OpenAI-compatible fetch**

Use standard `fetch()` with SSE parsing against `/v1/chat/completions`.

**Step 3: Wire model list from /v1/models**

Dynamically populate model selector by querying each endpoint.

---

### Task 29: Branding and Build

**Step 1: Rebrand**

- App name: "Hermia Coder Desktop"
- Update `electron-builder.yml`
- Add fleet status indicator
- Add model switcher toolbar

**Step 2: Build**

```bash
npm run build
```

---

### Task 30: Desktop Integration Test

**Step 1: Launch and verify streaming**

```bash
npm start
```

- Send prompt, verify MiniMax-M2.5 streams response
- Switch to Coder model, verify Qwen3-Next-80B responds

**Step 2: Test session persistence**

Close and reopen app, verify chat history preserved.

**Step 3: Test model switching**

Switch between Main Brain and Coder mid-conversation.

### Sprint 6 Exit Criteria

- [ ] Desktop app builds and launches
- [ ] Streams from MiniMax-M2.5
- [ ] Streams from Qwen3-Next-80B
- [ ] Model switching works
- [ ] Session persistence works
- [ ] Fleet status indicator shows live data

---

## Sprint 7: PicoClaw Gateway + v1.0 Release

**Duration:** 2 days | **Risk:** LOW | **Phase:** 8

### Task 31: Clone and Configure PicoClaw

**Step 1: Clone and build**

```bash
cd /home/hermia
git clone https://github.com/sipeed/picoclaw.git hermia-picoclaw
cd hermia-picoclaw
make deps && make build && make install
picoclaw onboard
```

**Step 2: Configure for Hermia fleet**

Write `~/.picoclaw/config.json`:

```json
{
  "agents": {
    "defaults": {
      "workspace": "~/.picoclaw/workspace",
      "restrict_to_workspace": false,
      "provider": "vllm",
      "model": "hermia-main-brain",
      "max_tokens": 32768,
      "temperature": 0.7,
      "max_tool_iterations": 20
    }
  },
  "providers": {
    "vllm": {
      "api_key": "not-needed",
      "api_base": "http://192.168.1.50:8000/v1"
    }
  },
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TELEGRAM_BOT_TOKEN",
      "allow_from": ["YOUR_TELEGRAM_USER_ID"]
    }
  },
  "heartbeat": { "enabled": true, "interval": 30 },
  "gateway": { "host": "0.0.0.0", "port": 18790 }
}
```

**Step 3: Test CLI mode**

```bash
picoclaw agent -m "What model are you? What time is it?"
```

---

### Task 32: Create Workspace Files

**Files:**
- Create: `~/.picoclaw/workspace/SOUL.md`
- Create: `~/.picoclaw/workspace/IDENTITY.md`
- Create: `~/.picoclaw/workspace/AGENT.md`
- Create: `~/.picoclaw/workspace/HEARTBEAT.md`

Write each file per the strategy document specifications (Section 8B-8C).

---

### Task 33: Systemd Service + Cron

**Files:**
- Create: `/etc/systemd/system/hermia-picoclaw.service`

**Step 1: Write service file**

Per the strategy document Section 8D.

**Step 2: Enable and start**

```bash
sudo systemctl daemon-reload
sudo systemctl enable hermia-picoclaw
sudo systemctl start hermia-picoclaw
sudo systemctl status hermia-picoclaw
```

**Step 3: Set up cron jobs**

```bash
picoclaw cron add "8:00" "Morning briefing: all 10 services, GPU temps, disk usage, overnight errors"
picoclaw cron add "18:00" "End of day: GPU hours, token counts, issues encountered"
picoclaw cron add "*/4h" "Quick fleet-manager health check on both workstations"
```

---

### Task 34: PicoClaw Testing

**Step 1: CLI mode** - Send message, verify response
**Step 2: Telegram** - Send Telegram message, verify bot responds
**Step 3: Tool execution** - Ask to run a command, verify it executes
**Step 4: Heartbeat** - Wait 30 minutes, verify heartbeat fires
**Step 5: Cron** - Verify cron list shows 3 jobs
**Step 6: Memory** - Close and reopen, verify conversation memory persists

---

### Task 35: Cross-Component Validation + v1.0 Tag

**Step 1: Verify all 3 interfaces work simultaneously**

- Terminal: `hermia-coder /plan "Design a microservices architecture"`
- Desktop: Open Hermia Coder Desktop, send same prompt
- PicoClaw: Send via Telegram "Design a microservices architecture"

All three should get responses from the same fleet.

**Step 2: Verify HCC dashboard**

Open `ws://192.168.1.50:9220/ws` dashboard. Confirm metrics flowing from all interfaces.

**Step 3: Final build**

```bash
cd /home/hermia/Documents/VS-Code-Claude/Hermia-Coder
./build-fast.sh
```

**Step 4: Tag v1.0**

```bash
git add -A
git commit -m "feat: Hermia Coder Ecosystem v1.0 - three interfaces, ten services, zero cloud"
git tag -a v1.0.0 -m "Hermia Coder Ecosystem v1.0.0"
```

### Sprint 7 Exit Criteria

- [ ] PicoClaw responds via CLI
- [ ] PicoClaw responds via Telegram
- [ ] Tool execution works through PicoClaw
- [ ] Heartbeat monitors fleet health
- [ ] Cron jobs configured (morning, EOD, 4h health)
- [ ] Systemd service starts on boot
- [ ] All 3 interfaces verified working simultaneously
- [ ] HCC dashboard shows metrics from all interfaces
- [ ] v1.0.0 tagged

---

## v1.1 Backlog (Deferred)

| Item | Sprint Estimate | Notes |
|------|----------------|-------|
| Everything-Claude-Code Adaptation (Phase 5) | 2 sprints | 13 agents, 40 skills, 37 commands |
| Multi-model routing in PicoClaw | 1 sprint | Route by task type to different models |
| Voice pipeline (ASR + TTS) | 1 sprint | ws2:8040 + ws2:8050 |
| Safety pre-screening (Guardian) | 0.5 sprint | ws2:8060 gate |
| RAG integration | 1 sprint | ws2:8001 embedding + ws2:8002 reranking |
| TALOS bridge | 0.5 sprint | Orange Pi I2C/SPI integration |

---

## Risk Register

| Risk | Sprint | Severity | Mitigation |
|------|--------|----------|------------|
| `agent_tool.rs` HTTP path breaks subprocess agents | S2 | HIGH | Regression tests written first (Task 10) |
| MiniMax-M2.5 tool calling unreliable via vLLM | S1 | MEDIUM | Test in Sprint 1 Task 5 before writing any code |
| `./build-fast.sh` takes >30 min | S1 | LOW | Use long timeout, only rebuild changed crates after cold cache |
| Qwen3-Next-80B SSE format differs from OpenAI | S2 | LOW | vLLM normalizes to OpenAI format; existing `chat_completions.rs` handles it |
| CodePilot deeply coupled to Anthropic SDK | S6 | LOW | TypeScript is straightforward to refactor |
| PicoClaw vllm provider needs api_key workaround | S7 | LOW | One-line Go fix or `"api_key": "not-needed"` |
| HCC WebSocket not running | S4 | LOW | `HccClient::spawn()` handles connection failure gracefully |

---

## Testing Summary

| Type | Where | Sprint | Count |
|------|-------|--------|-------|
| **Unit (TDD)** | `config_types.rs`, `model_provider_info.rs`, `agent_tool.rs`, `slash_commands.rs` | S2-S3 | ~10 tests |
| **Integration** | Live fleet: `/plan`, `/code`, `/solve` | S3, S5 | ~6 tests |
| **Regression** | Subprocess agents still work | S2-S4 | ~3 tests |
| **E2E** | Real prompts through all 3 interfaces | S5, S7 | ~9 tests |
| **Performance** | TTFT, tool calling reliability | S5 | ~3 benchmarks |
| **Network** | Zero external calls | S5 | 1 test |
| **Total** | | | ~32 tests/checks |
