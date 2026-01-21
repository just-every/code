use std::collections::VecDeque;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{anyhow, Context, Result};
use code_core::config::Config;
use code_core::agent_defaults::build_model_guide_description;
use code_core::config_types::{AutoDriveSettings, ReasoningEffort, TextVerbosity};
use code_core::debug_logger::DebugLogger;
use code_core::codex::compact::resolve_compact_prompt_text;
use code_core::model_family::{derive_default_model_family, find_family_for_model};
use code_core::project_doc::read_auto_drive_docs;
use code_core::protocol::SandboxPolicy;
use code_core::slash_commands::get_enabled_agents;
use code_core::{AuthManager, ModelClient, Prompt, ResponseEvent, TextFormat};
use code_core::{RateLimitSwitchState, switch_active_account_on_rate_limit};
use code_core::auth;
use code_core::auth_accounts;
use code_core::error::CodexErr;
use code_common::model_presets::clamp_reasoning_effort_for_model;
use code_protocol::models::{ContentItem, ReasoningItemContent, ResponseItem};
use code_core::protocol::TokenUsage;
use futures::StreamExt;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{self, json, Value};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::auto_compact::{
    apply_compaction,
    build_checkpoint_summary,
    compact_with_endpoint,
    compute_slice_bounds,
    estimate_item_tokens,
};
use crate::coordinator_user_schema::{parse_user_turn_reply, user_turn_schema};
use crate::session_metrics::SessionMetrics;
use crate::retry::{retry_with_backoff, RetryDecision, RetryError, RetryOptions};
#[cfg(feature = "dev-faults")]
use crate::faults::{fault_to_error, next_fault, FaultScope, InjectedFault};
use code_common::elapsed::format_duration;
use chrono::{DateTime, Local, Utc};
use rand::Rng;

const RATE_LIMIT_BUFFER: Duration = Duration::from_secs(5);
const RATE_LIMIT_JITTER_MAX: Duration = Duration::from_secs(3);
const MAX_RETRY_ELAPSED: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MAX_DECISION_RECOVERY_ATTEMPTS: u32 = 3;
const MESSAGE_LIMIT_FALLBACK: usize = 120;
const DEBUG_JSON_MAX_CHARS: usize = 1200;
const CLI_PROMPT_MIN_CHARS: usize = 4;
const CLI_PROMPT_MAX_CHARS: usize = 600;

#[derive(Debug, thiserror::Error)]
#[error("auto coordinator cancelled")]
struct AutoCoordinatorCancelled;

pub const MODEL_SLUG: &str = "gpt-5.1";
const USER_TURN_SCHEMA_NAME: &str = "auto_coordinator_user_turn";
const COORDINATOR_PROMPT: &str = include_str!("../../core/prompt_coordinator.md");
const TIMEBOXED_EXEC_COORDINATOR_GUIDANCE: &str = "SYSTEM: Time-boxed autonomous exec is enabled.\n\nYou are coordinating a non-interactive coding run. Optimize for verifier success, not exploration.\n\nContract-first and evidence-led:\n- In the first 3 minutes, force a concrete failing signal by running the most authoritative acceptance check available. Prefer (in order): `/tests/verify.sh` (read it, then run it) > a task-provided verifier script > the narrowest relevant test (e.g. `pytest -q /tests/test_outputs.py`).\n- Treat that check as the contract: exact paths, ports, formats, exit codes, and stated constraints.\n\nConverge with small diffs:\n- Ship a minimal first-pass fix quickly, then iterate against the same check.\n- Avoid speculative redesigns, broad refactors, or dependency churn unless forced by the contract.\n\nDelegate by outcomes only:\n- Each turn: give one short directive phrased as an outcome (\"make X pass\", \"produce file Y at path Z\", \"ensure binary at /path is executable\"). Let the CLI pick tactics.\n\nTime discipline:\n- Prefer cheap proofs before expensive steps (long builds/downloads/services).\n- Prefer local files and official libraries over scraping web UIs/config endpoints.\n\nFinish standard:\n- finish_success only with proof (acceptance check green + required artifacts present exactly where asserted).\n- Otherwise finish_failed with the last check, its output/error, what changed, and the single next verification step.";

const ALL_TEXT_VERBOSITY: &[TextVerbosity] = &[
    TextVerbosity::Low,
    TextVerbosity::Medium,
    TextVerbosity::High,
];

fn supported_text_verbosity_for_model(model: &str) -> &'static [TextVerbosity] {
    if model.eq_ignore_ascii_case("gpt-5.1-codex-max") {
        &[TextVerbosity::Medium]
    } else {
        ALL_TEXT_VERBOSITY
    }
}

#[derive(Debug, Clone)]
struct AutoTimeBudget {
    deadline: Instant,
    total: Duration,
    next_nudge_at: Instant,
}

fn retry_max_elapsed(deadline: Option<Instant>) -> Duration {
    let max = MAX_RETRY_ELAPSED;
    let Some(deadline) = deadline else {
        return max;
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    let remaining = remaining.saturating_sub(RATE_LIMIT_BUFFER);
    remaining.min(max)
}

impl AutoTimeBudget {
    fn new(deadline: Instant, total: Duration) -> Self {
        let half = total / 2;
        let next_nudge_at = deadline.checked_sub(half).unwrap_or(deadline);
        Self {
            deadline,
            total,
            next_nudge_at,
        }
    }

    fn maybe_nudge(&mut self) -> Option<String> {
        let now = Instant::now();
        if now < self.next_nudge_at {
            return None;
        }

        let remaining = self.deadline.saturating_duration_since(now);
        let elapsed = self.total.saturating_sub(remaining);

        if elapsed < (self.total / 2) {
            let half = self.total / 2;
            self.next_nudge_at = self.deadline.checked_sub(half).unwrap_or(self.deadline);
            return None;
        }

        let guidance = if remaining <= Duration::from_secs(30) {
            "Time is nearly up: stop exploring; take the simplest safe path and do one cheap verification before finishing."
        } else if remaining <= Duration::from_secs(120) {
            "Time is tight: parallelize any remaining scouting/verification (agents or tool-call batching) and finish with the cheapest proof."
        } else {
            "Past 50% of the time budget: start converging; parallelize remaining scouting/verification and avoid detours."
        };

        self.next_nudge_at = now + next_budget_nudge_interval(remaining);

        Some(format!(
            "Time budget update: total={} elapsed={} remaining={}. {guidance}",
            format_duration(self.total),
            format_duration(elapsed),
            format_duration(remaining)
        ))
    }
}

fn next_budget_nudge_interval(remaining: Duration) -> Duration {
    if remaining >= Duration::from_secs(30 * 60) {
        Duration::from_secs(5 * 60)
    } else if remaining >= Duration::from_secs(10 * 60) {
        Duration::from_secs(2 * 60)
    } else if remaining >= Duration::from_secs(5 * 60) {
        Duration::from_secs(60)
    } else if remaining >= Duration::from_secs(2 * 60) {
        Duration::from_secs(30)
    } else if remaining >= Duration::from_secs(60) {
        Duration::from_secs(15)
    } else if remaining >= Duration::from_secs(30) {
        Duration::from_secs(10)
    } else if remaining >= Duration::from_secs(10) {
        Duration::from_secs(5)
    } else {
        Duration::from_secs(2)
    }
}

#[derive(Clone)]
pub struct AutoCoordinatorEventSender {
    inner: Arc<dyn Fn(AutoCoordinatorEvent) + Send + Sync>,
}

impl AutoCoordinatorEventSender {
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(AutoCoordinatorEvent) + Send + Sync + 'static,
    {
        Self { inner: Arc::new(f) }
    }

    #[tracing::instrument(skip(self, event), fields(event = event.kind()))]
    pub fn send(&self, event: AutoCoordinatorEvent) {
        tracing::debug!(target: "auto_drive::coordinator", event = event.kind(), "dispatch coordinator event");
        (self.inner)(event);
    }
}

#[derive(Debug, Clone)]
pub struct AutoTurnCliAction {
    pub prompt: String,
    pub context: Option<String>,
    pub suppress_ui_context: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoTurnAgentsTiming {
    Parallel,
    Blocking,
}

#[derive(Debug, Clone)]
pub struct AutoTurnAgentsAction {
    pub prompt: String,
    pub context: Option<String>,
    pub write: bool,
    pub write_requested: Option<bool>,
    pub models: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoCoordinatorStatus {
    Continue,
    Success,
    Failed,
}

#[derive(Debug, Clone)]
pub enum AutoCoordinatorEvent {
    Decision {
        seq: u64,
        status: AutoCoordinatorStatus,
        status_title: Option<String>,
        status_sent_to_user: Option<String>,
        goal: Option<String>,
        cli: Option<AutoTurnCliAction>,
        agents_timing: Option<AutoTurnAgentsTiming>,
        agents: Vec<AutoTurnAgentsAction>,
        transcript: Vec<ResponseItem>,
    },
    Thinking {
        delta: String,
        summary_index: Option<u32>,
    },
    Action {
        message: String,
    },
    UserReply {
        user_response: Option<String>,
        cli_command: Option<String>,
    },
    TokenMetrics {
        total_usage: TokenUsage,
        last_turn_usage: TokenUsage,
        turn_count: u32,
        duplicate_items: u32,
        replay_updates: u32,
    },
    CompactedHistory {
        conversation: Arc<[ResponseItem]>,
        show_notice: bool,
    },
    StopAck,
}

impl AutoCoordinatorEvent {
    fn kind(&self) -> &'static str {
        match self {
            Self::Decision { .. } => "decision",
            Self::Thinking { .. } => "thinking",
            Self::Action { .. } => "action",
            Self::UserReply { .. } => "user_reply",
            Self::TokenMetrics { .. } => "token_metrics",
            Self::CompactedHistory { .. } => "compacted_history",
            Self::StopAck => "stop_ack",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AutoCoordinatorHandle {
    pub tx: Sender<AutoCoordinatorCommand>,
    cancel_token: CancellationToken,
}

impl AutoCoordinatorHandle {
    pub fn send(
        &self,
        command: AutoCoordinatorCommand,
    ) -> std::result::Result<(), mpsc::SendError<AutoCoordinatorCommand>> {
        self.tx.send(command)
    }

    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }
}

#[derive(Debug)]
pub enum AutoCoordinatorCommand {
    UpdateConversation(Arc<[ResponseItem]>),
    HandleUserPrompt {
        _prompt: String,
        conversation: Arc<[ResponseItem]>,
    },
    AckDecision { seq: u64 },
    Stop,
}

#[derive(Clone)]
struct PendingDecision {
    seq: u64,
    status: AutoCoordinatorStatus,
    status_title: Option<String>,
    status_sent_to_user: Option<String>,
    goal: Option<String>,
    cli: Option<AutoTurnCliAction>,
    agents_timing: Option<AutoTurnAgentsTiming>,
    agents: Vec<AutoTurnAgentsAction>,
    transcript: Vec<ResponseItem>,
}

impl PendingDecision {
    fn into_event(self) -> AutoCoordinatorEvent {
        AutoCoordinatorEvent::Decision {
            seq: self.seq,
            status: self.status,
            status_title: self.status_title,
            status_sent_to_user: self.status_sent_to_user,
            goal: self.goal,
            cli: self.cli,
            agents_timing: self.agents_timing,
            agents: self.agents,
            transcript: self.transcript,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TurnComplexity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TurnConfig {
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    #[allow(dead_code)]
    pub complexity: Option<TurnComplexity>,
    #[serde(default)]
    pub text_format_override: Option<code_core::TextFormat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnMode {
    Normal,
    SubAgentWrite,
    SubAgentReadOnly,
    Review,
}

impl Default for TurnMode {
    fn default() -> Self {
        Self::Normal
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AgentPreferences {
    #[serde(default)]
    pub prefer_research: bool,
    #[serde(default)]
    pub prefer_planning: bool,
    #[serde(default)]
    pub requested_models: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewTiming {
    PostTurn,
    PreWrite,
    Immediate,
}

impl Default for ReviewTiming {
    fn default() -> Self {
        Self::PostTurn
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ReviewStrategy {
    #[serde(default)]
    pub timing: ReviewTiming,
    #[serde(default)]
    pub custom_prompt: Option<String>,
    #[serde(default)]
    pub scope_hint: Option<String>,
}

impl Default for ReviewStrategy {
    fn default() -> Self {
        Self {
            timing: ReviewTiming::PostTurn,
            custom_prompt: None,
            scope_hint: None,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct TurnDescriptor {
    #[serde(default)]
    pub mode: TurnMode,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub complexity: Option<TurnComplexity>,
    #[serde(default)]
    pub agent_preferences: Option<AgentPreferences>,
    #[serde(default)]
    pub review_strategy: Option<ReviewStrategy>,
    #[serde(default)]
    pub text_format_override: Option<code_core::TextFormat>,
}

impl Default for TurnDescriptor {
    fn default() -> Self {
        Self {
            mode: TurnMode::Normal,
            read_only: false,
            complexity: None,
            agent_preferences: None,
            review_strategy: None,
            text_format_override: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use code_core::agent_defaults::DEFAULT_AGENT_NAMES;
    use code_core::error::RetryLimitReachedError;
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn turn_descriptor_defaults_to_normal_mode() {
        let value = json!({});
        let descriptor: TurnDescriptor = serde_json::from_value(value).unwrap();
        assert_eq!(descriptor.mode, TurnMode::Normal);
        assert!(!descriptor.read_only);
        assert!(descriptor.complexity.is_none());
        assert!(descriptor.agent_preferences.is_none());
        assert!(descriptor.review_strategy.is_none());
    }

    #[test]
    fn schema_includes_prompt_and_agents() {
        let active_agents = vec![
            "codex-plan".to_string(),
            "codex-research".to_string(),
        ];
        let schema = build_schema(&active_agents, SchemaFeatures::default());
        let props = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .expect("schema properties");
        assert!(!props.contains_key("goal"));
        assert!(props.contains_key("status_title"), "status_title property missing");
        assert!(
            props.contains_key("status_sent_to_user"),
            "status_sent_to_user property missing"
        );
        assert!(
            props.contains_key("prompt_sent_to_cli"),
            "prompt_sent_to_cli property missing"
        );
        assert!(props.contains_key("agents"), "agents property missing");
        assert!(!props.contains_key("code_review"));
        assert!(!props.contains_key("cross_check"));
        assert!(!props.contains_key("progress"));

        let schema_required = schema
            .get("required")
            .and_then(|v| v.as_array())
            .expect("root required");
        assert!(schema_required.contains(&json!("status_title")));
        assert!(schema_required.contains(&json!("status_sent_to_user")));
        assert!(schema_required.contains(&json!("prompt_sent_to_cli")));

        let agents_obj = props
            .get("agents")
            .and_then(|v| v.as_object())
            .expect("agents schema object");
        let agents_required = agents_obj
            .get("required")
            .and_then(|v| v.as_array())
            .expect("agents required");
        assert!(agents_required.contains(&json!("timing")));
        assert!(agents_required.contains(&json!("list")));
        assert!(!agents_required.contains(&json!("models")));

        let list_items_schema = agents_obj
            .get("properties")
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("list"))
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("items"))
            .and_then(|v| v.as_object())
            .expect("agents.list items");
        let item_props = list_items_schema
            .get("properties")
            .and_then(|v| v.as_object())
            .expect("agents.list item properties");
        let models_schema = item_props
            .get("models")
            .and_then(|v| v.as_object())
            .expect("agents.list item models schema");
        assert_eq!(models_schema.get("type"), Some(&json!("array")));
        let enum_values = models_schema
            .get("items")
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("enum"))
            .and_then(|v| v.as_array())
            .expect("models enum values");
        let expected_enum: Vec<Value> = active_agents
            .iter()
            .map(|name| Value::String(name.clone()))
            .collect();
        assert_eq!(*enum_values, expected_enum);

        assert!(!props.contains_key("code_review"));
        assert!(!props.contains_key("cross_check"));
    }

    #[test]
    fn schema_sets_prompt_sent_to_cli_min_but_no_max_length() {
        let active_agents: Vec<String> = Vec::new();
        let schema = build_schema(&active_agents, SchemaFeatures::default());
        let prompt_schema = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("prompt_sent_to_cli"))
            .and_then(|v| v.as_object())
            .expect("prompt_sent_to_cli schema");

        assert_eq!(
            prompt_schema.get("minLength"),
            Some(&json!(CLI_PROMPT_MIN_CHARS)),
            "schema minLength should match CLI_PROMPT_MIN_CHARS"
        );
        assert!(
            !prompt_schema.contains_key("maxLength"),
            "schema should omit maxLength to avoid provider truncation"
        );
    }

    #[test]
    fn retry_max_elapsed_defaults_to_global_limit() {
        assert_eq!(retry_max_elapsed(None), MAX_RETRY_ELAPSED);
    }

    #[test]
    fn retry_max_elapsed_zero_when_deadline_has_passed() {
        let deadline = Instant::now();
        assert_eq!(retry_max_elapsed(Some(deadline)), Duration::ZERO);
    }

    #[test]
    fn retry_limit_marked_retryable_is_retried() {
        let err = CodexErr::RetryLimit(RetryLimitReachedError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            request_id: None,
            retryable: true,
        });

        match classify_model_error(&anyhow!(err)) {
            RetryDecision::RetryAfterBackoff { reason } => {
                assert!(reason.contains("retry limit"));
            }
            other => panic!("expected retry, got {other:?}"),
        }
    }

    #[test]
    fn schema_defaults_to_builtin_agents_enum() {
        let schema = build_schema(
            &DEFAULT_AGENT_NAMES
                .iter()
                .map(|name| (*name).to_string())
                .collect::<Vec<_>>(),
            SchemaFeatures::default(),
        );
        let props = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .expect("schema properties");
        let agents_obj = props
            .get("agents")
            .and_then(|v| v.as_object())
            .expect("agents schema");
        let item_enum = agents_obj
            .get("properties")
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("list"))
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("items"))
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("properties"))
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("models"))
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("items"))
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("enum"))
            .and_then(|v| v.as_array())
            .expect("models enum");
        let expected: Vec<Value> = DEFAULT_AGENT_NAMES
            .iter()
            .map(|name| Value::String((*name).to_string()))
            .collect();
        assert_eq!(*item_enum, expected);
    }

    #[test]
    fn schema_omits_agents_when_disabled() {
        let active_agents = vec!["codex-plan".to_string()];
        let schema = build_schema(
            &active_agents,
            SchemaFeatures {
                include_agents: false,
                ..SchemaFeatures::default()
            },
        );
        let props = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .expect("schema properties");
        assert!(!props.contains_key("agents"));
        let required = schema
            .get("required")
            .and_then(|v| v.as_array())
            .expect("required array");
        assert!(!required.contains(&json!("agents")));
        assert!(!required.contains(&json!("goal")));
    }

    #[test]
    fn schema_marks_goal_required_with_bootstrap_description() {
        let mut features = SchemaFeatures::default();
        features.include_goal_field = true;
        let schema = build_schema(&Vec::new(), features);
        let required = schema
            .get("required")
            .and_then(|v| v.as_array())
            .expect("required array");
        assert!(required.contains(&json!("goal")), "goal should be required");

        let props = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .expect("schema properties");
        let goal = props
            .get("goal")
            .and_then(|v| v.as_object())
            .expect("goal schema");
        let description = goal
            .get("description")
            .and_then(|v| v.as_str())
            .expect("goal description");
        assert!(description.contains("primary coding goal"));
    }

    #[test]
    fn developer_message_uses_bootstrap_instructions_when_deriving_goal() {
        let (_, _intro_bootstrap, primary_bootstrap) = build_developer_message(
            "Deriving goal from recent conversation",
            "Env",
            None,
            true,
        );
        assert!(primary_bootstrap.contains("You are preparing to start Auto Drive"));

        let (_, _intro_normal, primary_normal) =
            build_developer_message("Ship feature", "Env", None, false);
        assert!(primary_normal.contains("Ship feature"));
        assert!(!primary_normal.contains("You are preparing to start Auto Drive"));
    }

    #[test]
    fn parse_decision_new_schema() {
        let raw = r#"{
            "finish_status": "continue",
            "status_title": "Dispatching fix",
            "status_sent_to_user": "Ran smoke tests while validating the fix.",
            "prompt_sent_to_cli": "Apply the patch for the failing test",
            "agents": {
                "timing": "blocking",
                "list": [
                    {"prompt": "Draft alternative fix", "write": false, "context": "Consider module B", "models": ["codex-plan"]}
                ]
            }
        }"#;

        let (decision, _) = parse_decision(raw).expect("parse new schema decision");
        assert_eq!(decision.status, AutoCoordinatorStatus::Continue);
        assert_eq!(
            decision.status_sent_to_user.as_deref(),
            Some("Ran smoke tests while validating the fix.")
        );
        assert_eq!(decision.status_title.as_deref(), Some("Dispatching fix"));

        let cli = decision.cli.expect("cli action expected");
        assert_eq!(cli.prompt, "Apply the patch for the failing test");
        assert!(cli.context.is_none());

        assert_eq!(
            decision.agents_timing,
            Some(AutoTurnAgentsTiming::Blocking)
        );
        assert_eq!(decision.agents.len(), 1);
        let agent = &decision.agents[0];
        assert_eq!(agent.prompt, "Draft alternative fix");
        assert_eq!(agent.write, Some(false));
        assert_eq!(
            agent.models,
            Some(vec!["codex-plan".to_string()])
        );

    }

    #[test]
    fn parse_decision_new_schema_array_backcompat() {
        let raw = r#"{
            "finish_status": "continue",
            "status_title": "Running tests",
            "status_sent_to_user": "Outlined fix before execution.",
            "prompt_sent_to_cli": "Run cargo test",
            "agents": [
                {"prompt": "Investigate benchmark", "write": false}
            ]
        }"#;

        let (decision, _) = parse_decision(raw).expect("parse array-style agents");
        assert_eq!(decision.status, AutoCoordinatorStatus::Continue);
        assert!(decision.cli.is_some());
        assert_eq!(decision.agents.len(), 1);
        assert!(decision.agents_timing.is_none());
        assert_eq!(decision.status_title.as_deref(), Some("Running tests"));
        assert_eq!(
            decision.status_sent_to_user.as_deref(),
            Some("Outlined fix before execution.")
        );
    }

    #[test]
    fn parse_decision_legacy_schema() {
        let raw = r#"{
            "finish_status": "continue",
            "progress": {"past": "Drafted fix", "current": "Running unit tests"},
            "prompt_sent_to_cli": "Run cargo test --package core"
        }"#;

        let (decision, _) = parse_decision(raw).expect("parse legacy decision");
        assert_eq!(decision.status, AutoCoordinatorStatus::Continue);
        assert_eq!(
            decision.status_sent_to_user.as_deref(),
            Some("Drafted fix")
        );
        assert_eq!(
            decision.status_title.as_deref(),
            Some("Running unit tests")
        );

        let cli = decision.cli.expect("cli action expected");
        assert_eq!(cli.prompt, "Run cargo test --package core");
        assert!(cli.context.is_none());

        assert!(decision.agents.is_empty());
        assert!(decision.agents_timing.is_none());
    }

    #[test]
    fn classify_missing_cli_prompt_is_recoverable() {
        let err = anyhow!("model response missing prompt_sent_to_cli for continue");
        let info = classify_recoverable_decision_error(&err).expect("recoverable error");
        assert!(info
            .summary
            .contains("prompt_sent_to_cli"));
        assert!(
            info
                .guidance
                .as_ref()
                .expect("guidance")
                .contains("prompt_sent_to_cli")
        );
    }

    #[test]
    fn classify_empty_field_is_recoverable() {
        let err = anyhow!("agents[*].prompt is empty");
        let info = classify_recoverable_decision_error(&err).expect("recoverable error");
        assert!(info.summary.contains("agents[*].prompt"));
        assert!(info
            .guidance
            .as_ref()
            .expect("guidance")
            .contains("agents[*].prompt"));
    }

    #[test]
    fn classify_overlong_cli_prompt_is_recoverable_and_guided() {
        let err = ensure_cli_prompt_length(&"x".repeat(CLI_PROMPT_MAX_CHARS + 1))
            .expect_err("length check should fail");
        let info = classify_recoverable_decision_error(&err).expect("recoverable error");

        assert!(
            info.summary.contains("length cap"),
            "summary should mention length issue"
        );
        let guidance = info.guidance.expect("guidance");
        assert!(guidance.contains("<=600"), "guidance should include limit");
    }

    #[test]
    fn quota_exceeded_errors_short_circuit_retries() {
        let err = anyhow!(CodexErr::QuotaExceeded);
        match classify_model_error(&err) {
            RetryDecision::Fatal(e) => {
                assert!(e.to_string().contains("Quota exceeded"));
            }
            other => panic!("expected fatal quota decision, got {other:?}"),
        }
    }

    #[test]
    fn push_unique_guidance_trims_and_dedupes() {
        let mut guidance = vec!["Keep CLI prompts short".to_string()];
        push_unique_guidance(&mut guidance, "  keep cli prompts short  ");
        assert_eq!(guidance.len(), 1, "duplicate hint should not be added");
        push_unique_guidance(&mut guidance, "Respond with JSON only");
        assert_eq!(guidance.len(), 2);
        assert!(guidance.iter().any(|hint| hint == "Respond with JSON only"));
    }

    #[test]
    fn compaction_triggers_when_projected_exceeds_threshold() {
        assert!(should_compact("gpt-5.1", 220_000, 10_000, 0, true));
        assert!(!should_compact("gpt-5.1", 100_000, 10_000, 0, true));
    }

    #[test]
    fn compaction_falls_back_to_message_limit_when_unknown_model() {
        assert!(should_compact(
            "unknown-model",
            0,
            0,
            MESSAGE_LIMIT_FALLBACK,
            false,
        ));
        assert!(!should_compact(
            "unknown-model",
            0,
            0,
            MESSAGE_LIMIT_FALLBACK.saturating_sub(1),
            false,
        ));
    }

    #[test]
    fn compaction_skip_fallback_when_context_known() {
        assert!(!should_compact(
            "gpt-5.1",
            0,
            4_000,
            MESSAGE_LIMIT_FALLBACK,
            false,
        ));
    }

    #[test]
    fn compaction_fallback_stops_once_tokens_recorded() {
        assert!(!should_compact(
            "unknown-model",
            0,
            0,
            MESSAGE_LIMIT_FALLBACK,
            true,
        ));
    }
}

#[derive(Debug, Deserialize)]
struct CoordinatorDecisionNew {
    finish_status: String,
    #[serde(default)]
    status_title: Option<String>,
    #[serde(default)]
    status_sent_to_user: Option<String>,
    #[serde(default)]
    progress: Option<ProgressPayload>,
    #[serde(default)]
    prompt_sent_to_cli: Option<String>,
    #[serde(default)]
    agents: Option<AgentsField>,
    #[serde(default)]
    goal: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProgressPayload {
    #[serde(default)]
    past: Option<String>,
    #[serde(default)]
    current: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AgentPayload {
    prompt: String,
    #[serde(default)]
    context: Option<String>,
    #[serde(default)]
    write: Option<bool>,
    #[serde(default)]
    models: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AgentsField {
    List(Vec<AgentPayload>),
    Object(AgentsPayload),
}

#[derive(Debug, Deserialize)]
struct AgentsPayload {
    #[serde(default)]
    timing: Option<AgentsTimingValue>,
    #[serde(default)]
    models: Option<Vec<String>>,
    #[serde(
        default,
        alias = "list",
        alias = "agents",
        alias = "entries",
        alias = "requests"
    )]
    requests: Vec<AgentPayload>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AgentsTimingValue {
    Parallel,
    Blocking,
}

impl From<AgentsTimingValue> for AutoTurnAgentsTiming {
    fn from(value: AgentsTimingValue) -> Self {
        match value {
            AgentsTimingValue::Parallel => AutoTurnAgentsTiming::Parallel,
            AgentsTimingValue::Blocking => AutoTurnAgentsTiming::Blocking,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CoordinatorDecisionLegacy {
    finish_status: String,
    #[serde(default)]
    progress_past: Option<String>,
    #[serde(default)]
    progress_current: Option<String>,
    #[serde(default)]
    cli_context: Option<String>,
    #[serde(default)]
    cli_prompt: Option<String>,
    #[serde(default)]
    goal: Option<String>,
}

struct ParsedCoordinatorDecision {
    status: AutoCoordinatorStatus,
    status_title: Option<String>,
    status_sent_to_user: Option<String>,
    cli: Option<CliAction>,
    agents_timing: Option<AutoTurnAgentsTiming>,
    agents: Vec<AgentAction>,
    goal: Option<String>,
    response_items: Vec<ResponseItem>,
    token_usage: Option<TokenUsage>,
    model_slug: String,
}

#[derive(Debug, Clone)]
struct CliAction {
    prompt: String,
    context: Option<String>,
    suppress_ui_context: bool,
}

#[derive(Debug, Clone)]
struct AgentAction {
    prompt: String,
    context: Option<String>,
    write: Option<bool>,
    models: Option<Vec<String>>,
}

struct DecisionFailure {
    error: anyhow::Error,
    schema_label: &'static str,
    output_text: Option<String>,
}

impl DecisionFailure {
    fn new(error: anyhow::Error, schema_label: &'static str, output_text: Option<String>) -> Self {
        Self {
            error,
            schema_label,
            output_text,
        }
    }
}

pub fn start_auto_coordinator(
    event_tx: AutoCoordinatorEventSender,
    goal_text: String,
    conversation: Vec<ResponseItem>,
    config: Config,
    debug_enabled: bool,
    derive_goal_from_history: bool,
) -> Result<AutoCoordinatorHandle> {
    if std::env::var_os("CODEX_DEBUG_AUTO_COORDINATOR").is_some() {
        eprintln!(
            "start_auto_coordinator invoked\n{:?}",
            std::backtrace::Backtrace::force_capture()
        );
    }

    let (cmd_tx, cmd_rx) = mpsc::channel();
    let thread_tx = cmd_tx.clone();
    let cancel_token = CancellationToken::new();
    let thread_cancel = cancel_token.clone();

    // Keep plenty of stack headroom for deep JSON Schema validation stacks and
    // large coordinator transcripts. The previous 256 KiB budget could
    // overflow when validation recursed through long assistant responses.
    let builder = std::thread::Builder::new()
        .name("code-auto-coordinator".to_string())
        .stack_size(1 * 1024 * 1024);
    let handle = builder.spawn(move || {
        if let Err(err) = run_auto_loop(
            event_tx,
            goal_text,
            conversation,
            config,
            cmd_rx,
            debug_enabled,
            thread_cancel,
            derive_goal_from_history,
        ) {
            tracing::error!("auto coordinator loop error: {err:#}");
        }
    });

    if handle.is_err() {
        tracing::error!("auto coordinator spawn failed: {:#}", handle.unwrap_err());
        return Err(anyhow!("auto coordinator worker unavailable"));
    }

    Ok(AutoCoordinatorHandle {
        tx: thread_tx,
        cancel_token,
    })
}

#[tracing::instrument(skip_all, fields(goal = %goal_text, derive_goal = derive_goal_from_history))]
fn run_auto_loop(
    event_tx: AutoCoordinatorEventSender,
    goal_text: String,
    initial_conversation: Vec<ResponseItem>,
    config: Config,
    cmd_rx: Receiver<AutoCoordinatorCommand>,
    debug_enabled: bool,
    cancel_token: CancellationToken,
    derive_goal_from_history: bool,
) -> Result<()> {
    let mut config = config;
    if config.model.trim().is_empty() {
        config.model = MODEL_SLUG.to_string();
    }
    if matches!(config.model_reasoning_effort, ReasoningEffort::None) {
        config.model_reasoning_effort = ReasoningEffort::High;
    }
    let requested_effort: code_protocol::config_types::ReasoningEffort =
        config.model_reasoning_effort.into();
    let clamped_effort =
        clamp_reasoning_effort_for_model(&config.model, requested_effort);
    config.model_reasoning_effort = ReasoningEffort::from(clamped_effort);
    let allowed_verbosity = supported_text_verbosity_for_model(&config.model);
    config.model_text_verbosity = allowed_verbosity
        .iter()
        .find(|v| matches!(v, TextVerbosity::Medium))
        .copied()
        .or_else(|| allowed_verbosity.first().copied())
        .unwrap_or(TextVerbosity::Medium);
    let compact_prompt_text =
        resolve_compact_prompt_text(config.compact_prompt_override.as_deref());

    let preferred_auth = if config.using_chatgpt_auth {
        code_protocol::mcp_protocol::AuthMode::ChatGPT
    } else {
        code_protocol::mcp_protocol::AuthMode::ApiKey
    };
    let code_home = config.code_home.clone();
    let responses_originator_header = config.responses_originator_header.clone();
    let auth_mgr = AuthManager::shared_with_mode_and_originator(
        code_home,
        preferred_auth,
        responses_originator_header,
    );
    let model_provider = config.model_provider.clone();
    let model_reasoning_summary = config.model_reasoning_summary;
    let model_text_verbosity = config.model_text_verbosity;
    let sandbox_policy = config.sandbox_policy.clone();
    let mut time_budget = config.max_run_seconds.map(|secs| {
        let total = Duration::from_secs(secs);
        let deadline = config
            .max_run_deadline
            .unwrap_or_else(|| Instant::now() + total);
        AutoTimeBudget::new(deadline, total)
    });
    let coordinator_turn_cap = config.auto_drive.coordinator_turn_cap;
    let config = Arc::new(config);
    let active_agent_names = get_enabled_agents(&config.agents);
    let client = Arc::new(ModelClient::new(
        config.clone(),
        Some(auth_mgr),
        None,
        model_provider,
        config.model_reasoning_effort,
        model_reasoning_summary,
        model_text_verbosity,
        Uuid::new_v4(),
        Arc::new(Mutex::new(
            DebugLogger::new(debug_enabled)
                .unwrap_or_else(|_| DebugLogger::new(false).expect("debug logger")),
        )),
    ));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("creating runtime for auto coordinator")?;

    let auto_instructions = match runtime.block_on(read_auto_drive_docs(config.as_ref())) {
        Ok(Some(text)) => {
            let trimmed = text.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        Ok(None) => None,
        Err(err) => {
            warn!("failed to read AUTO_AGENTS.md instructions: {err:#}");
            None
        }
    };
    let sandbox_label = if matches!(sandbox_policy, SandboxPolicy::DangerFullAccess) {
        "full access"
    } else {
        "limited sandbox"
    };
    let environment_details = format_environment_details(sandbox_label);
    let coordinator_prompt = read_coordinator_prompt(config.as_ref());
    let (coordinator_prompt_message, mut base_developer_intro, mut primary_goal_message) = build_developer_message(
        &goal_text,
        &environment_details,
        coordinator_prompt.as_deref(),
        derive_goal_from_history,
    );
    let git_repo_present = run_git_command(["rev-parse", "--is-inside-work-tree"])
        .as_deref()
        .map(|value| value == "true")
        .unwrap_or(false);
    if !git_repo_present {
        base_developer_intro.push_str(
            "\n\nThe current working directory is not a git repository. Auto Drive must only launch read-only agents. If a request includes write: true, downgrade it to read-only.",
        );
    }
    let mut schema_features = SchemaFeatures::from_auto_settings(&config.auto_drive);
    if derive_goal_from_history {
        schema_features.include_goal_field = true;
    }
    let include_agents = schema_features.include_agents;
    let mut pending_conversation =
        Some(Arc::<[ResponseItem]>::from(filter_popular_commands(initial_conversation)));
    let mut decision_seq: u64 = 0;
    let mut pending_ack_seq: Option<u64> = None;
    let mut queued_updates: VecDeque<Arc<[ResponseItem]>> = VecDeque::new();
    if !derive_goal_from_history {
        if let Some(seed) = build_initial_planning_seed(&goal_text, include_agents) {
            let transcript_item = make_message("assistant", seed.response_json.clone());
            let cli_action = AutoTurnCliAction {
                prompt: seed.cli_prompt.clone(),
                context: Some(seed.goal_message.clone()),
                suppress_ui_context: true,
            };
            let event = AutoCoordinatorEvent::Decision {
                seq: decision_seq,
                status: AutoCoordinatorStatus::Continue,
                status_title: Some(seed.status_title.clone()),
                status_sent_to_user: Some(seed.status_sent_to_user.clone()),
                goal: Some(goal_text.clone()),
                cli: Some(cli_action),
                agents_timing: seed.agents_timing,
                agents: Vec::new(),
                transcript: vec![transcript_item],
            };
            event_tx.send(event);
            pending_ack_seq = Some(decision_seq);
            pending_conversation = None;
        }
    }
    let mut schema = build_schema(&active_agent_names, schema_features);
    let platform = std::env::consts::OS;
    debug!("[Auto coordinator] starting: goal={goal_text} platform={platform}");

    let mut stopped = false;
    let mut requests_completed: u64 = 0;
    let mut consecutive_decision_failures: u32 = 0;
    let mut session_metrics = SessionMetrics::default();
    let mut coordinator_turns_seen: u32 = 0;
    let mut active_model_slug = config.model.clone();
    let mut prev_compact_summary: Option<String> = None;

    loop {
        if stopped {
            break;
        }

        let mut next_conversation: Option<Arc<[ResponseItem]>> = None;

        if let Some(conv) = pending_conversation.take() {
            if let Some(pending_seq) = pending_ack_seq {
                tracing::debug!(target: "auto_drive::coordinator", pending_seq, "queueing conversation until ack");
                queued_updates.push_back(conv);
            } else {
                next_conversation = Some(conv);
            }
        } else if pending_ack_seq.is_none() {
            if let Some(conv) = queued_updates.pop_front() {
                next_conversation = Some(conv);
            }
        }

        if let Some(conv) = next_conversation {
            if cancel_token.is_cancelled() {
                stopped = true;
                continue;
            }

            let conv = conv.as_ref().to_vec();
            let mut conv = filter_popular_commands(conv);
            let compaction_result = maybe_compact(
                &runtime,
                client.as_ref(),
                &event_tx,
                &mut conv,
                &session_metrics,
                prev_compact_summary.as_deref(),
                &active_model_slug,
                &compact_prompt_text,
            );
            let conv = Arc::<[ResponseItem]>::from(conv);
            if matches!(compaction_result, CompactionResult::Completed { .. }) {
                event_tx.send(AutoCoordinatorEvent::CompactedHistory {
                    conversation: Arc::clone(&conv),
                    show_notice: true,
                });
            }
            if let CompactionResult::Completed { summary_text } = compaction_result {
                prev_compact_summary = summary_text;
            }
            let developer_intro = base_developer_intro.as_str();
            let mut retry_conversation: Option<Vec<ResponseItem>> = None;
            let time_budget_message = time_budget.as_mut().and_then(|budget| budget.maybe_nudge());
            let time_budget_deadline = time_budget.as_ref().map(|budget| budget.deadline);
            let loop_warning = session_metrics.loop_detection_warning();
            match request_coordinator_decision(
                &runtime,
                client.as_ref(),
                developer_intro,
                &primary_goal_message,
                coordinator_prompt_message.as_deref(),
                time_budget_message.as_deref(),
                time_budget_deadline,
                loop_warning.as_deref(),
                &schema,
                Arc::clone(&conv),
                auto_instructions.as_deref(),
                &event_tx,
                &cancel_token,
                &active_model_slug,
            ) {
                Ok(ParsedCoordinatorDecision {
                    status,
                    status_title,
                    status_sent_to_user,
                    goal,
                    cli,
                    mut agents_timing,
                    mut agents,
                    mut response_items,
                    token_usage,
                    model_slug,
                }) => {
                    retry_conversation.take();
                    coordinator_turns_seen = coordinator_turns_seen.saturating_add(1);
                    if let Some(usage) = token_usage.as_ref() {
                        session_metrics.record_turn(usage);
                        emit_auto_drive_metrics(&event_tx, &session_metrics);
                    } else {
                        let estimated_prompt_tokens: u64 = conv
                            .iter()
                            .map(|item| estimate_item_tokens(item) as u64)
                            .sum();
                        session_metrics.record_turn_without_usage(estimated_prompt_tokens);
                        emit_auto_drive_metrics(&event_tx, &session_metrics);
                    }
                    active_model_slug = model_slug;
                    if !include_agents {
                        agents_timing = None;
                        agents.clear();
                    }
                    consecutive_decision_failures = 0;

                    if coordinator_turn_cap > 0
                        && coordinator_turns_seen >= coordinator_turn_cap
                        && matches!(status, AutoCoordinatorStatus::Continue)
                    {
                        warn!(
                            "auto coordinator turn cap reached ({coordinator_turns_seen}/{coordinator_turn_cap}); stopping"
                        );
                        decision_seq = decision_seq.wrapping_add(1);
                        let current_seq = decision_seq;
                        let event = AutoCoordinatorEvent::Decision {
                            seq: current_seq,
                            status: AutoCoordinatorStatus::Failed,
                            status_title: Some("Turn limit reached".to_string()),
                            status_sent_to_user: Some(format!(
                                "Stopped after {coordinator_turns_seen} coordinator turns (cap={coordinator_turn_cap}) to prevent a runaway session."
                            )),
                            goal,
                            cli: None,
                            agents_timing: None,
                            agents: Vec::new(),
                            transcript: std::mem::take(&mut response_items),
                        };
                        pending_ack_seq = Some(current_seq);
                        event_tx.send(event);
                        stopped = true;
                        continue;
                    }

                    if let Some(goal_text) = goal
                        .as_ref()
                        .map(|value| value.trim())
                        .filter(|value| !value.is_empty())
                    {
                        primary_goal_message = format!("**Primary Goal**\n{goal_text}");
                        if schema_features.include_goal_field {
                            schema_features.include_goal_field = false;
                            schema = build_schema(&active_agent_names, schema_features);
                        }
                    }
                    decision_seq = decision_seq.wrapping_add(1);
                    let current_seq = decision_seq;
                    if matches!(status, AutoCoordinatorStatus::Continue) {
                        let event = AutoCoordinatorEvent::Decision {
                            seq: current_seq,
                            status,
                            status_title: status_title.clone(),
                            status_sent_to_user: status_sent_to_user.clone(),
                            goal: goal.clone(),
                            cli: cli.as_ref().map(cli_action_to_event),
                            agents_timing,
                            agents: agents
                                .iter()
                                .map(|action| {
                                    agent_action_to_event_with_write_guard(
                                        action,
                                        git_repo_present,
                                    )
                                })
                                .collect(),
                            transcript: std::mem::take(&mut response_items),
                        };
                        pending_ack_seq = Some(current_seq);
                        event_tx.send(event);
                        continue;
                    }

                    let decision_event = PendingDecision {
                        seq: current_seq,
                        status,
                        status_title,
                        status_sent_to_user,
                        goal: goal.clone(),
                        cli: cli.as_ref().map(cli_action_to_event),
                        agents_timing,
                        agents: agents
                            .iter()
                            .map(|action| {
                                agent_action_to_event_with_write_guard(action, git_repo_present)
                            })
                            .collect(),
                        transcript: response_items,
                    };

                    let should_stop = matches!(decision_event.status, AutoCoordinatorStatus::Failed);
                    pending_ack_seq = Some(current_seq);
                    event_tx.send(decision_event.into_event());
                    stopped = should_stop;
                    continue;
                }
                Err(failure) => {
                    let DecisionFailure {
                        error,
                        schema_label,
                        output_text,
                    } = failure;
                    let raw_output = output_text.clone();
                    if error.downcast_ref::<AutoCoordinatorCancelled>().is_some() {
                        stopped = true;
                        continue;
                    }
                    if let Some(recoverable) = classify_recoverable_decision_error(&error) {
                        consecutive_decision_failures =
                            consecutive_decision_failures.saturating_add(1);
                        if consecutive_decision_failures <= MAX_DECISION_RECOVERY_ATTEMPTS {
                            let attempt = consecutive_decision_failures;

                            const OVERLONG_MSG: &str = "ERROR: Your last prompt_sent_to_cli was greater than 600 characters and was not sent to the CLI. Please try again with a shorter prompt. You must keep prompts succinct (<=600 chars) to give the CLI autonomy to decide how to best execute the task.";

                            let mut already_shared_raw = false;
                            if let Some(raw) = raw_output.as_ref() {
                                // Assistant message should show the model's raw output so the UI sees the failed response.
                                let assistant_msg = make_message("assistant", raw.clone());
                                retry_conversation
                                    .get_or_insert_with(|| conv.as_ref().to_vec())
                                    .push(assistant_msg);
                                already_shared_raw = true;
                            }

                            warn!(
                                "auto coordinator decision validation failed (attempt {}/{}): {:#}",
                                attempt,
                                MAX_DECISION_RECOVERY_ATTEMPTS,
                                error
                            );
                            let raw_excerpt = if already_shared_raw {
                                None
                            } else {
                                raw_output.as_deref().map(summarize_json_for_debug)
                            };
                            let mut message = format!(
                                "Coordinator response invalid (attempt {attempt}/{MAX_DECISION_RECOVERY_ATTEMPTS}): {}. Retrying…\nSchema: {schema_label}",
                                recoverable.summary
                            );
                            if let Some(excerpt) = raw_excerpt.as_ref() {
                                message.push_str("\nLast JSON:\n");
                                message.push_str(&indent_lines(excerpt, "    "));
                            } else if already_shared_raw {
                                message.push_str("\nSee assistant message above for the full model output that failed validation.");
                            }
                            let _ = event_tx.send(AutoCoordinatorEvent::Thinking {
                                delta: message,
                                summary_index: None,
                            });
                            {
                                let retry_vec =
                                    retry_conversation.get_or_insert_with(|| conv.as_ref().to_vec());
                                let mut developer_note = format!(
                                    "Previous coordinator response failed validation (attempt {attempt}/{MAX_DECISION_RECOVERY_ATTEMPTS}).\nError: {error}\nSchema: {schema_label}"
                                );
                                if let Some(guidance) = recoverable.guidance.as_ref() {
                                    developer_note.push_str("\nGuidance: ");
                                    developer_note.push_str(guidance);
                                }
                                if already_shared_raw {
                                    developer_note.push_str("\n");
                                    developer_note.push_str(OVERLONG_MSG);
                                } else if let Some(excerpt) = raw_excerpt {
                                    developer_note.push_str("\nLast JSON:\n");
                                    developer_note.push_str(&indent_lines(&excerpt, "    "));
                                    developer_note.push_str("\n");
                                    developer_note.push_str(OVERLONG_MSG);
                                }
                                retry_vec.push(make_message("developer", developer_note));
                            }
                            let retry_snapshot = retry_conversation
                                .take()
                                .map(Arc::<[ResponseItem]>::from)
                                .unwrap_or_else(|| Arc::clone(&conv));
                            // Keep the model and UI in sync with the full conversation, but avoid spamming a compaction notice.
                            let _ = event_tx.send(AutoCoordinatorEvent::CompactedHistory {
                                conversation: Arc::clone(&retry_snapshot),
                                show_notice: false,
                            });
                            // Show a user-facing action entry in the Auto Drive card (does not go to the model).
                            let _ = event_tx.send(AutoCoordinatorEvent::Action {
                                message: "Retrying prompt generation after the previous response was too long to send to the CLI.".to_string(),
                            });
                            pending_conversation = Some(retry_snapshot);
                            continue;
                        }
                        warn!(
                            "auto coordinator validation retry limit exceeded after {} attempts: {:#}",
                            MAX_DECISION_RECOVERY_ATTEMPTS,
                            error
                        );
                    }
                    consecutive_decision_failures = 0;
                    decision_seq = decision_seq.wrapping_add(1);
                    let current_seq = decision_seq;
                    let event = AutoCoordinatorEvent::Decision {
                        seq: current_seq,
                        status: AutoCoordinatorStatus::Failed,
                        status_title: Some("Coordinator error".to_string()),
                        status_sent_to_user: Some(format!("Encountered an error: {error}")),
                        goal: None,
                        cli: None,
                        agents_timing: None,
                        agents: Vec::new(),
                        transcript: Vec::new(),
                    };
                    pending_ack_seq = Some(current_seq);
                    event_tx.send(event);
                    stopped = true;
                    continue;
                }
            }
        }

        match cmd_rx.recv() {
            Ok(AutoCoordinatorCommand::AckDecision { seq }) => {
                if pending_ack_seq == Some(seq) {
                    tracing::debug!(target: "auto_drive::coordinator", seq, "ack received");
                    pending_ack_seq = None;
                    if let Some(queued) = queued_updates.pop_front() {
                        pending_conversation = Some(queued);
                    }
                } else {
                    tracing::debug!(target: "auto_drive::coordinator", pending = ?pending_ack_seq, seq, "ignoring ack for unexpected sequence");
                }
            }
            Ok(AutoCoordinatorCommand::HandleUserPrompt { _prompt, conversation }) => {
                let developer_intro = base_developer_intro.as_str();
                let base_conversation = conversation.as_ref().to_vec();
                let conversation_snapshot = Arc::<[ResponseItem]>::from(base_conversation);
                let schema = user_turn_schema();
                let time_budget_message = time_budget.as_mut().and_then(|budget| budget.maybe_nudge());
                let time_budget_deadline = time_budget.as_ref().map(|budget| budget.deadline);
                match request_user_turn_decision(
                    &runtime,
                    client.as_ref(),
                    developer_intro,
                    &primary_goal_message,
                    coordinator_prompt_message.as_deref(),
                    time_budget_message.as_deref(),
                    time_budget_deadline,
                    &schema,
                    Arc::clone(&conversation_snapshot),
                    auto_instructions.as_deref(),
                    &event_tx,
                    &cancel_token,
                    &active_model_slug,
                ) {
                    Ok((user_response, cli_command)) => {
                        let mut updated_conversation = conversation_snapshot.as_ref().to_vec();
                        if let Some(response_text) = user_response.clone() {
                            updated_conversation.push(make_message("assistant", response_text.clone()));
                        }
                        pending_conversation = Some(Arc::<[ResponseItem]>::from(updated_conversation));
                        event_tx.send(AutoCoordinatorEvent::UserReply {
                            user_response,
                            cli_command,
                        });
                    }
                    Err(failure) => {
                        let DecisionFailure {
                            error,
                            schema_label,
                            output_text,
                        } = failure;
                        tracing::warn!(
                            "failed to handle coordinator user prompt (schema={}): {:#}",
                            schema_label,
                            error
                        );
                        if let Some(raw) = output_text.as_ref() {
                            tracing::debug!(
                                "user-turn raw response (schema={}): {}",
                                schema_label,
                                raw
                            );
                        }
                        event_tx.send(AutoCoordinatorEvent::UserReply {
                            user_response: Some(format!("Coordinator error: {error}")),
                            cli_command: None,
                        });
                    }
                }
            }
            Ok(AutoCoordinatorCommand::UpdateConversation(conv)) => {
                requests_completed = requests_completed.saturating_add(1);
                consecutive_decision_failures = 0;
                let conv = conv.as_ref().to_vec();
                let filtered = Arc::<[ResponseItem]>::from(filter_popular_commands(conv));
                if let Some(pending_seq) = pending_ack_seq {
                    tracing::debug!(target: "auto_drive::coordinator", pending_seq, "queueing update while awaiting ack");
                    session_metrics.record_replay();
                    queued_updates.push_back(filtered);
                } else if pending_conversation.is_some() {
                    session_metrics.record_replay();
                    queued_updates.push_back(filtered);
                } else {
                    pending_conversation = Some(filtered);
                }
            }
            Ok(AutoCoordinatorCommand::Stop) | Err(_) => {
                stopped = true;
                event_tx.send(AutoCoordinatorEvent::StopAck);
                pending_ack_seq = None;
                queued_updates.clear();
            }
        }
    }

    Ok(())
}

fn filter_popular_commands(items: Vec<ResponseItem>) -> Vec<ResponseItem> {
    items
        .into_iter()
        .filter(|item| !is_popular_commands_message(item))
        .collect()
}

fn is_popular_commands_message(item: &ResponseItem) -> bool {
    match item {
        ResponseItem::Message { role, content, .. } if role.eq_ignore_ascii_case("user") => {
            content.iter().any(|c| match c {
                ContentItem::InputText { text } => text.contains("Popular commands:"),
                _ => false,
            })
        }
        _ => false,
    }
}
fn read_coordinator_prompt(config: &Config) -> Option<String> {
    let trimmed = COORDINATOR_PROMPT.trim();
    if trimmed.is_empty() {
        None
    } else {
        if config.max_run_seconds.is_some() {
            Some(format!("{trimmed}\n\n# Time Budget\n{TIMEBOXED_EXEC_COORDINATOR_GUIDANCE}"))
        } else {
            Some(trimmed.to_string())
        }
    }
}

fn build_developer_message(
    goal_text: &str,
    environment_details: &str,
    coordinator_prompt: Option<&str>,
    derive_goal_from_history: bool,
) -> (Option<String>, String, String) {
    let prompt_body = coordinator_prompt.unwrap_or("").trim();
    let coordinator_message = if prompt_body.is_empty() {
        None
    } else {
        Some(prompt_body.to_string())
    };
    let intro = format!("Environment:
{environment_details}");
    let primary_goal = if derive_goal_from_history {
        "**Primary Goal**\nYou are preparing to start Auto Drive. Review the recent conversation history and identify the single primary coding goal the assistant should pursue next.".to_string()
    } else {
        format!("**Primary Goal**\n{}", goal_text)
    };
    (coordinator_message, intro, primary_goal)
}

struct InitialPlanningSeed {
    response_json: String,
    cli_prompt: String,
    goal_message: String,
    status_title: String,
    status_sent_to_user: String,
    agents_timing: Option<AutoTurnAgentsTiming>,
}

fn build_initial_planning_seed(goal_text: &str, include_agents: bool) -> Option<InitialPlanningSeed> {
    let goal = goal_text.trim();
    if goal.is_empty() {
        return None;
    }

    let cli_prompt = if include_agents {
        "Please provide a clear plan to best achieve the Primary Goal. If this is not a trivial task, launch agents and use your tools to research the best approach. If this is a trivial task, or the plan is already in the conversation history, immediately provide the plan. Judge the length of research and planning you perform based on the complexity of the task. For more complex tasks, you can break the plan into workstreams that can be performed at the same time."
            .to_string()
    } else {
        "Please provide a clear plan to best achieve the Primary Goal. If this is not a trivial task, use your tools to research the best approach. If this is a trivial task, or the plan is already in the conversation history, immediately provide the plan. Judge the length of research and planning you perform based on the complexity of the task."
            .to_string()
    };

    let response_json = format!(
        "{{\"finish_status\":\"continue\",\"status_title\":\"Planning\",\"status_sent_to_user\":\"Started initial planning phase\",\"prompt_sent_to_cli\":\"{cli_prompt}\"}}"
    );

    Some(InitialPlanningSeed {
        response_json,
        cli_prompt,
        goal_message: format!("Primary Goal: {goal}"),
        status_title: "Planning route".to_string(),
        status_sent_to_user: "Planning best route to reach the goal.".to_string(),
        agents_timing: if include_agents {
            Some(AutoTurnAgentsTiming::Parallel)
        } else {
            None
        },
    })
}

fn format_environment_details(sandbox: &str) -> String {
    let cwd = std::env::current_dir()
        .map(|dir| dir.display().to_string())
        .unwrap_or_else(|_| "<unknown>".to_string());
    let branch = run_git_command(["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "<unknown>".to_string());
    let git_status_raw = run_git_command(["status", "--short"]);
    let git_status = match git_status_raw {
        Some(raw) if raw.trim().is_empty() => "  clean".to_string(),
        Some(raw) => raw
            .lines()
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
        None => "  <git status unavailable>".to_string(),
    };
    format!(
        "- Access: {sandbox}\n- Working directory: {cwd}\n- Git branch: {branch}\n- Git status:\n{git_status}"
    )
}

fn run_git_command<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    if args.iter().any(|arg| matches!(*arg, "pull" | "checkout" | "merge" | "apply")) {
        code_core::review_coord::bump_snapshot_epoch();
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|text| text.trim_end().to_string())
}

#[derive(Clone, Copy)]
struct SchemaFeatures {
    include_agents: bool,
    include_goal_field: bool,
}

impl SchemaFeatures {
    fn from_auto_settings(settings: &AutoDriveSettings) -> Self {
        Self {
            include_agents: settings.agents_enabled,
            include_goal_field: false,
        }
    }
}

impl Default for SchemaFeatures {
    fn default() -> Self {
        Self {
            include_agents: true,
            include_goal_field: false,
        }
    }
}

fn build_schema(active_agents: &[String], features: SchemaFeatures) -> Value {
    let models_enum_values: Vec<Value> = active_agents
        .iter()
        .map(|name| Value::String(name.clone()))
        .collect();

    let models_items_schema = {
        let mut schema = json!({
            "type": "string",
        });
        if !models_enum_values.is_empty() {
            schema["enum"] = Value::Array(models_enum_values.clone());
        }
        schema
    };

    let models_description = build_model_guide_description(active_agents);

    let models_request_property = json!({
        "type": "array",
        "description": models_description,
        "items": models_items_schema,
    });

    let mut properties = serde_json::Map::new();
    let mut required: Vec<Value> = Vec::new();

    properties.insert(
        "finish_status".to_string(),
        json!({
            "type": "string",
            "enum": ["continue", "finish_success", "finish_failed"],
            "description": "Prefer 'continue' unless the mission is fully complete or truly blocked. Always consider what further work might be possible to confirm the goal is complete before ending."
        }),
    );
    required.push(Value::String("finish_status".to_string()));

    if features.include_goal_field {
        properties.insert(
            "goal".to_string(),
            json!({
                "type": "string",
                "minLength": 4,
                "maxLength": 200,
                "description": "Provide the single primary coding goal derived from the recent conversation history to begin Auto Drive without a user-supplied prompt."
            }),
        );
        required.push(Value::String("goal".to_string()));
    }

    properties.insert(
        "status_title".to_string(),
        json!({
            "type": ["string", "null"],
            "minLength": 2,
            "maxLength": 80,
            "description": "1-4 words, present-tense, what you asked the CLI to work on now."
        }),
    );
    required.push(Value::String("status_title".to_string()));

    properties.insert(
        "status_sent_to_user".to_string(),
        json!({
            "type": ["string", "null"],
            "minLength": 4,
            "maxLength": 600,
            "description": "1-2 sentences shown to the user explaining what you asked the CLI to work on now. Will be shown in the UI to keep the user updated on the progress."
        }),
    );
    required.push(Value::String("status_sent_to_user".to_string()));

    // NOTE: We intentionally omit `maxLength` here. Some providers truncate
    // responses to satisfy schema length caps, which would hide overlong
    // prompts. We validate length after parsing and treat >600 as recoverable
    // so the coordinator can retry with guidance instead of silently chopping.
    properties.insert(
        "prompt_sent_to_cli".to_string(),
        json!({
            "type": ["string", "null"],
            "minLength": CLI_PROMPT_MIN_CHARS,
            "description": "Instruction sent to the CLI to push it forward with the task (4-600 chars). Write this like a human maintainer pushing the CLI forwards, without digging too deep into the technical side. Provide when finish_status is 'continue'. Keep it high-level; the CLI has more context and tools than you do. e.g. 'Execute the first two steps of the plan you provided in parellel using agents.' NEVER ask the CLI to show you files so you solve problems directly. ALWAYS allow the CLI to take control. You are the COORDINATOR not the WORKER. Prompts over 600 characters will be rejected as this indicates the CLI is not being given sufficient autonomy."
        }),
    );
    required.push(Value::String("prompt_sent_to_cli".to_string()));

    if features.include_agents {
        properties.insert(
            "agents".to_string(),
            json!({
                "type": ["object", "null"],
                "additionalProperties": false,
                "description": "Parallel help agents for the CLI to spawn. Use often. Agents are faster, parallelize work and allow exploration of a range of approaches.",
                "properties": {
                    "timing": {
                        "type": "string",
                        "enum": ["parallel", "blocking"],
                        "description": "Parallel: run while the CLI works. Blocking: wait for results before the CLI executes the prompt you provided."
                    },
                    "list": {
                        "type": "array",
                        "maxItems": 5,
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "write": {
                                    "type": "boolean",
                                    "description": "Creates an isolated worktree for each agent and enable writes to that worktree. Default false so that the agent can only read files."
                                },
                                "context": {
                                    "type": ["string", "null"],
                                    "maxLength": 1500,
                                    "description": "Background details (agents can not see the conversation - you must provide ALL neccessary information here). You might want to include parts of the plan or conversation history relevant to the work given to the agent."
                                },
                                "prompt": {
                                    "type": "string",
                                    "minLength": 8,
                                    "maxLength": 400,
                                    "description": "Outcome-oriented instruction (what to produce)."
                                },
                                "models": models_request_property.clone()
                            },
                            "required": ["prompt", "context", "write", "models"]
                        },
                        "description": "Up to 3 batches per turn with up to 4 agents in each. Use agents whenever it will help to source a variety of opinions when planning/researching or when there a mulitple workstreams which can be extecuted at once. Instruct the agent to carefully merge in the results of the agents work. Another great reason to use agents is that it helps to split the work up in small batches with a new context history - this speeds up work and dramatically improve focus. Having said that, the CLI has to be responible for merging in the results and producing the final product, so you need to balance the work given to the agents vs work given to the CLI at each step."
                    },
                },
                "required": ["timing", "list"]
            }),
        );
        required.push(Value::String("agents".to_string()));
    }

    let mut schema = serde_json::Map::new();
    schema.insert(
        "title".to_string(),
        Value::String("Coordinator Turn".to_string()),
    );
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert("additionalProperties".to_string(), Value::Bool(false));
    schema.insert("properties".to_string(), Value::Object(properties));
    schema.insert("required".to_string(), Value::Array(required));

    Value::Object(schema)
}


struct RequestStreamResult {
    output_text: String,
    response_items: Vec<ResponseItem>,
    token_usage: Option<TokenUsage>,
    model_slug: String,
}

#[tracing::instrument(skip_all, fields(conv_items = conversation.len()))]
fn request_coordinator_decision(
    runtime: &tokio::runtime::Runtime,
    client: &ModelClient,
    developer_intro: &str,
    primary_goal: &str,
    coordinator_prompt: Option<&str>,
    time_budget_message: Option<&str>,
    time_budget_deadline: Option<Instant>,
    loop_warning: Option<&str>,
    schema: &Value,
    conversation: Arc<[ResponseItem]>,
    auto_instructions: Option<&str>,
    event_tx: &AutoCoordinatorEventSender,
    cancel_token: &CancellationToken,
    preferred_model_slug: &str,
) -> Result<ParsedCoordinatorDecision, DecisionFailure> {
    let RequestStreamResult {
        output_text,
        response_items,
        token_usage,
        model_slug,
    } = request_decision(
        runtime,
        client,
        developer_intro,
        primary_goal,
        coordinator_prompt,
        time_budget_message,
        time_budget_deadline,
        loop_warning,
        schema,
        Arc::clone(&conversation),
        auto_instructions,
        event_tx,
        cancel_token,
        preferred_model_slug,
    )
    .map_err(|err| DecisionFailure::new(err, "coordinator_decision", None))?;
    if output_text.trim().is_empty() && response_items.is_empty() {
        return Err(DecisionFailure::new(
            anyhow!("coordinator stream ended without producing output (possible transient error)"),
            "coordinator_decision",
            Some(output_text),
        ));
    }
    let (mut decision, value) = parse_decision(&output_text)
        .map_err(|err| DecisionFailure::new(err, "coordinator_decision", Some(output_text.clone())))?;
    debug!("[Auto coordinator] model decision: {:?}", value);
    decision.response_items = response_items;
    decision.token_usage = token_usage;
    decision.model_slug = model_slug;
    Ok(decision)
}

fn request_decision(
    runtime: &tokio::runtime::Runtime,
    client: &ModelClient,
    developer_intro: &str,
    primary_goal: &str,
    coordinator_prompt: Option<&str>,
    time_budget_message: Option<&str>,
    time_budget_deadline: Option<Instant>,
    loop_warning: Option<&str>,
    schema: &Value,
    conversation: Arc<[ResponseItem]>,
    auto_instructions: Option<&str>,
    event_tx: &AutoCoordinatorEventSender,
    cancel_token: &CancellationToken,
    preferred_model_slug: &str,
) -> Result<RequestStreamResult> {
    match request_decision_with_model(
        runtime,
        client,
        developer_intro,
        primary_goal,
        coordinator_prompt,
        time_budget_message,
        time_budget_deadline,
        loop_warning,
        schema,
        Arc::clone(&conversation),
        auto_instructions,
        event_tx,
        cancel_token,
        preferred_model_slug,
    ) {
        Ok(result) => Ok(result),
        Err(err) => {
            let preferred = preferred_model_slug;
            let fallback_candidate = client.default_model_slug().to_string();
            let fallback_slug = if fallback_candidate.eq_ignore_ascii_case(preferred) {
                MODEL_SLUG.to_string()
            } else {
                fallback_candidate
            };
            if fallback_slug != preferred_model_slug && should_retry_with_default_model(&err) {
                debug!(
                    preferred = %preferred,
                    fallback = %fallback_slug,
                    "auto coordinator falling back to configured model after invalid model error"
                );
                let original_error = err.to_string();
                return request_decision_with_model(
                    runtime,
                    client,
                    developer_intro,
                    primary_goal,
                    coordinator_prompt.as_deref(),
                    time_budget_message,
                    time_budget_deadline,
                    loop_warning,
                    schema,
                    Arc::clone(&conversation),
                    auto_instructions,
                    event_tx,
                    cancel_token,
                    &fallback_slug,
                )
                .map_err(|fallback_err| {
                    fallback_err.context(format!(
                        "coordinator fallback with model '{}' failed after original error: {}",
                        fallback_slug, original_error
                    ))
                });
            }
            Err(err)
        }
    }
}

fn summarize_json_for_debug(raw: &str) -> String {
    let trimmed = raw.trim();
    let mut chars = trimmed.chars();
    if trimmed.chars().count() <= DEBUG_JSON_MAX_CHARS {
        return trimmed.to_string();
    }
    let mut summary: String = chars.by_ref().take(DEBUG_JSON_MAX_CHARS).collect();
    summary.push('…');
    summary
}

fn indent_lines(text: &str, prefix: &str) -> String {
    text.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tracing::instrument(skip_all, fields(conv_items = conversation.len()))]
fn request_user_turn_decision(
    runtime: &tokio::runtime::Runtime,
    client: &ModelClient,
    developer_intro: &str,
    primary_goal: &str,
    coordinator_prompt: Option<&str>,
    time_budget_message: Option<&str>,
    time_budget_deadline: Option<Instant>,
    schema: &Value,
    conversation: Arc<[ResponseItem]>,
    auto_instructions: Option<&str>,
    event_tx: &AutoCoordinatorEventSender,
    cancel_token: &CancellationToken,
    preferred_model_slug: &str,
) -> Result<(Option<String>, Option<String>), DecisionFailure> {
    // User turn decisions don't need loop warnings as they handle user prompts.
    let result = request_decision(
        runtime,
        client,
        developer_intro,
        primary_goal,
        coordinator_prompt,
        time_budget_message,
        time_budget_deadline,
        None,
        schema,
        Arc::clone(&conversation),
        auto_instructions,
        event_tx,
        cancel_token,
        preferred_model_slug,
    )
    .map_err(|err| DecisionFailure::new(err, "auto_coordinator_user_turn", None))?;
    let (user_response, cli_command) = parse_user_turn_reply(&result.output_text)
        .map_err(|err| DecisionFailure::new(err, "auto_coordinator_user_turn", Some(result.output_text.clone())))?;
    Ok((user_response, cli_command))
}

fn request_decision_with_model(
    runtime: &tokio::runtime::Runtime,
    client: &ModelClient,
    developer_intro: &str,
    primary_goal: &str,
    coordinator_prompt: Option<&str>,
    time_budget_message: Option<&str>,
    time_budget_deadline: Option<Instant>,
    loop_warning: Option<&str>,
    schema: &Value,
    conversation: Arc<[ResponseItem]>,
    auto_instructions: Option<&str>,
    event_tx: &AutoCoordinatorEventSender,
    cancel_token: &CancellationToken,
    model_slug: &str,
) -> Result<RequestStreamResult> {
    let developer_intro = developer_intro.to_string();
    let primary_goal = primary_goal.to_string();
    let time_budget_message = time_budget_message.map(|text| text.to_string());
    let loop_warning = loop_warning.map(|text| text.to_string());
    let schema = schema.clone();
    let conversation = Arc::clone(&conversation);
    let auto_instructions = auto_instructions.map(|text| text.to_string());
    let coordinator_prompt = coordinator_prompt.map(|text| text.to_string());
    let tx = event_tx.clone();
    let cancel = cancel_token.clone();
    let mut rate_limit_switch_state = RateLimitSwitchState::default();
    let classify = |error: &anyhow::Error| {
        classify_model_error_with_auto_switch(client, &mut rate_limit_switch_state, event_tx, error)
    };
    let options = RetryOptions::with_defaults(retry_max_elapsed(time_budget_deadline));

    let result = runtime.block_on(async move {
        retry_with_backoff(
            || {
                let instructions = auto_instructions.clone();
                let coordinator_prompt = coordinator_prompt.clone();
                let time_budget_message = time_budget_message.clone();
                let loop_warning = loop_warning.clone();
                let conversation = Arc::clone(&conversation);
                let prompt = build_user_turn_prompt(
                    &developer_intro,
                    &primary_goal,
                    coordinator_prompt.as_deref(),
                    time_budget_message.as_deref(),
                    loop_warning.as_deref(),
                    &schema,
                    conversation.as_ref(),
                    model_slug,
                    instructions.as_deref(),
                );
                let tx_inner = tx.clone();
                async move {
                    #[cfg(feature = "dev-faults")]
                    if let Some(fault) = next_fault(FaultScope::AutoDrive) {
                        let err = fault_to_error(fault);
                        return Err(err);
                    }
                    let mut stream = client.stream(&prompt).await?;
                    let mut out = String::new();
                    let mut response_items: Vec<ResponseItem> = Vec::new();
                    let mut reasoning_delta_accumulator = String::new();
                    let mut saw_output_text_delta = false;
                    let mut token_usage: Option<TokenUsage> = None;
                    while let Some(ev) = stream.next().await {
                        match ev {
                            Ok(ResponseEvent::OutputTextDelta { delta, .. }) => {
                                out.push_str(&delta);
                                saw_output_text_delta = true;
                            }
                            Ok(ResponseEvent::OutputItemDone { item, .. }) => {
                                if let ResponseItem::Message { content, .. } = &item {
                                    if !saw_output_text_delta {
                                        for c in content {
                                            if let ContentItem::OutputText { text } = c {
                                                out.push_str(text);
                                            }
                                        }
                                    }
                                }
                                if matches!(item, ResponseItem::Reasoning { .. }) {
                                    reasoning_delta_accumulator.clear();
                                }
                                response_items.push(item);
                                saw_output_text_delta = false;
                            }
                            Ok(ResponseEvent::ReasoningSummaryDelta {
                                delta,
                                summary_index,
                                ..
                            }) => {
                                let cleaned = strip_role_prefix(&delta);
                                reasoning_delta_accumulator.push_str(cleaned);
                                let message = cleaned.to_string();
                                tx_inner.send(AutoCoordinatorEvent::Thinking {
                                    delta: message,
                                    summary_index,
                                });
                            }
                            Ok(ResponseEvent::ReasoningContentDelta { delta, .. }) => {
                                let cleaned = strip_role_prefix(&delta);
                                reasoning_delta_accumulator.push_str(cleaned);
                                let message = cleaned.to_string();
                                tx_inner.send(AutoCoordinatorEvent::Thinking {
                                    delta: message,
                                    summary_index: None,
                                });
                            }
                            Ok(ResponseEvent::Completed { token_usage: usage, .. }) => {
                                token_usage = usage;
                                break;
                            }
                            Err(err) => return Err(err.into()),
                            _ => {}
                        }
                    }
                    if !reasoning_delta_accumulator.trim().is_empty()
                        && !response_items
                            .iter()
                            .any(|item| matches!(item, ResponseItem::Reasoning { .. }))
                    {
                        response_items.push(ResponseItem::Reasoning {
                            id: String::new(),
                            summary: Vec::new(),
                            content: Some(vec![ReasoningItemContent::ReasoningText {
                                text: reasoning_delta_accumulator.trim().to_string(),
                            }]),
                            encrypted_content: None,
                        });
                    }
                    Ok(RequestStreamResult {
                        output_text: out,
                        response_items,
                        token_usage,
                        model_slug: model_slug.to_string(),
                    })
                }
            },
            classify,
            options,
            &cancel,
            |status| {
                let human_delay = status
                    .sleep
                    .map(format_duration)
                    .unwrap_or_else(|| "0s".to_string());
                let elapsed = format_duration(status.elapsed);
                let prefix = if status.is_rate_limit {
                    "Rate limit"
                } else {
                    "Transient error"
                };
                let attempt = status.attempt;
                let resume_str = status.resume_at.and_then(|resume| {
                    let now = Instant::now();
                    if resume <= now {
                        Some("now".to_string())
                    } else {
                        let remaining = resume.duration_since(now);
                        SystemTime::now()
                            .checked_add(remaining)
                            .map(|time| {
                                let local: DateTime<Local> = time.into();
                                local.format("%Y-%m-%d %H:%M:%S").to_string()
                            })
                    }
                });
                let message = format!(
                    "{prefix} (attempt {attempt}): {}; retrying in {human_delay} (elapsed {elapsed}){}",
                    status.reason,
                    resume_str
                        .map(|s| format!("; next attempt at {s}"))
                        .unwrap_or_default()
                );
                let _ = tx.send(AutoCoordinatorEvent::Thinking {
                    delta: message,
                    summary_index: None,
                });
            },
        )
        .await
    });

    match result {
        Ok(output) => Ok(output),
        Err(RetryError::Aborted) => Err(anyhow!(AutoCoordinatorCancelled)),
        Err(RetryError::Fatal(err)) => Err(err),
        Err(RetryError::Timeout { elapsed, last_error }) => Err(last_error.context(format!(
            "auto coordinator retry window exceeded after {}",
            format_duration(elapsed)
        ))),
    }
}

fn build_user_turn_prompt(
    developer_intro: &str,
    primary_goal: &str,
    coordinator_prompt: Option<&str>,
    time_budget_message: Option<&str>,
    loop_warning: Option<&str>,
    schema: &Value,
    conversation: &[ResponseItem],
    model_slug: &str,
    auto_instructions: Option<&str>,
) -> Prompt {
    let mut prompt = Prompt::default();
    prompt.store = true;
    prompt.session_id_override = Some(Uuid::new_v4());
    if let Some(instructions) = auto_instructions {
        let trimmed = instructions.trim();
        if !trimmed.is_empty() {
            prompt
                .input
                .push(make_message("developer", trimmed.to_string()));
        }
    }
    if let Some(prompt_text) = coordinator_prompt {
        let trimmed = prompt_text.trim();
        if !trimmed.is_empty() {
            prompt
                .prepend_developer_messages
                .push(trimmed.to_string());
        }
    }

    if let Some(message) = time_budget_message {
        let trimmed = message.trim();
        if !trimmed.is_empty() {
            prompt
                .input
                .push(make_message("developer", trimmed.to_string()));
        }
    }
    if let Some(warning) = loop_warning {
        let trimmed = warning.trim();
        if !trimmed.is_empty() {
            prompt
                .input
                .push(make_message("developer", trimmed.to_string()));
        }
    }
    prompt
        .input
        .push(make_message("developer", developer_intro.to_string()));
    prompt
        .input
        .push(make_message("developer", primary_goal.to_string()));
    prompt.input.extend(conversation.iter().cloned());
    prompt.text_format = Some(TextFormat {
        r#type: "json_schema".to_string(),
        name: Some(USER_TURN_SCHEMA_NAME.to_string()),
        strict: Some(true),
        schema: Some(schema.clone()),
    });
    prompt.model_override = Some(model_slug.to_string());
    let family = find_family_for_model(model_slug)
        .unwrap_or_else(|| derive_default_model_family(model_slug));
    prompt.model_family_override = Some(family);
    prompt.set_log_tag("auto/coordinator");
    prompt
}

fn should_retry_with_default_model(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        if let Some(code_err) = cause.downcast_ref::<CodexErr>() {
            if let CodexErr::UnexpectedStatus(err) = code_err {
                if !err.status.is_client_error() {
                    return false;
                }
                let body_lower = err.body.to_lowercase();
                return body_lower.contains("invalid model")
                    || body_lower.contains("unknown model")
                    || body_lower.contains("model_not_found")
                    || body_lower.contains("model does not exist");
            }
        }
        false
    })
}

pub(crate) fn classify_model_error(error: &anyhow::Error) -> RetryDecision {
    if let Some(code_err) = find_in_chain::<CodexErr>(error) {
        match code_err {
            CodexErr::Stream(message, _, _) => {
                return RetryDecision::RetryAfterBackoff {
                    reason: format!("model stream error: {message}"),
                };
            }
            CodexErr::Timeout => {
                return RetryDecision::RetryAfterBackoff {
                    reason: "model request timed out".to_string(),
                };
            }
            CodexErr::UnexpectedStatus(err) => {
                let status = err.status;
                let body = &err.body;
                if status == StatusCode::REQUEST_TIMEOUT || status.as_u16() == 408 {
                    return RetryDecision::RetryAfterBackoff {
                        reason: format!("provider returned {status}"),
                    };
                }
                if status.as_u16() == 499 {
                    return RetryDecision::RetryAfterBackoff {
                        reason: "client closed request (499)".to_string(),
                    };
                }
                if status == StatusCode::TOO_MANY_REQUESTS {
                    if let Some(wait_until) = parse_rate_limit_hint(body) {
                        return RetryDecision::RateLimited {
                            wait_until,
                            reason: "rate limited; waiting for reset".to_string(),
                        };
                    }
                    return RetryDecision::RetryAfterBackoff {
                        reason: "rate limited (429)".to_string(),
                    };
                }
                if status.is_client_error() {
                    return RetryDecision::Fatal(anyhow!(error.to_string()));
                }
                if status.is_server_error() {
                    return RetryDecision::RetryAfterBackoff {
                        reason: format!("server error {status}"),
                    };
                }
            }
            CodexErr::UsageLimitReached(limit) => {
                if let Some(seconds) = limit.resets_in_seconds {
                    let wait_until = compute_rate_limit_wait(Duration::from_secs(seconds));
                    return RetryDecision::RateLimited {
                        wait_until,
                        reason: "usage limit reached".to_string(),
                    };
                }
                return RetryDecision::RetryAfterBackoff {
                    reason: "usage limit reached".to_string(),
                };
            }
            CodexErr::UsageNotIncluded => {
                return RetryDecision::Fatal(anyhow!(error.to_string()));
            }
            CodexErr::AuthRefreshPermanent(_) => {
                return RetryDecision::Fatal(anyhow!(error.to_string()));
            }
            CodexErr::QuotaExceeded => {
                return RetryDecision::Fatal(anyhow!(error.to_string()));
            }
            CodexErr::ServerError(_) => {
                return RetryDecision::RetryAfterBackoff {
                    reason: error.to_string(),
                };
            }
            CodexErr::RetryLimit(status) => {
                if status.retryable {
                    return RetryDecision::RetryAfterBackoff {
                        reason: format!("retry limit exceeded (status {}), treating as transient", status.status),
                    };
                }
                return RetryDecision::Fatal(anyhow!("retry limit exceeded (status {})", status.status));
            }
            CodexErr::Reqwest(req_err) => {
                return classify_reqwest_error(req_err);
            }
            CodexErr::Io(io_err) => {
                if io_err.kind() == std::io::ErrorKind::TimedOut {
                    return RetryDecision::RetryAfterBackoff {
                        reason: "network timeout".to_string(),
                    };
                }
            }
            _ => {}
        }
    }

    if let Some(req_err) = find_in_chain::<reqwest::Error>(error) {
        return classify_reqwest_error(req_err);
    }

    if let Some(io_err) = find_in_chain::<std::io::Error>(error) {
        if io_err.kind() == std::io::ErrorKind::TimedOut {
            return RetryDecision::RetryAfterBackoff {
                reason: "network timeout".to_string(),
            };
        }
    }

    RetryDecision::Fatal(anyhow!(error.to_string()))
}

fn classify_model_error_with_auto_switch(
    client: &ModelClient,
    state: &mut RateLimitSwitchState,
    event_tx: &AutoCoordinatorEventSender,
    error: &anyhow::Error,
) -> RetryDecision {
    if let Some(code_err) = find_in_chain::<CodexErr>(error) {
        if let CodexErr::UsageLimitReached(limit) = code_err {
            if client.auto_switch_accounts_on_rate_limit()
                && auth::read_code_api_key_from_env().is_none()
            {
                if let Some(auth_manager) = client.get_auth_manager() {
                    let auth = auth_manager.auth();
                    let current_account_id = auth
                        .as_ref()
                        .and_then(|current| current.get_account_id())
                        .or_else(|| {
                            auth_accounts::get_active_account_id(client.code_home())
                                .ok()
                                .flatten()
                        });
                    if let Some(current_account_id) = current_account_id {
                        let now = Utc::now();
                        let blocked_until = limit
                            .resets_in_seconds
                            .map(|seconds| now + chrono::Duration::seconds(seconds as i64));
                        let current_auth_mode = auth
                            .as_ref()
                            .map(|current| current.mode)
                            .or_else(|| {
                                auth_accounts::find_account(
                                    client.code_home(),
                                    current_account_id.as_str(),
                                )
                                .ok()
                                .flatten()
                                .map(|account| account.mode)
                            });
                        if let Some(current_auth_mode) = current_auth_mode {
                            match switch_active_account_on_rate_limit(
                                client.code_home(),
                                state,
                                client.api_key_fallback_on_all_accounts_limited(),
                                now,
                                current_account_id.as_str(),
                                current_auth_mode,
                                blocked_until,
                            ) {
                                Ok(Some(next_account_id)) => {
                                    let next_label = auth_accounts::find_account(
                                        client.code_home(),
                                        &next_account_id,
                                    )
                                    .ok()
                                    .flatten()
                                    .and_then(|account| account.label)
                                    .unwrap_or_else(|| next_account_id.clone());
                                    tracing::info!(
                                        from_account_id = %current_account_id,
                                        to_account_id = %next_account_id,
                                        reason = "usage_limit_reached",
                                        "rate limit hit; auto-switching active account"
                                    );
                                    auth_manager.reload();
                                    event_tx.send(AutoCoordinatorEvent::Action {
                                        message: format!(
                                            "Auto-switch: now using {next_label} due to usage limit."
                                        ),
                                    });
                                    return RetryDecision::RateLimited {
                                        wait_until: Instant::now(),
                                        reason: "usage limit reached; switched accounts".to_string(),
                                    };
                                }
                                Ok(None) => {}
                                Err(err) => {
                                    warn!(
                                        from_account_id = %current_account_id,
                                        error = %err,
                                        "failed to activate account after usage limit"
                                    );
                                }
                            }
                        } else {
                            warn!(
                                from_account_id = %current_account_id,
                                "skipping account switch after usage limit: missing auth mode"
                            );
                        }
                    }
                }
            }
        }
    }

    classify_model_error(error)
}

fn classify_reqwest_error(err: &reqwest::Error) -> RetryDecision {
    if err.is_timeout() || err.is_connect() || err.is_request() && err.status().is_none() {
        return RetryDecision::RetryAfterBackoff {
            reason: format!("network error: {err}"),
        };
    }

    if let Some(status) = err.status() {
        if status == StatusCode::TOO_MANY_REQUESTS {
            return RetryDecision::RetryAfterBackoff {
                reason: "rate limited (429)".to_string(),
            };
        }
        if status == StatusCode::REQUEST_TIMEOUT || status.as_u16() == 408 {
            return RetryDecision::RetryAfterBackoff {
                reason: format!("provider returned {status}"),
            };
        }
        if status.as_u16() == 499 {
            return RetryDecision::RetryAfterBackoff {
                reason: "client closed request (499)".to_string(),
            };
        }
        if status.is_server_error() {
            return RetryDecision::RetryAfterBackoff {
                reason: format!("server error {status}"),
            };
        }
        if status.is_client_error() {
            return RetryDecision::Fatal(anyhow!(err.to_string()));
        }
    }

    RetryDecision::Fatal(anyhow!(err.to_string()))
}

fn parse_rate_limit_hint(body: &str) -> Option<Instant> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let error_obj = value.get("error").unwrap_or(&value);

    if let Some(seconds) = extract_seconds(error_obj) {
        return Some(compute_rate_limit_wait(seconds));
    }

    if let Some(reset_at) = extract_reset_at(error_obj) {
        return Some(reset_at);
    }

    None
}

fn extract_seconds(value: &serde_json::Value) -> Option<Duration> {
    let fields = [
        "reset_seconds",
        "reset_in_seconds",
        "resets_in_seconds",
        "x-ratelimit-reset",
        "x-ratelimit-reset-requests",
    ];
    for key in fields {
        if let Some(seconds) = value.get(key) {
            if let Some(num) = seconds.as_f64() {
                if num.is_sign_negative() {
                    continue;
                }
                return Some(Duration::from_secs_f64(num));
            }
            if let Some(text) = seconds.as_str() {
                if let Ok(num) = text.parse::<f64>() {
                    if num.is_sign_negative() {
                        continue;
                    }
                    return Some(Duration::from_secs_f64(num));
                }
            }
        }
    }
    None
}

fn extract_reset_at(value: &serde_json::Value) -> Option<Instant> {
    let reset_at = value.get("reset_at").and_then(|v| v.as_str())?;
    let parsed = DateTime::parse_from_rfc3339(reset_at)
        .or_else(|_| DateTime::parse_from_str(reset_at, "%+"))
        .ok()?;
    let reset_utc = parsed.with_timezone(&Utc);
    let now = Utc::now();
    let duration = reset_utc.signed_duration_since(now).to_std().unwrap_or_default();
    Some(compute_rate_limit_wait(duration))
}

fn compute_rate_limit_wait(base: Duration) -> Instant {
    let mut wait = if base > Duration::ZERO { base } else { Duration::ZERO };
    wait += RATE_LIMIT_BUFFER;
    wait += random_jitter(RATE_LIMIT_JITTER_MAX);
    Instant::now() + wait
}

fn random_jitter(max: Duration) -> Duration {
    if max.is_zero() {
        return Duration::ZERO;
    }
    let mut rng = rand::rng();
    let jitter = rng.random_range(0.0..max.as_secs_f64());
    Duration::from_secs_f64(jitter)
}

fn find_in_chain<'a, T: std::error::Error + 'static>(error: &'a anyhow::Error) -> Option<&'a T> {
    for cause in error.chain() {
        if let Some(specific) = cause.downcast_ref::<T>() {
            return Some(specific);
        }
    }
    None
}


struct RecoverableDecisionError {
    summary: String,
    #[cfg_attr(not(test), allow(dead_code))]
    guidance: Option<String>,
}

fn classify_recoverable_decision_error(err: &anyhow::Error) -> Option<RecoverableDecisionError> {
    let text = err.to_string();
    let lower = text.to_ascii_lowercase();

    if lower.contains("missing prompt_sent_to_cli")
        || lower.contains("missing cli prompt for continue")
        || lower.contains("missing cli prompt for `finish_status")
        || lower.contains("missing cli prompt")
    {
        return Some(RecoverableDecisionError {
            summary: "missing `prompt_sent_to_cli` for `finish_status: \"continue\"`".to_string(),
            guidance: Some(
                "Include a non-empty `prompt_sent_to_cli` string whenever `finish_status` is `\"continue\"`."
                    .to_string(),
            ),
        });
    }

    if lower.contains("length limit")
        || lower.contains("cut off")
        || lower.contains("exceeds") && lower.contains("prompt_sent_to_cli")
    {
        return Some(RecoverableDecisionError {
            summary: "model output was cut off by a length cap".to_string(),
            guidance: Some(
                "Regenerate with a shorter `prompt_sent_to_cli` (<=600 chars) and more concise status text so the response fits within provider limits."
                    .to_string(),
            ),
        });
    }

    if lower.contains("legacy model response missing cli_prompt for continue") {
        return Some(RecoverableDecisionError {
            summary: "legacy response omitted `cli_prompt` for continue turn".to_string(),
            guidance: Some(
                "Legacy coordinator responses must populate `cli_prompt` when the turn continues."
                    .to_string(),
            ),
        });
    }

    if lower.contains(" is empty") {
        if let Some((field, _)) = text.split_once(" is empty") {
            let field_trimmed = field.trim().trim_matches('`');
            if !field_trimmed.is_empty() {
                let summary = format!("`{field_trimmed}` was empty");
                let guidance = format!(
                    "Provide a meaningful value for `{field_trimmed}` instead of leaving it blank."
                );
                return Some(RecoverableDecisionError {
                    summary,
                    guidance: Some(guidance),
                });
            }
        }
    }

    if lower.contains("unexpected finish_status") {
        let extracted = text
            .split('\'')
            .nth(1)
            .filter(|value| !value.is_empty())
            .map(|value| format!("unexpected finish_status '{value}'"))
            .unwrap_or_else(|| "unexpected finish_status".to_string());
        return Some(RecoverableDecisionError {
            summary: extracted,
            guidance: Some(
                "Use `finish_status` values: `continue`, `finish_success`, or `finish_failed`."
                    .to_string(),
            ),
        });
    }

    if lower.contains("model response was not valid json") || lower.contains("parsing json from model output") {
        return Some(RecoverableDecisionError {
            summary: "response was not valid JSON".to_string(),
            guidance: Some(
                "Return strictly valid JSON that matches the `auto_coordinator_flow` schema without extra prose."
                    .to_string(),
            ),
        });
    }

    if lower.contains("decoding coordinator decision failed") {
        return Some(RecoverableDecisionError {
            summary: "response did not match the coordinator schema".to_string(),
            guidance: Some(
                "Ensure every required field is present and spelled correctly per the coordinator schema."
                    .to_string(),
            ),
        });
    }

    None
}

#[cfg(test)]
fn push_unique_guidance(guidance: &mut Vec<String>, message: &str) {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return;
    }
    if guidance
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(trimmed))
    {
        return;
    }
    guidance.push(trimmed.to_string());
}


fn parse_decision(raw: &str) -> Result<(ParsedCoordinatorDecision, Value)> {
    let value: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => {
            let Some(json_blob) = extract_first_json_object(raw) else {
                return Err(anyhow!("model response was not valid JSON"));
            };
            serde_json::from_str(&json_blob).context("parsing JSON from model output")?
        }
    };
    match serde_json::from_value::<CoordinatorDecisionNew>(value.clone()) {
        Ok(decision) => {
            let status = parse_finish_status(&decision.finish_status)?;
            let parsed = convert_decision_new(decision, status)?;
            Ok((parsed, value))
        }
        Err(new_err) => {
            let decision: CoordinatorDecisionLegacy = serde_json::from_value(value.clone()).map_err(|legacy_err| {
                let payload = serde_json::to_string(&value).unwrap_or_else(|_| "<unprintable json>".to_string());
                let snippet = if payload.len() > 2000 {
                    format!("{}…", &payload[..2000])
                } else {
                    payload
                };
                anyhow!("decoding coordinator decision failed: new_schema_err={new_err}; legacy_err={legacy_err}; payload_snippet={snippet}")
            })?;
            let status = parse_finish_status(&decision.finish_status)?;
            let parsed = convert_decision_legacy(decision, status)?;
            Ok((parsed, value))
        }
    }
}

fn parse_finish_status(finish_status: &str) -> Result<AutoCoordinatorStatus> {
    let normalized = finish_status.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "continue" => Ok(AutoCoordinatorStatus::Continue),
        "finish_success" => Ok(AutoCoordinatorStatus::Success),
        "finish_failed" => Ok(AutoCoordinatorStatus::Failed),
        other => Err(anyhow!("unexpected finish_status '{other}'")),
    }
}

fn convert_decision_new(
    decision: CoordinatorDecisionNew,
    status: AutoCoordinatorStatus,
) -> Result<ParsedCoordinatorDecision> {
    let CoordinatorDecisionNew {
        finish_status: _,
        status_title,
        status_sent_to_user,
        progress,
        prompt_sent_to_cli,
        agents: agent_payloads,
        goal,
    } = decision;

    let mut status_title = clean_optional(status_title);
    let mut status_sent_to_user = clean_optional(status_sent_to_user);

    if let Some(progress) = progress {
        let legacy_past = clean_optional(progress.past);
        let legacy_current = clean_optional(progress.current);
        if status_title.is_none() {
            status_title = legacy_current.clone();
        }
        if status_sent_to_user.is_none() {
            status_sent_to_user = legacy_past.clone();
        }
    }

    let goal = clean_optional(goal);

    let cli_prompt = clean_optional(prompt_sent_to_cli);

    let cli = match (status, cli_prompt) {
        (AutoCoordinatorStatus::Continue, Some(prompt)) => {
            let prompt = clean_required(&prompt, "prompt_sent_to_cli")?;
            ensure_cli_prompt_length(&prompt)?;

            Some(CliAction {
                prompt,
                context: None,
                suppress_ui_context: false,
            })
        }
        (AutoCoordinatorStatus::Continue, None) => {
            return Err(anyhow!(
                "model response missing prompt_sent_to_cli for continue"
            ));
        }
        (_, Some(_prompt)) => None,
        (_, None) => None,
    };

    let mut agent_actions: Vec<AgentAction> = Vec::new();
    let mut agents_timing: Option<AutoTurnAgentsTiming> = None;
    if let Some(payloads) = agent_payloads {
        match payloads {
            AgentsField::List(list) => {
                for payload in list {
                    let AgentPayload { prompt, context, write, models } = payload;
                    let prompt = clean_required(&prompt, "agents[*].prompt")?;
                    agent_actions.push(AgentAction {
                        prompt,
                        context: clean_optional(context),
                        write,
                        models: clean_models(models),
                    });
                }
            }
            AgentsField::Object(plan) => {
                let AgentsPayload {
                    timing,
                    models,
                    requests,
                } = plan;
                if let Some(timing_value) = timing {
                    agents_timing = Some(timing_value.into());
                }
                let batch_models = clean_models(models);
                for payload in requests {
                    let AgentPayload { prompt, context, write, models } = payload;
                    let prompt = clean_required(&prompt, "agents.requests[*].prompt")?;
                    let models = clean_models(models).or_else(|| batch_models.clone());
                    agent_actions.push(AgentAction {
                        prompt,
                        context: clean_optional(context),
                        write,
                        models,
                    });
                }
            }
        }
    }

    Ok(ParsedCoordinatorDecision {
        status,
        status_title,
        status_sent_to_user,
        cli,
        agents_timing,
        agents: agent_actions,
        goal,
        response_items: Vec::new(),
        token_usage: None,
        model_slug: MODEL_SLUG.to_string(),
    })
}

fn convert_decision_legacy(
    decision: CoordinatorDecisionLegacy,
    status: AutoCoordinatorStatus,
) -> Result<ParsedCoordinatorDecision> {
    let CoordinatorDecisionLegacy {
        finish_status: _,
        progress_past,
        progress_current,
        cli_context,
        cli_prompt,
        goal,
    } = decision;

    let status_title = clean_optional(progress_current);
    let status_sent_to_user = clean_optional(progress_past);
    let context = clean_optional(cli_context);
    let goal = clean_optional(goal);

    let cli = match (status, cli_prompt) {
        (AutoCoordinatorStatus::Continue, Some(prompt)) => Some(CliAction {
            prompt: clean_required(&prompt, "cli_prompt")?,
            context: context.clone(),
            suppress_ui_context: false,
        }),
        (AutoCoordinatorStatus::Continue, None) => {
            return Err(anyhow!("legacy model response missing cli_prompt for continue"));
        }
        (_, Some(prompt)) => Some(CliAction {
            prompt: clean_required(&prompt, "cli_prompt")?,
            context: context.clone(),
            suppress_ui_context: false,
        }),
        (_, None) => None,
    };

    Ok(ParsedCoordinatorDecision {
        status,
        status_title,
        status_sent_to_user,
        cli,
        agents_timing: None,
        agents: Vec::new(),
        goal,
        response_items: Vec::new(),
        token_usage: None,
        model_slug: MODEL_SLUG.to_string(),
    })
}

fn clean_optional(input: Option<String>) -> Option<String> {
    input.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            let without_prefix = strip_role_prefix(trimmed);
            let final_trimmed = without_prefix.trim();
            if final_trimmed.is_empty() {
                None
            } else {
                Some(final_trimmed.to_string())
            }
        }
    })
}

fn clean_models(models: Option<Vec<String>>) -> Option<Vec<String>> {
    let mut cleaned: Vec<String> = models?
        .into_iter()
        .filter_map(|model| {
            let trimmed = model.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect();

    if cleaned.is_empty() {
        return None;
    }

    cleaned.sort();
    cleaned.dedup();
    Some(cleaned)
}

fn clean_required(value: &str, field: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(anyhow!("{field} is empty"))
    } else {
        let without_prefix = strip_role_prefix(trimmed);
        let final_trimmed = without_prefix.trim();
        if final_trimmed.is_empty() {
            Err(anyhow!("{field} is empty"))
        } else {
            Ok(final_trimmed.to_string())
        }
    }
}

fn ensure_cli_prompt_length(prompt: &str) -> Result<()> {
    let len = prompt.chars().count();
    if len < CLI_PROMPT_MIN_CHARS {
        return Err(anyhow!(
            "prompt_sent_to_cli must be at least {CLI_PROMPT_MIN_CHARS} characters; keep it concise but not empty"
        ));
    }
    if len > CLI_PROMPT_MAX_CHARS {
        return Err(anyhow!(
            "prompt_sent_to_cli exceeds {CLI_PROMPT_MAX_CHARS} characters; keep prompts succinct (<=600 chars) and let the CLI decide how to execute the task"
        ));
    }

    Ok(())
}

fn cli_action_to_event(action: &CliAction) -> AutoTurnCliAction {
    AutoTurnCliAction {
        prompt: action.prompt.clone(),
        context: action.context.clone(),
        suppress_ui_context: action.suppress_ui_context,
    }
}

fn agent_action_to_event(action: &AgentAction) -> AutoTurnAgentsAction {
    AutoTurnAgentsAction {
        prompt: action.prompt.clone(),
        context: action.context.clone(),
        write: action.write.unwrap_or(false),
        write_requested: action.write,
        models: action.models.clone(),
    }
}

fn agent_action_to_event_with_write_guard(
    action: &AgentAction,
    allow_write: bool,
) -> AutoTurnAgentsAction {
    let mut event = agent_action_to_event(action);
    if !allow_write && event.write {
        event.write = false;
    }
    event
}

pub(crate) fn extract_first_json_object(input: &str) -> Option<String> {
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escape = false;
    let mut start: Option<usize> = None;
    for (idx, ch) in input.char_indices() {
        if in_str {
            if escape {
                escape = false;
                continue;
            }
            match ch {
                '"' => in_str = false,
                '\\' => escape = true,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => {
                if depth == 0 {
                    start = Some(idx);
                }
                depth += 1;
            }
            '}' => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    let Some(s) = start else { return None; };
                    return Some(input[s..=idx].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn make_message(role: &str, text: String) -> ResponseItem {
    let content = if role.eq_ignore_ascii_case("assistant") {
        ContentItem::OutputText { text }
    } else {
        ContentItem::InputText { text }
    };

    ResponseItem::Message {
        id: None,
        role: role.to_string(),
        content: vec![content],
    }
}

fn strip_role_prefix(input: &str) -> &str {
    const PREFIXES: [&str; 2] = ["Coordinator:", "CLI:"];
    for prefix in PREFIXES {
        if let Some(head) = input.get(..prefix.len()) {
            if head.eq_ignore_ascii_case(prefix) {
                let rest = input
                    .get(prefix.len()..)
                    .unwrap_or_default();
                return rest.strip_prefix(' ').unwrap_or(rest);
            }
        }
    }
    input
}

fn emit_auto_drive_metrics(event_tx: &AutoCoordinatorEventSender, metrics: &SessionMetrics) {
    if metrics.turn_count() == 0 && metrics.running_total().is_zero() {
        return;
    }

    let event = AutoCoordinatorEvent::TokenMetrics {
        total_usage: metrics.running_total().clone(),
        last_turn_usage: metrics.last_turn().clone(),
        turn_count: metrics.turn_count(),
        duplicate_items: metrics.duplicate_items(),
        replay_updates: metrics.replay_updates(),
    };
    event_tx.send(event);
}

enum CompactionResult {
    Skipped,
    Completed { summary_text: Option<String> },
}

fn maybe_compact(
    runtime: &tokio::runtime::Runtime,
    client: &ModelClient,
    event_tx: &AutoCoordinatorEventSender,
    conversation: &mut Vec<ResponseItem>,
    metrics: &SessionMetrics,
    prev_summary: Option<&str>,
    model_slug: &str,
    compact_prompt: &str,
) -> CompactionResult {
    let transcript_tokens: u64 = conversation
        .iter()
        .map(|item| estimate_item_tokens(item) as u64)
        .sum();
    let estimated_next = metrics.estimated_next_prompt_tokens();
    let message_count = conversation.len();
    let has_recorded_usage = metrics.has_recorded_usage();

    if !should_compact(
        model_slug,
        transcript_tokens,
        estimated_next,
        message_count,
        has_recorded_usage,
    ) {
        return CompactionResult::Skipped;
    }

    let Some(bounds) = compute_slice_bounds(conversation) else {
        return CompactionResult::Skipped;
    };

    event_tx.send(AutoCoordinatorEvent::Thinking {
        delta: "Compacting history to stay within the context window…".to_string(),
        summary_index: None,
    });

    let original_len = conversation.len();
    match compact_with_endpoint(runtime, client, conversation, model_slug, compact_prompt) {
        Ok(compacted) => {
            let removed = original_len.saturating_sub(compacted.len());
            let plural = if removed == 1 { "" } else { "s" };
            *conversation = compacted;
            event_tx.send(AutoCoordinatorEvent::Thinking {
                delta: format!(
                    "Finished compacting history ({removed} message{plural} -> {} total).",
                    conversation.len()
                ),
                summary_index: None,
            });
            debug!(
                "[Auto coordinator] remote compacted {removed} messages; new conversation length {}",
                conversation.len()
            );
            return CompactionResult::Completed {
                summary_text: None,
            };
        }
        Err(err) => {
            warn!("[Auto coordinator] remote compaction failed: {err:#}");
            event_tx.send(AutoCoordinatorEvent::Thinking {
                delta: "Remote compaction failed; falling back to local summary.".to_string(),
                summary_index: None,
            });
        }
    }

    let slice: Vec<ResponseItem> = conversation[bounds.0..bounds.1].to_vec();

    let (checkpoint, summary_warning) = build_checkpoint_summary(
        runtime,
        client,
        model_slug,
        &slice,
        prev_summary,
        compact_prompt,
    );

    if let Some(warning_text) = summary_warning {
        warn!(
            "[Auto coordinator] checkpoint summary fell back to deterministic mode: {warning_text}"
        );
        event_tx.send(AutoCoordinatorEvent::Thinking {
            delta: format!(
                "History compaction warning: {warning_text}. Falling back to a deterministic summary."
            ),
            summary_index: None,
        });
    }

    if apply_compaction(conversation, bounds, prev_summary, checkpoint.message).is_none() {
        warn!("[Auto coordinator] apply_compaction returned None; bounds={bounds:?}");
        event_tx.send(AutoCoordinatorEvent::Thinking {
            delta: "Failed to compact history because the conversation changed while applying the summary. Continuing without compaction.".to_string(),
            summary_index: None,
        });
        return CompactionResult::Skipped;
    }

    let removed = slice.len();
    let total = conversation.len();
    let plural = if removed == 1 { "" } else { "s" };
    event_tx.send(AutoCoordinatorEvent::Thinking {
        delta: format!(
            "Finished compacting history ({removed} message{plural} -> {total} total)."
        ),
        summary_index: None,
    });

    debug!(
        "[Auto coordinator] compacted {} messages; new conversation length {}",
        slice.len(),
        conversation.len()
    );
    CompactionResult::Completed {
        summary_text: Some(checkpoint.text),
    }
}

/// Determine if compaction should occur based on token usage.
///
/// Uses 80% of the model's max_context as the threshold.
/// Returns true if `session_total + estimated_next >= 0.8 * model_context_window`.
///
/// # Arguments
/// * `model_slug` - The model identifier to look up context limits
/// * `session_total` - Total tokens used in the session so far
/// * `estimated_next` - Estimated tokens for the next turn
/// * `message_count` - Number of messages in the current conversation (fallback heuristic)
/// * `has_recorded_usage` - Whether we have real token usage from the backend
pub fn should_compact(
    model_slug: &str,
    transcript_tokens: u64,
    estimated_next: u64,
    message_count: usize,
    has_recorded_usage: bool,
) -> bool {
    // Get model family to look up model info
    let family = find_family_for_model(model_slug)
        .unwrap_or_else(|| derive_default_model_family(model_slug));

    let token_limit = family
        .auto_compact_token_limit()
        .and_then(|limit| (limit > 0).then(|| limit as u64))
        .or(family.context_window);

    if let Some(token_limit) = token_limit {
        if token_limit > 0 {
            let threshold = (token_limit as f64 * 0.8) as u64;
            let projected_total = transcript_tokens.saturating_add(estimated_next);
            if projected_total >= threshold {
                return true;
            }

            // When we have an explicit token budget for the model, rely on it and
            // skip the fallback message-count heuristic. This avoids runaway
            // compaction loops when restarting Auto Drive with a large but still
            // token-safe transcript.
            return false;
        }
    }

    if has_recorded_usage {
        return false;
    }

    fallback_message_limit(message_count)
}

fn fallback_message_limit(message_count: usize) -> bool {
    message_count >= MESSAGE_LIMIT_FALLBACK
}
