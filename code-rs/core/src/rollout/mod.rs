//! Rollout module: persistence and discovery of session rollout files.

use code_protocol::protocol::SessionSource;

pub const SESSIONS_SUBDIR: &str = "sessions";
#[allow(dead_code)]
pub const ARCHIVED_SESSIONS_SUBDIR: &str = "archived_sessions";
pub const INTERACTIVE_SESSION_SOURCES: &[SessionSource] =
    &[SessionSource::Cli, SessionSource::VSCode];

pub mod catalog;
pub mod list;
pub(crate) mod policy;
pub mod recorder;

pub use code_protocol::protocol::SessionMeta;
#[allow(unused_imports)]
pub use list::find_conversation_path_by_id_str;
pub use recorder::RolloutRecorder;
#[allow(unused_imports)]
pub use recorder::RolloutRecorderParams;

#[cfg(test)]
pub mod tests;
