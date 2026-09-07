//! Removes the opt-in verification mode from the flattened stable elicitation union.
//!
//! `ExperimentalApi` enum annotations currently drive runtime checks only. Keep this new mode
//! out of stable exports without changing the export behavior of existing gated enum variants.

use super::extract_discriminator_from_arm;
use super::split_top_level;
use super::split_type_alias;
use serde_json::Value;

const MODE: &str = "openai/userVerification";

pub(super) fn filter_ts(content: &str) -> String {
    let Some((prefix, body, suffix)) = split_type_alias(content) else {
        return content.to_string();
    };
    let parts = split_top_level(&body, '&')
        .into_iter()
        .map(|part| {
            let Some(union) = part
                .strip_prefix('(')
                .and_then(|part| part.strip_suffix(')'))
            else {
                return part;
            };
            let arms = split_top_level(union, '|')
                .into_iter()
                .filter(|arm| extract_discriminator_from_arm(arm, "mode").as_deref() != Some(MODE))
                .collect::<Vec<_>>();
            format!("({})", arms.join(" | "))
        })
        .collect::<Vec<_>>();
    format!("{prefix} {}{suffix}", parts.join(" & "))
}

pub(super) fn filter_json(value: &mut Value) {
    match value {
        Value::Array(items) => {
            items.retain(|item| {
                item.pointer("/properties/mode/enum") != Some(&serde_json::json!([MODE]))
            });
            for item in items {
                filter_json(item);
            }
        }
        Value::Object(map) => {
            for entry in map.values_mut() {
                filter_json(entry);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[cfg(test)]
#[path = "export_user_verification_tests.rs"]
mod tests;
