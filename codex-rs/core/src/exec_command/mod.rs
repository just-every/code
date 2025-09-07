mod exec_command_params;
mod exec_command_session;
mod responses_api;
mod session_id;
mod session_manager;

#[allow(unused_imports)]
pub use exec_command_params::ExecCommandParams;
#[allow(unused_imports)]
pub use exec_command_params::WriteStdinParams;
#[allow(unused_imports)]
pub use responses_api::EXEC_COMMAND_TOOL_NAME;
#[allow(unused_imports)]
pub use responses_api::WRITE_STDIN_TOOL_NAME;
#[allow(unused_imports)]
pub use responses_api::create_exec_command_tool_for_responses_api;
#[allow(unused_imports)]
pub use responses_api::create_write_stdin_tool_for_responses_api;
#[allow(unused_imports)]
pub use session_manager::result_into_payload;

// Provide a stable type alias used by the rest of the codebase.
// Upstream removed the global SESSION_MANAGER; we now manage a per-session
// instance. Keep the ExecSessionManager name for minimal churn.
pub type ExecSessionManager = session_manager::SessionManager;
