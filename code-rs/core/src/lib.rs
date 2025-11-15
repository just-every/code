//! Root of the `codex-core` library.

// Prevent accidental direct writes to stdout/stderr in library code. All
// user-visible output must go through the appropriate abstraction (e.g.,
// the TUI or the tracing stack).
#![deny(clippy::print_stdout, clippy::print_stderr)]

mod apply_patch;
pub mod auth;
pub mod auth_accounts;
pub mod account_usage;
pub mod bash;
mod chat_completions;
mod client;
mod client_common;
pub mod codex;
mod code_conversation;
pub mod token_data;
pub use code_conversation::CodexConversation;
mod command_safety;
pub mod config;
pub mod config_edit;
pub mod config_profile;
pub mod config_types;
mod config_loader;
mod conversation_history;
pub mod context_timeline;
pub mod acp;
pub mod custom_prompts;
pub mod debug_logger;
mod environment_context;
pub mod retention;
pub mod telemetry;
pub use environment_context::BrowserSnapshot;
pub use environment_context::EnvironmentContextDelta;
pub use environment_context::EnvironmentContextEmission;
pub use environment_context::EnvironmentContextSnapshot;
pub use environment_context::EnvironmentContextTracker;
pub use environment_context::OperatingSystemInfo;
pub use environment_context::ViewportDimensions;
pub mod error;
pub mod exec;
mod exec_command;
pub mod exec_env;
mod flags;
pub mod git_info;
pub mod landlock;
pub mod http_client;
pub mod housekeeping;
pub mod mcp_connection_manager;
mod mcp_tool_call;
mod message_history;
mod model_provider_info;
pub mod agent_defaults;
mod agent_tool;
mod dry_run_guard;
mod image_comparison;
pub mod git_worktree;
pub mod slash_commands;
pub mod parse_command;
pub mod history;
mod truncate;
mod unified_exec;
mod user_instructions;
pub use model_provider_info::BUILT_IN_OSS_MODEL_PROVIDER_ID;
pub use model_provider_info::ModelProviderInfo;
pub use model_provider_info::OpenRouterConfig;
pub use model_provider_info::OpenRouterProviderConfig;
pub use model_provider_info::WireApi;
pub use model_provider_info::built_in_model_providers;
pub use model_provider_info::create_oss_provider_with_base_url;
mod conversation_manager;
pub mod protocol;
mod event_mapping;
pub mod review_format;
#[cfg(test)]
mod prompt_assembly_tests;
pub use code_protocol::protocol::InitialHistory;
pub use conversation_manager::ConversationManager;
pub use conversation_manager::NewConversation;
// Re-export common auth types for workspace consumers
pub use auth::AuthManager;
pub use auth::CodexAuth;
pub use auth::RefreshTokenError;
pub use auth::RefreshTokenErrorKind;
pub mod default_client;
pub mod model_family;
pub mod openai_model_info;
mod openai_tools;
mod patch_harness;
pub mod plan_tool;
pub mod project_doc;
pub mod project_features;
mod rollout;
pub(crate) mod safety;
pub mod seatbelt;
pub mod shell;
pub mod spawn;
pub mod terminal;
pub mod otel_init;
mod tool_apply_patch;
mod workflow_validation;
pub mod turn_diff_tracker;
pub use rollout::ARCHIVED_SESSIONS_SUBDIR;
pub use rollout::INTERACTIVE_SESSION_SOURCES;
pub use rollout::RolloutRecorder;
pub use rollout::SESSIONS_SUBDIR;
pub use rollout::SessionMeta;
pub use rollout::find_conversation_path_by_id_str;
pub use rollout::list::ConversationItem;
pub use rollout::list::ConversationsPage;
pub use rollout::list::Cursor;
mod function_tool;
mod user_notification;
pub mod util;

pub use apply_patch::CODEX_APPLY_PATCH_ARG1;
pub use command_safety::is_safe_command;
pub use agent_tool::smoke_test_agent_blocking;
pub use agent_tool::split_command_and_args;
pub use safety::get_platform_sandbox;
pub use housekeeping::run_housekeeping_if_due;
pub use housekeeping::CleanupOutcome;
// Use our internal protocol module for crate-internal types and helpers.
// External callers should rely on specific re-exports below.
// Re-export protocol config enums to ensure call sites can use the same types
// as those in the protocol crate when constructing protocol messages.
pub use code_protocol::config_types as protocol_config_types;
// Preserve `code_core::models::...` imports as an alias to the protocol models.
pub use code_protocol::models as models;

pub use client::ModelClient;
pub use client_common::Prompt;
pub use client_common::TextFormat;
pub use client_common::REVIEW_PROMPT;
pub use client_common::ResponseEvent;
pub use client_common::ResponseStream;
pub use codex::Codex;
pub use codex::CodexSpawnOk;
pub use codex::compact::content_items_to_text;
pub use codex::compact::is_session_prefix_message;
pub use code_protocol::models::ContentItem;
pub use code_protocol::models::LocalShellAction;
pub use code_protocol::models::LocalShellExecAction;
pub use code_protocol::models::LocalShellStatus;
pub use code_protocol::models::ReasoningItemContent;
pub use code_protocol::models::ResponseItem;
pub use environment_context::ToolCandidate;
pub use environment_context::TOOL_CANDIDATES;
pub use openai_tools::{get_openai_tools, OpenAiTool, ToolsConfig};
pub use otel_init::*;
