use std::collections::BTreeMap;

use crate::openai_tools::JsonSchema;
use crate::openai_tools::ResponsesApiTool;

pub const EXEC_COMMAND_TOOL_NAME: &str = "exec_command";
pub const WRITE_STDIN_TOOL_NAME: &str = "write_stdin";

pub fn create_exec_command_tool_for_responses_api() -> ResponsesApiTool {
    let mut properties = BTreeMap::<String, JsonSchema>::new();
    properties.insert(
        "cmd".to_string(),
        JsonSchema::String {
            description: Some("Shell command to execute.".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "workdir".to_string(),
        JsonSchema::String {
            description: Some(
                "Optional working directory to run the command in; defaults to the turn cwd."
                    .to_string(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "shell".to_string(),
        JsonSchema::String {
            description: Some("Shell binary to launch. Defaults to /bin/bash.".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "login".to_string(),
        JsonSchema::Boolean {
            description: Some(
                "Whether to run the shell with -l/-i semantics. Defaults to true.".to_string(),
            ),
        },
    );
    properties.insert(
        "yield_time_ms".to_string(),
        JsonSchema::Number {
            description: Some(
                "How long to wait (in milliseconds) for output before yielding.".to_string(),
            ),
        },
    );
    properties.insert(
        "max_output_tokens".to_string(),
        JsonSchema::Number {
            description: Some(
                "Maximum number of tokens to return. Excess output will be truncated."
                    .to_string(),
            ),
        },
    );
    properties.insert(
        "sandbox_permissions".to_string(),
        JsonSchema::String {
            description: Some(
                "Sandbox permissions for the command. Set to \"require_escalated\" to request running without sandbox restrictions; defaults to \"use_default\"."
                    .to_string(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "justification".to_string(),
        JsonSchema::String {
            description: Some(
                "Only set if sandbox_permissions is \"require_escalated\". 1-sentence explanation of why we want to run this command."
                    .to_string(),
            ),
            allowed_values: None,
        },
    );

    ResponsesApiTool {
        name: EXEC_COMMAND_TOOL_NAME.to_owned(),
        description:
            "Runs a command in a PTY, returning output or a session ID for ongoing interaction."
            .to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["cmd".to_string()]),
            additional_properties: Some(false.into()),
        },
    }
}

pub fn create_write_stdin_tool_for_responses_api() -> ResponsesApiTool {
    let mut properties = BTreeMap::<String, JsonSchema>::new();
    properties.insert(
        "session_id".to_string(),
        JsonSchema::Number {
            description: Some("Identifier of the running unified exec session.".to_string()),
        },
    );
    properties.insert(
        "chars".to_string(),
        JsonSchema::String {
            description: Some("Bytes to write to stdin (may be empty to poll).".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "yield_time_ms".to_string(),
        JsonSchema::Number {
            description: Some(
                "How long to wait (in milliseconds) for output before yielding.".to_string(),
            ),
        },
    );
    properties.insert(
        "max_output_tokens".to_string(),
        JsonSchema::Number {
            description: Some(
                "Maximum number of tokens to return. Excess output will be truncated."
                    .to_string(),
            ),
        },
    );

    ResponsesApiTool {
        name: WRITE_STDIN_TOOL_NAME.to_owned(),
        description:
            "Writes characters to an existing unified exec session and returns recent output."
            .to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["session_id".to_string()]),
            additional_properties: Some(false.into()),
        },
    }
}
