use std::env;
use std::fs;
use std::io::Write;
use std::process::Command;

use tempfile::Builder;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum ExternalEditorError {
    #[error("Set $VISUAL or $EDITOR to use the external editor.")]
    MissingEditor,
    #[error("Failed to parse editor command.")]
    ParseFailed,
    #[error("Editor command is empty.")]
    EmptyCommand,
    #[error("Failed to launch editor: {0}")]
    LaunchFailed(String),
    #[error("Editor exited with status {0}.")]
    NonZeroExit(String),
    #[error("Failed to read edited content: {0}")]
    ReadFailed(String),
}

pub(crate) fn run_editor(initial: &str) -> Result<String, ExternalEditorError> {
    let command = resolve_editor_command()?;
    let (program, args) = command
        .split_first()
        .ok_or(ExternalEditorError::EmptyCommand)?;

    let mut temp = Builder::new()
        .prefix("code-edit-")
        .suffix(".md")
        .tempfile()
        .map_err(|e| ExternalEditorError::ReadFailed(e.to_string()))?;
    if !initial.is_empty() {
        std::io::Write::write_all(&mut temp, initial.as_bytes())
            .map_err(|e| ExternalEditorError::ReadFailed(e.to_string()))?;
    }
    temp.flush()
        .map_err(|e| ExternalEditorError::ReadFailed(e.to_string()))?;

    let temp_path = temp.into_temp_path();
    let path = temp_path.to_path_buf();
    let path_str = path.to_string_lossy().to_string();

    let mut args = args.to_vec();
    let mut replaced = false;
    for arg in &mut args {
        if arg.contains("{file}") {
            *arg = arg.replace("{file}", &path_str);
            replaced = true;
        } else if arg == "{}" {
            *arg = path_str.clone();
            replaced = true;
        }
    }
    if !replaced {
        args.push(path_str.clone());
    }

    let status = Command::new(program)
        .args(&args)
        .status()
        .map_err(|e| ExternalEditorError::LaunchFailed(e.to_string()))?;
    if !status.success() {
        return Err(ExternalEditorError::NonZeroExit(format!("{status:?}")));
    }

    fs::read_to_string(&path)
        .map_err(|e| ExternalEditorError::ReadFailed(e.to_string()))
}

fn resolve_editor_command() -> Result<Vec<String>, ExternalEditorError> {
    let visual = env::var("VISUAL").ok().filter(|val| !val.trim().is_empty());
    let editor = env::var("EDITOR").ok().filter(|val| !val.trim().is_empty());
    let raw = visual.or(editor).ok_or(ExternalEditorError::MissingEditor)?;
    parse_editor_command(&raw)
}

fn parse_editor_command(raw: &str) -> Result<Vec<String>, ExternalEditorError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ExternalEditorError::EmptyCommand);
    }
    let parsed = shlex::split(trimmed).ok_or(ExternalEditorError::ParseFailed)?;
    if parsed.is_empty() {
        return Err(ExternalEditorError::EmptyCommand);
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::parse_editor_command;

    #[test]
    fn parse_editor_command_splits_args() {
        let parsed = parse_editor_command("code --wait --reuse-window").expect("parse");
        assert_eq!(parsed, vec!["code", "--wait", "--reuse-window"]);
    }
}
