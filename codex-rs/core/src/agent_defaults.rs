//! Defaults for agent CLI parameters per model and access mode.
//!
//! These are used by both the executor (to assemble argv when a model has no
//! explicit per‑mode args configured) and by the TUI editor (to prefill the
//! read‑only/write parameter fields with sensible defaults).

/// Return default CLI arguments (excluding the prompt flag) for a given agent
/// `name` and access mode.
///
/// - The returned vector does NOT include the prompt or `-p` — callers append
///   those as needed.
/// - For write mode, arguments that enable write permissions are included where
///   applicable (e.g., `-y` for Gemini/Qwen; workspace‑write for Code).
pub fn default_params_for(name: &str, read_only: bool) -> Vec<String> {
    match name.to_ascii_lowercase().as_str() {
        // Claude CLI: in read-only, restrict allowed tools; in write, allow full permissions
        "claude" => {
            if read_only {
                vec![
                    "--allowedTools".into(),
                    "Bash(ls:*), Bash(cat:*), Bash(grep:*), Bash(git status:*), Bash(git log:*), Bash(find:*), Read, Grep, Glob, LS, WebFetch, TodoRead, TodoWrite, WebSearch".into(),
                ]
            } else {
                vec!["--dangerously-skip-permissions".into()]
            }
        }
        // Gemini CLI: pin to a stable model by default; write mode adds -y
        "gemini" => {
            let mut v = vec!["--model".to_string(), "gemini-2.5-pro".to_string()];
            if !read_only { v.push("-y".into()); }
            v
        }
        // Qwen CLI: do not pin a model by default; write mode adds -y
        "qwen" => {
            if read_only { Vec::new() } else { vec!["-y".into()] }
        }
        // Built-in codex/code: map to our exec subcommand with appropriate sandbox
        "codex" | "code" => {
            if read_only {
                vec![
                    "-s".into(), "read-only".into(),
                    "-a".into(), "never".into(),
                    "exec".into(), "--skip-git-repo-check".into(),
                ]
            } else {
                vec![
                    "-s".into(), "workspace-write".into(),
                    "-a".into(), "never".into(),
                    "exec".into(), "--skip-git-repo-check".into(),
                ]
            }
        }
        _ => Vec::new(),
    }
}
