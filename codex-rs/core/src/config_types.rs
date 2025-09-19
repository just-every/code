//! Types used to define the fields of [`crate::config::Config`].

// Note this file should generally be restricted to simple struct/enum
// definitions that do not contain business logic.

use std::collections::HashMap;
use std::path::PathBuf;
use wildmatch::WildMatchPattern;

use serde::Deserialize;
use serde::Serialize;
use strum_macros::Display;

/// Configuration for commands that require an explicit `confirm:` prefix.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct ConfirmGuardConfig {
    /// List of regex patterns applied to the raw command (joined argv or shell script).
    #[serde(default)]
    pub patterns: Vec<ConfirmGuardPattern>,
}

impl Default for ConfirmGuardConfig {
    fn default() -> Self {
        Self { patterns: default_confirm_guard_patterns() }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct ConfirmGuardPattern {
    /// ECMA-style regular expression matched against the command string.
    pub regex: String,
    /// Optional custom guidance text surfaced when the guard triggers.
    #[serde(default)]
    pub message: Option<String>,
}

fn default_confirm_guard_patterns() -> Vec<ConfirmGuardPattern> {
    vec![
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+reset\b".to_string(),
            message: Some("Blocked git reset. Reset rewrites the working tree/index and may delete local work. Resend with 'confirm:' if you're certain.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+checkout\s+--\b".to_string(),
            message: Some("Blocked git checkout -- <paths>. This overwrites local modifications; resend with 'confirm:' to proceed.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+checkout\s+(?:-b|-B|--orphan|--detach)\b".to_string(),
            message: Some("Blocked git checkout with branch-changing flag. Switching branches can discard or hide in-progress changes.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+checkout\s+-\b".to_string(),
            message: Some("Blocked git checkout -. Confirm before switching back to the previous branch.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+switch\b.*(?:-c|--detach)".to_string(),
            message: Some("Blocked git switch creating or detaching a branch. Resend with 'confirm:' if requested.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+switch\s+[^\s-][^\s]*".to_string(),
            message: Some("Blocked git switch <branch>. Branch changes can discard or hide work; confirm before continuing.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+clean\b.*(?:-f|--force|-x|-X|-d)".to_string(),
            message: Some("Blocked git clean with destructive flags. This deletes untracked files or build artifacts.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+push\b.*(?:--force|-f)".to_string(),
            message: Some("Blocked git push --force. Force pushes rewrite remote history; only continue if explicitly requested.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?rm\s+-[a-z-]*rf[a-z-]*\s+(?:--\s+)?(?:\.|\.\.|\./|/|\*)(?:\s|$)".to_string(),
            message: Some("Blocked rm -rf targeting a broad path (., .., /, or *). Confirm before destructive delete.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?rm\s+-[a-z-]*r[a-z-]*\s+-[a-z-]*f[a-z-]*\s+(?:--\s+)?(?:\.|\.\.|\./|/|\*)(?:\s|$)".to_string(),
            message: Some("Blocked rm -r/-f combination targeting broad paths. Resend with 'confirm:' if you intend to wipe this tree.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?rm\s+-[a-z-]*f[a-z-]*\s+-[a-z-]*r[a-z-]*\s+(?:--\s+)?(?:\.|\.\.|\./|/|\*)(?:\s|$)".to_string(),
            message: Some("Blocked rm -f/-r combination targeting broad paths. Confirm before running.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?find\s+\.(?:\s|$).*\s-delete\b".to_string(),
            message: Some("Blocked find . ... -delete. Recursive deletes require confirmation.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?find\s+\.(?:\s|$).*\s-exec\s+rm\b".to_string(),
            message: Some("Blocked find . ... -exec rm. Confirm before running recursive rm.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?trash\s+-[a-z-]*r[a-z-]*f[a-z-]*\b".to_string(),
            message: Some("Blocked trash -rf. Bulk trash operations can delete large portions of the workspace.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?fd\b.*(?:--exec|-x)\s+rm\b".to_string(),
            message: Some("Blocked fd … --exec rm. Confirm before piping search results into rm.".to_string()),
        },
    ]
}

/// Configuration for a subagent slash command (e.g., plan/solve/code or custom)
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SubagentCommandConfig {
    /// Name of the command (e.g., "plan", "solve", "code", or custom)
    pub name: String,

    /// Whether agents launched for this command should run in read-only mode
    /// Defaults: plan/solve=true, code=false (applied if not specified here)
    #[serde(default)]
    pub read_only: bool,

    /// Agent names to enable for this command. If empty, falls back to
    /// enabled agents from `[[agents]]`, or built-in defaults.
    #[serde(default)]
    pub agents: Vec<String>,

    /// Extra instructions to append to the orchestrator (Code) prompt.
    #[serde(default)]
    pub orchestrator_instructions: Option<String>,

    /// Extra instructions that the orchestrator should append to the prompt
    /// given to each launched agent.
    #[serde(default)]
    pub agent_instructions: Option<String>,
}

/// Top-level subagents section containing a list of commands.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SubagentsToml {
    #[serde(default)]
    pub commands: Vec<SubagentCommandConfig>,
}

/// Configuration for external agent models
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct AgentConfig {
    /// Name of the agent (e.g., "claude", "gemini", "gpt-4")
    pub name: String,

    /// Command to execute the agent (e.g., "claude", "gemini").
    /// If omitted, defaults to the agent `name` during config load.
    #[serde(default)]
    pub command: String,

    /// Optional arguments to pass to the agent command
    #[serde(default)]
    pub args: Vec<String>,

    /// Whether this agent can only run in read-only mode
    #[serde(default)]
    pub read_only: bool,

    /// Whether this agent is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Optional description of the agent
    #[serde(default)]
    pub description: Option<String>,

    /// Optional environment variables for the agent
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,

    /// Optional arguments to pass only when the agent is executed in
    /// read-only mode. When present, these are preferred over `args` for
    /// read-only runs.
    #[serde(default)]
    pub args_read_only: Option<Vec<String>>,

    /// Optional arguments to pass only when the agent is executed with write
    /// permissions. When present, these are preferred over `args` for write
    /// runs.
    #[serde(default)]
    pub args_write: Option<Vec<String>>,

    /// Optional per-agent instructions. When set, these are prepended to the
    /// prompt provided to the agent whenever it runs.
    #[serde(default)]
    pub instructions: Option<String>,
}

fn default_true() -> bool {
    true
}

/// GitHub integration settings.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct GithubConfig {
    /// When true, Codex watches for GitHub Actions workflow runs after a
    /// successful `git push` and reports failures as background messages.
    /// Enabled by default; can be disabled via `~/.code/config.toml` under
    /// `[github]` with `check_workflows_on_push = false`.
    #[serde(default = "default_true")]
    pub check_workflows_on_push: bool,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct McpServerConfig {
    pub command: String,

    #[serde(default)]
    pub args: Vec<String>,

    #[serde(default)]
    pub env: Option<HashMap<String, String>>,

    /// Optional per-server startup timeout in milliseconds.
    /// Applies to both the initial `initialize` handshake and the first
    /// `tools/list` request during startup. If unset, defaults to 10_000ms.
    #[serde(default)]
    pub startup_timeout_ms: Option<u64>,
}

#[derive(Deserialize, Debug, Copy, Clone, PartialEq)]
pub enum UriBasedFileOpener {
    #[serde(rename = "vscode")]
    VsCode,

    #[serde(rename = "vscode-insiders")]
    VsCodeInsiders,

    #[serde(rename = "windsurf")]
    Windsurf,

    #[serde(rename = "cursor")]
    Cursor,

    /// Option to disable the URI-based file opener.
    #[serde(rename = "none")]
    None,
}

impl UriBasedFileOpener {
    pub fn get_scheme(&self) -> Option<&str> {
        match self {
            UriBasedFileOpener::VsCode => Some("vscode"),
            UriBasedFileOpener::VsCodeInsiders => Some("vscode-insiders"),
            UriBasedFileOpener::Windsurf => Some("windsurf"),
            UriBasedFileOpener::Cursor => Some("cursor"),
            UriBasedFileOpener::None => None,
        }
    }
}

/// Settings that govern if and what will be written to `~/.code/history.jsonl`
/// (Code still reads legacy `~/.codex/history.jsonl`).
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct History {
    /// If true, history entries will not be written to disk.
    pub persistence: HistoryPersistence,

    /// If set, the maximum size of the history file in bytes.
    /// TODO(mbolin): Not currently honored.
    pub max_bytes: Option<usize>,
}

#[derive(Deserialize, Debug, Copy, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum HistoryPersistence {
    /// Save all history entries to disk.
    #[default]
    SaveAll,
    /// Do not write history to disk.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Notifications {
    Enabled(bool),
    Custom(Vec<String>),
}

impl Default for Notifications {
    fn default() -> Self {
        Self::Enabled(false)
    }
}

/// Collection of settings that are specific to the TUI.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Tui {
    /// Theme configuration for the TUI
    #[serde(default)]
    pub theme: ThemeConfig,

    /// Syntax highlighting configuration (Markdown fenced code blocks)
    #[serde(default)]
    pub highlight: HighlightConfig,

    /// Whether to show reasoning content expanded by default (can be toggled with Ctrl+R/T)
    #[serde(default)]
    pub show_reasoning: bool,

    /// Streaming/animation behavior for assistant/reasoning output
    #[serde(default)]
    pub stream: StreamConfig,

    /// Loading spinner style selection
    #[serde(default)]
    pub spinner: SpinnerSelection,

    /// Enable desktop notifications from the TUI when the terminal is unfocused.
    /// Defaults to `false`.
    #[serde(default)]
    pub notifications: Notifications,

    /// Whether to use the terminal's Alternate Screen (full-screen) mode.
    /// When false, Codex renders nothing and leaves the standard terminal
    /// buffer visible; users can toggle back to Alternate Screen at runtime
    /// with Ctrl+T. Defaults to true.
    #[serde(default = "default_true")]
    pub alternate_screen: bool,

    /// Keyboard shortcut customization.
    #[serde(default)]
    pub shortcuts: Shortcuts,
}

// Important: Provide a manual Default so that when no config file exists and we
// construct `Config` via `unwrap_or_default()`, we still honor the intended
// default of `alternate_screen = true`. Deriving `Default` would set booleans to
// `false`, which caused fresh installs (or a temporary CODEX_HOME) to start in
// standard-terminal mode until the user pressed Ctrl+T.
impl Default for Tui {
    fn default() -> Self {
        Self {
            theme: ThemeConfig::default(),
            highlight: HighlightConfig::default(),
            show_reasoning: false,
            stream: StreamConfig::default(),
            spinner: SpinnerSelection::default(),
            notifications: Notifications::default(),
            alternate_screen: true,
            shortcuts: Shortcuts::default(),
        }
    }
}

/// A parsed key chord like "ctrl-g", "shift+enter", "f1", "pageup".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyChord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub code: KeyCodeName,
}

impl KeyChord {
    pub const fn new(ctrl: bool, alt: bool, shift: bool, code: KeyCodeName) -> Self {
        Self { ctrl, alt, shift, code }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value")]
pub enum KeyCodeName {
    Char(char),
    F(u8),
    Enter,
    Esc,
    Tab,
    BackTab,
    Insert,
    Space,
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

impl<'de> Deserialize<'de> for KeyCodeName {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let s = String::deserialize(d)?;
        let t = s.trim();
        let lower = t.to_ascii_lowercase();
        Ok(match lower.as_str() {
            "enter" => KeyCodeName::Enter,
            "esc" | "escape" => KeyCodeName::Esc,
            "tab" => KeyCodeName::Tab,
            "backtab" | "shift+tab" => KeyCodeName::BackTab,
            "insert" => KeyCodeName::Insert,
            "space" => KeyCodeName::Space,
            "left" => KeyCodeName::Left,
            "right" => KeyCodeName::Right,
            "up" => KeyCodeName::Up,
            "down" => KeyCodeName::Down,
            "pageup" | "page-up" => KeyCodeName::PageUp,
            "pagedown" | "page-down" => KeyCodeName::PageDown,
            "home" => KeyCodeName::Home,
            "end" => KeyCodeName::End,
            _ if lower.starts_with('f') => {
                let n = lower[1..].parse::<u8>().map_err(|_| D::Error::custom("invalid F-key"))?;
                KeyCodeName::F(n)
            }
            _ => {
                let mut chs = t.chars();
                if let (Some(c), None) = (chs.next(), chs.next()) {
                    KeyCodeName::Char(c.to_ascii_lowercase())
                } else {
                    return Err(D::Error::custom("invalid key code name"));
                }
            }
        })
    }
}

impl<'de> Deserialize<'de> for KeyChord {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let s = String::deserialize(d)?;
        let s_trim = s.trim();
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;

        fn parse_code<'de, E: serde::de::Error>(raw: &str) -> Result<KeyCodeName, E> {
            let t = raw.trim();
            let lower = t.to_ascii_lowercase();
            Ok(match lower.as_str() {
                "enter" => KeyCodeName::Enter,
                "esc" | "escape" => KeyCodeName::Esc,
                "tab" => KeyCodeName::Tab,
                "backtab" | "shift+tab" => KeyCodeName::BackTab,
                "insert" => KeyCodeName::Insert,
                "space" => KeyCodeName::Space,
                "left" => KeyCodeName::Left,
                "right" => KeyCodeName::Right,
                "up" => KeyCodeName::Up,
                "down" => KeyCodeName::Down,
                "pageup" | "page-up" => KeyCodeName::PageUp,
                "pagedown" | "page-down" => KeyCodeName::PageDown,
                "home" => KeyCodeName::Home,
                "end" => KeyCodeName::End,
                _ if lower.starts_with('f') => {
                    let n = lower[1..].parse::<u8>().map_err(|_| E::custom("invalid F-key"))?;
                    KeyCodeName::F(n)
                }
                _ => {
                    let mut chs = t.chars();
                    if let (Some(c), None) = (chs.next(), chs.next()) {
                        KeyCodeName::Char(c.to_ascii_lowercase())
                    } else {
                        return Err(E::custom("invalid key code name"));
                    }
                }
            })
        }

        if s_trim.contains('+') {
            let mut last: Option<&str> = None;
            for part in s_trim.split('+') {
                let p = part.trim();
                let lower = p.to_ascii_lowercase();
                match lower.as_str() {
                    "ctrl" | "control" => ctrl = true,
                    "alt" | "meta" => alt = true,
                    "shift" => shift = true,
                    _ => last = Some(p),
                }
            }
            let Some(code_str) = last else { return Err(D::Error::custom("missing key code")); };
            let code = parse_code::<D::Error>(code_str)?;
            return Ok(KeyChord { ctrl, alt, shift, code });
        }

        if let Ok(code) = parse_code::<D::Error>(s_trim) {
            return Ok(KeyChord { ctrl, alt, shift, code });
        }

        let mut last: Option<&str> = None;
        for part in s_trim.split('-') {
            let p = part.trim();
            let lower = p.to_ascii_lowercase();
            match lower.as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" | "meta" => alt = true,
                "shift" => shift = true,
                _ => last = Some(p),
            }
        }
        let Some(code_str) = last else { return Err(D::Error::custom("invalid key code name")); };
        let code = parse_code::<D::Error>(code_str)?;
        Ok(KeyChord { ctrl, alt, shift, code })
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Shortcuts {
    #[serde(default = "Shortcuts::default_help")] pub help: KeyChord,
    #[serde(default = "Shortcuts::default_toggle_browser_hud")] pub toggle_browser_hud: KeyChord,
    #[serde(default = "Shortcuts::default_toggle_agents_hud")] pub toggle_agents_hud: KeyChord,
    #[serde(default = "Shortcuts::default_transcript_page_up")] pub transcript_page_up: KeyChord,
    #[serde(default = "Shortcuts::default_transcript_page_down")] pub transcript_page_down: KeyChord,
    #[serde(default = "Shortcuts::default_transcript_to_top")] pub transcript_to_top: KeyChord,
    #[serde(default = "Shortcuts::default_transcript_to_bottom")] pub transcript_to_bottom: KeyChord,
    #[serde(default = "Shortcuts::default_composer_cycle_access_mode")] pub composer_cycle_access_mode: KeyChord,
    #[serde(default = "Shortcuts::default_composer_file_search")] pub composer_file_search: KeyChord,
    #[serde(default = "Shortcuts::default_composer_send")] pub composer_send: KeyChord,
    #[serde(default = "Shortcuts::default_composer_insert_newline")] pub composer_insert_newline: KeyChord,
    #[serde(default = "Shortcuts::default_composer_history_up")] pub composer_history_up: KeyChord,
    #[serde(default = "Shortcuts::default_composer_history_down")] pub composer_history_down: KeyChord,
    #[serde(default = "Shortcuts::default_app_exit_if_empty")] pub app_exit_if_empty: KeyChord,
    #[serde(default = "Shortcuts::default_app_toggle_reasoning")] pub app_toggle_reasoning: KeyChord,
    #[serde(default = "Shortcuts::default_app_toggle_screen")] pub app_toggle_screen: KeyChord,
    #[serde(default = "Shortcuts::default_app_toggle_diffs")] pub app_toggle_diffs: KeyChord,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            help: Self::default_help(),
            toggle_browser_hud: Self::default_toggle_browser_hud(),
            toggle_agents_hud: Self::default_toggle_agents_hud(),
            transcript_page_up: Self::default_transcript_page_up(),
            transcript_page_down: Self::default_transcript_page_down(),
            transcript_to_top: Self::default_transcript_to_top(),
            transcript_to_bottom: Self::default_transcript_to_bottom(),
            composer_cycle_access_mode: Self::default_composer_cycle_access_mode(),
            composer_file_search: Self::default_composer_file_search(),
            composer_send: Self::default_composer_send(),
            composer_insert_newline: Self::default_composer_insert_newline(),
            composer_history_up: Self::default_composer_history_up(),
            composer_history_down: Self::default_composer_history_down(),
            app_exit_if_empty: Self::default_app_exit_if_empty(),
            app_toggle_reasoning: Self::default_app_toggle_reasoning(),
            app_toggle_screen: Self::default_app_toggle_screen(),
            app_toggle_diffs: Self::default_app_toggle_diffs(),
        }
    }
}

impl Shortcuts {
    fn default_help() -> KeyChord { KeyChord::new(true, false, false, KeyCodeName::Char('h')) }
    fn default_toggle_browser_hud() -> KeyChord { KeyChord::new(true, false, false, KeyCodeName::Char('b')) }
    fn default_toggle_agents_hud() -> KeyChord { KeyChord::new(true, false, false, KeyCodeName::Char('a')) }
    fn default_transcript_page_up() -> KeyChord { KeyChord::new(false, false, false, KeyCodeName::PageUp) }
    fn default_transcript_page_down() -> KeyChord { KeyChord::new(false, false, false, KeyCodeName::PageDown) }
    fn default_transcript_to_top() -> KeyChord { KeyChord::new(false, false, false, KeyCodeName::Home) }
    fn default_transcript_to_bottom() -> KeyChord { KeyChord::new(false, false, false, KeyCodeName::End) }
    fn default_composer_cycle_access_mode() -> KeyChord { KeyChord::new(false, false, false, KeyCodeName::BackTab) }
    fn default_composer_file_search() -> KeyChord { KeyChord::new(false, false, false, KeyCodeName::Tab) }
    fn default_composer_send() -> KeyChord { KeyChord::new(false, false, false, KeyCodeName::Enter) }
    fn default_composer_insert_newline() -> KeyChord { KeyChord::new(false, false, true, KeyCodeName::Enter) }
    fn default_composer_history_up() -> KeyChord { KeyChord::new(false, false, true, KeyCodeName::Up) }
    fn default_composer_history_down() -> KeyChord { KeyChord::new(false, false, true, KeyCodeName::Down) }
    fn default_app_exit_if_empty() -> KeyChord { KeyChord::new(true, false, false, KeyCodeName::Char('d')) }
    fn default_app_toggle_reasoning() -> KeyChord { KeyChord::new(true, false, false, KeyCodeName::Char('r')) }
    fn default_app_toggle_screen() -> KeyChord { KeyChord::new(true, false, false, KeyCodeName::Char('t')) }
    fn default_app_toggle_diffs() -> KeyChord { KeyChord::new(true, false, false, KeyCodeName::Char('d')) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct W { k: KeyChord }

    fn parse(s: &str) -> KeyChord {
        toml::from_str::<W>(&format!("k = \"{}\"", s)).unwrap().k
    }

    #[test]
    fn keychord_parses_ctrl_g_variants() {
        assert_eq!(parse("ctrl-g"), KeyChord::new(true, false, false, KeyCodeName::Char('g')));
        assert_eq!(parse("ctrl+g"), KeyChord::new(true, false, false, KeyCodeName::Char('g')));
        assert_eq!(parse("CONTROL+G"), KeyChord::new(true, false, false, KeyCodeName::Char('g')));
    }

    #[test]
    fn keychord_parses_special_keys() {
        assert!(matches!(parse("enter").code, KeyCodeName::Enter));
        assert!(matches!(parse("page-down").code, KeyCodeName::PageDown));
        assert!(matches!(parse("PageUp").code, KeyCodeName::PageUp));
        assert!(matches!(parse("backtab").code, KeyCodeName::BackTab));
        assert!(matches!(parse("shift+tab").code, KeyCodeName::BackTab));
        match parse("f7").code { KeyCodeName::F(7) => {}, _ => panic!("expected F7") }
    }
}

/// Streaming behavior configuration for the TUI.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct StreamConfig {
    /// Emit the Answer header immediately when a stream begins (before first newline).
    #[serde(default)]
    pub answer_header_immediate: bool,

    /// Show an ellipsis placeholder in the Answer body while waiting for first text.
    #[serde(default = "default_true")]
    pub show_answer_ellipsis: bool,

    /// Commit animation pacing in milliseconds (lines per CommitTick).
    /// If unset, defaults to 50ms; in responsive profile, defaults to 30ms.
    #[serde(default)]
    pub commit_tick_ms: Option<u64>,

    /// Soft-commit timeout (ms) when no newline arrives; commits partial content.
    /// If unset, disabled; in responsive profile, defaults to 400ms.
    #[serde(default)]
    pub soft_commit_timeout_ms: Option<u64>,

    /// Soft-commit when this many chars have streamed without a newline.
    /// If unset, disabled; in responsive profile, defaults to 160 chars.
    #[serde(default)]
    pub soft_commit_chars: Option<usize>,

    /// Relax list hold-back: allow list lines with content; only withhold bare markers.
    #[serde(default)]
    pub relax_list_holdback: bool,

    /// Relax code hold-back: allow committing inside an open fenced code block
    /// except the very last partial line.
    #[serde(default)]
    pub relax_code_holdback: bool,

    /// Convenience switch enabling a snappier preset for the above values.
    /// Explicit values above still take precedence if set.
    #[serde(default)]
    pub responsive: bool,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            answer_header_immediate: false,
            show_answer_ellipsis: true,
            commit_tick_ms: None,
            soft_commit_timeout_ms: None,
            soft_commit_chars: None,
            relax_list_holdback: false,
            relax_code_holdback: false,
            responsive: false,
        }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ReasoningSummaryFormat {
    #[default]
    None,
    Experimental,
}

/// Theme configuration for the TUI
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct ThemeConfig {
    /// Name of the predefined theme to use
    #[serde(default)]
    pub name: ThemeName,

    /// Custom color overrides (optional)
    #[serde(default)]
    pub colors: ThemeColors,

    /// Optional display name when using a custom theme generated by the user.
    /// Not used for built-in themes. If `name == Custom` and this is set, the
    /// UI may display it in place of the generic "Custom" label.
    #[serde(default)]
    pub label: Option<String>,

    /// Optional hint whether the custom theme targets a dark background.
    /// When present and `name == Custom`, the UI can show "Dark - <label>"
    /// or "Light - <label>" in lists.
    #[serde(default)]
    pub is_dark: Option<bool>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: ThemeName::default(),
            colors: ThemeColors::default(),
            label: None,
            is_dark: None,
        }
    }
}

/// Selected loading spinner style.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct SpinnerSelection {
    /// Name of the spinner to use. Accepts one of the names from
    /// sindresorhus/cli-spinners (kebab-case), or custom names supported
    /// by Codex. Defaults to "diamond".
    #[serde(default = "default_spinner_name")] 
    pub name: String,
    /// Custom spinner definitions saved by the user
    #[serde(default)]
    pub custom: std::collections::HashMap<String, CustomSpinner>,
}

fn default_spinner_name() -> String { "diamond".to_string() }

impl Default for SpinnerSelection {
    fn default() -> Self {
        Self { name: default_spinner_name(), custom: Default::default() }
    }
}

/// User-defined custom spinner
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct CustomSpinner {
    pub interval: u64,
    pub frames: Vec<String>,
    /// Optional human-readable label to display in the UI
    #[serde(default)]
    pub label: Option<String>,
}

/// Configuration for syntax highlighting in Markdown code blocks.
///
/// `theme` accepts the following values:
/// - "auto" (default): choose a sensible built-in syntect theme based on
///   whether the current UI theme is light or dark.
/// - "<name>": use a specific syntect theme by name from the default ThemeSet.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct HighlightConfig {
    /// Theme selection preference (see docstring for accepted values)
    #[serde(default)]
    pub theme: Option<String>,
}

/// Available predefined themes
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeName {
    // Light themes (at top)
    #[default]
    LightPhoton,
    LightPrismRainbow,
    LightVividTriad,
    LightPorcelain,
    LightSandbar,
    LightGlacier,
    // Dark themes (below)
    DarkCarbonNight,
    DarkShinobiDusk,
    DarkOledBlackPro,
    DarkAmberTerminal,
    DarkAuroraFlux,
    DarkCharcoalRainbow,
    DarkZenGarden,
    DarkPaperLightPro,
    Custom,
}

/// Theme colors that can be customized
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ThemeColors {
    // Primary colors
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub background: Option<String>,
    pub foreground: Option<String>,

    // UI elements
    pub border: Option<String>,
    pub border_focused: Option<String>,
    pub selection: Option<String>,
    pub cursor: Option<String>,

    // Status colors
    pub success: Option<String>,
    pub warning: Option<String>,
    pub error: Option<String>,
    pub info: Option<String>,

    // Text colors
    pub text: Option<String>,
    pub text_dim: Option<String>,
    pub text_bright: Option<String>,

    // Syntax/special colors
    pub keyword: Option<String>,
    pub string: Option<String>,
    pub comment: Option<String>,
    pub function: Option<String>,

    // Animation colors
    pub spinner: Option<String>,
    pub progress: Option<String>,
}

/// Browser configuration for integrated screenshot capabilities.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct BrowserConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub viewport: Option<BrowserViewportConfig>,

    #[serde(default)]
    pub wait: Option<BrowserWaitStrategy>,

    #[serde(default)]
    pub fullpage: bool,

    #[serde(default)]
    pub segments_max: Option<usize>,

    #[serde(default)]
    pub idle_timeout_ms: Option<u64>,

    #[serde(default)]
    pub format: Option<BrowserImageFormat>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct BrowserViewportConfig {
    pub width: u32,
    pub height: u32,

    #[serde(default)]
    pub device_scale_factor: Option<f64>,

    #[serde(default)]
    pub mobile: bool,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum BrowserWaitStrategy {
    Event(String),
    Delay { delay_ms: u64 },
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BrowserImageFormat {
    Png,
    Webp,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct SandboxWorkspaceWrite {
    #[serde(default)]
    pub writable_roots: Vec<PathBuf>,
    #[serde(default)]
    pub network_access: bool,
    #[serde(default)]
    pub exclude_tmpdir_env_var: bool,
    #[serde(default)]
    pub exclude_slash_tmp: bool,
    /// When true, do not protect the top-level `.git` folder under a writable
    /// root. Defaults to true (historical behavior allows Git writes).
    #[serde(default = "crate::config_types::default_true_bool")]
    pub allow_git_writes: bool,
}

// Serde helper: default to true for `allow_git_writes` when omitted.
pub(crate) const fn default_true_bool() -> bool { true }

#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ShellEnvironmentPolicyInherit {
    /// "Core" environment variables for the platform. On UNIX, this would
    /// include HOME, LOGNAME, PATH, SHELL, and USER, among others.
    Core,

    /// Inherits the full environment from the parent process.
    #[default]
    All,

    /// Do not inherit any environment variables from the parent process.
    None,
}

/// Policy for building the `env` when spawning a process via either the
/// `shell` or `local_shell` tool.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ShellEnvironmentPolicyToml {
    pub inherit: Option<ShellEnvironmentPolicyInherit>,

    pub ignore_default_excludes: Option<bool>,

    /// List of regular expressions.
    pub exclude: Option<Vec<String>>,

    pub r#set: Option<HashMap<String, String>>,

    /// List of regular expressions.
    pub include_only: Option<Vec<String>>,

    pub experimental_use_profile: Option<bool>,
}

pub type EnvironmentVariablePattern = WildMatchPattern<'*', '?'>;

/// Deriving the `env` based on this policy works as follows:
/// 1. Create an initial map based on the `inherit` policy.
/// 2. If `ignore_default_excludes` is false, filter the map using the default
///    exclude pattern(s), which are: `"*KEY*"` and `"*TOKEN*"`.
/// 3. If `exclude` is not empty, filter the map using the provided patterns.
/// 4. Insert any entries from `r#set` into the map.
/// 5. If non-empty, filter the map using the `include_only` patterns.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShellEnvironmentPolicy {
    /// Starting point when building the environment.
    pub inherit: ShellEnvironmentPolicyInherit,

    /// True to skip the check to exclude default environment variables that
    /// contain "KEY" or "TOKEN" in their name.
    pub ignore_default_excludes: bool,

    /// Environment variable names to exclude from the environment.
    pub exclude: Vec<EnvironmentVariablePattern>,

    /// (key, value) pairs to insert in the environment.
    pub r#set: HashMap<String, String>,

    /// Environment variable names to retain in the environment.
    pub include_only: Vec<EnvironmentVariablePattern>,

    /// If true, the shell profile will be used to run the command.
    pub use_profile: bool,
}

impl From<ShellEnvironmentPolicyToml> for ShellEnvironmentPolicy {
    fn from(toml: ShellEnvironmentPolicyToml) -> Self {
        // Default to inheriting the full environment when not specified.
        let inherit = toml.inherit.unwrap_or(ShellEnvironmentPolicyInherit::All);
        let ignore_default_excludes = toml.ignore_default_excludes.unwrap_or(false);
        let exclude = toml
            .exclude
            .unwrap_or_default()
            .into_iter()
            .map(|s| EnvironmentVariablePattern::new_case_insensitive(&s))
            .collect();
        let r#set = toml.r#set.unwrap_or_default();
        let include_only = toml
            .include_only
            .unwrap_or_default()
            .into_iter()
            .map(|s| EnvironmentVariablePattern::new_case_insensitive(&s))
            .collect();
        let use_profile = toml.experimental_use_profile.unwrap_or(false);

        Self {
            inherit,
            ignore_default_excludes,
            exclude,
            r#set,
            include_only,
            use_profile,
        }
    }
}

/// See https://platform.openai.com/docs/guides/reasoning?api-mode=responses#get-started-with-reasoning
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq, Display)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ReasoningEffort {
    /// Minimal reasoning. Accepts legacy value "none" for backwards compatibility.
    #[serde(alias = "none")]
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    /// Deprecated: previously disabled reasoning. Kept for internal use only.
    #[serde(skip)]
    None,
}

/// A summary of the reasoning performed by the model. This can be useful for
/// debugging and understanding the model's reasoning process.
/// See https://platform.openai.com/docs/guides/reasoning?api-mode=responses#reasoning-summaries
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq, Display)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ReasoningSummary {
    #[default]
    Auto,
    Concise,
    Detailed,
    /// Option to disable reasoning summaries.
    None,
}

/// Text verbosity level for OpenAI API responses.
/// Controls the level of detail in the model's text responses.
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq, Display)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum TextVerbosity {
    Low,
    #[default]
    Medium,
    High,
}

impl From<codex_protocol::config_types::ReasoningEffort> for ReasoningEffort {
    fn from(v: codex_protocol::config_types::ReasoningEffort) -> Self {
        match v {
            codex_protocol::config_types::ReasoningEffort::Minimal => ReasoningEffort::Minimal,
            codex_protocol::config_types::ReasoningEffort::Low => ReasoningEffort::Low,
            codex_protocol::config_types::ReasoningEffort::Medium => ReasoningEffort::Medium,
            codex_protocol::config_types::ReasoningEffort::High => ReasoningEffort::High,
        }
    }
}

impl From<codex_protocol::config_types::ReasoningSummary> for ReasoningSummary {
    fn from(v: codex_protocol::config_types::ReasoningSummary) -> Self {
        match v {
            codex_protocol::config_types::ReasoningSummary::Auto => ReasoningSummary::Auto,
            codex_protocol::config_types::ReasoningSummary::Concise => ReasoningSummary::Concise,
            codex_protocol::config_types::ReasoningSummary::Detailed => ReasoningSummary::Detailed,
            codex_protocol::config_types::ReasoningSummary::None => ReasoningSummary::None,
        }
    }
}
