//! JSON schema helper for coordinator user-turn responses.

use anyhow::Context;
use serde_json::Value;

use crate::auto_coordinator::extract_first_json_object;

pub fn user_turn_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "user_response": {
                "type": ["string", "null"],
                "maxLength": 400,
                "description": "Short message to respond the USER immediately."
            },
            "cli_command": {
                "type": ["string", "null"],
                "maxLength": 400,
                "description": "Shell command to execute in the CLI this turn. Use null when no CLI action is required."
            }
        },
        "required": ["user_response", "cli_command"]
    })
}

pub fn parse_user_turn_reply(raw: &str) -> anyhow::Result<(Option<String>, Option<String>)> {
    let value: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(first_err) => {
            let Some(blob) = extract_first_json_object(raw) else {
                return Err(first_err).context("parsing coordinator user turn JSON");
            };
            let first_err_msg = first_err.to_string();
            serde_json::from_str(&blob).with_context(|| {
                format!(
                    "parsing coordinator user turn JSON (after salvage); initial parse error: {first_err_msg}"
                )
            })?
        }
    };
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("coordinator response was not a JSON object"))?;

    let extract = |name: &str| -> anyhow::Result<Option<String>> {
        let field = obj
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("coordinator response missing required field '{name}'"))?;
        if field.is_null() {
            return Ok(None);
        }
        let Some(text) = field.as_str() else {
            return Err(anyhow::anyhow!("coordinator field '{name}' must be string or null"));
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        if trimmed.chars().count() > 400 {
            return Err(anyhow::anyhow!("coordinator field '{name}' exceeded 400 characters"));
        }
        Ok(Some(trimmed.to_string()))
    };

    Ok((extract("user_response")?, extract("cli_command")?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn parse_user_turn_reply_strict_object() -> Result<()> {
        let raw = r#"{"user_response":"  Thanks!  ","cli_command":null}"#;
        let (user, cli) = parse_user_turn_reply(raw)?;
        assert_eq!(user.as_deref(), Some("Thanks!"));
        assert_eq!(cli, None);
        Ok(())
    }

    #[test]
    fn parse_user_turn_reply_salvages_embedded_json() -> Result<()> {
        let raw = r#"Here are two options: do A or B.
{"user_response":null,"cli_command":" echo done "}
Let me know."#;
        let (user, cli) = parse_user_turn_reply(raw)?;
        assert_eq!(user, None);
        assert_eq!(cli.as_deref(), Some("echo done"));
        Ok(())
    }
}
