//! Apply credential and environment policy to captured exports, producing typed render values.
//! Native declarations are rendered only after applying the supplied credential policy.

use super::capture::CapturedSnapshot;
use super::render;
use super::render::Export;
use super::render::Value;
use super::render::ValuePart;
use std::collections::HashMap;

/// Environment views used to render protected credential references into a shell snapshot.
pub struct SnapshotCredentialEnvironment<'a> {
    pub original: &'a HashMap<String, String>,
    pub restored: &'a HashMap<String, String>,
    pub configured: &'a HashMap<String, String>,
    pub discovered: &'a HashMap<String, String>,
    pub allowed: &'a HashMap<String, String>,
    /// Applies environment policy to export-only names absent from the captured environment.
    pub is_allowed_unset: &'a dyn Fn(&str) -> bool,
    pub brokered_keys: &'a [String],
    pub brokered_alias_keys: &'a [String],
    pub allowed_brokered_keys: &'a [String],
}

/// Credential-checked replay script and aliases to restore after shell startup.
pub struct PreparedSnapshot {
    pub script: String,
    pub aliases: HashMap<String, String>,
    pub rejected_alias_keys: Vec<String>,
}

/// Prepare a replay script from captured state without changing the captured data.
///
/// Credential policy is applied before rendering. A rejected capture produces no script,
/// when the assembled source still contains a recognized credential.
pub fn prepare_snapshot_credentials(
    captured: &CapturedSnapshot<'_>,
    environment: SnapshotCredentialEnvironment<'_>,
    mut virtualize_text: impl FnMut(&mut String) -> bool,
) -> Option<PreparedSnapshot> {
    let SnapshotCredentialEnvironment {
        original,
        restored,
        configured,
        discovered,
        allowed,
        is_allowed_unset,
        brokered_keys,
        brokered_alias_keys,
        allowed_brokered_keys,
    } = environment;
    let real_credential_value = |key: &str| restored.get(key).or_else(|| configured.get(key));
    let mut credential_aliases = original
        .iter()
        .filter(|(key, _)| !brokered_keys.contains(key) && !brokered_alias_keys.contains(key))
        .filter_map(|(key, value)| {
            let credential_keys = brokered_keys
                .iter()
                .filter(|credential_key| {
                    real_credential_value(credential_key).is_some_and(|real| {
                        value == real || real.len() >= 16 && value.contains(real)
                    }) || discovered.get(*credential_key).is_some_and(|dummy| {
                        value == dummy || dummy.len() >= 16 && value.contains(dummy)
                    })
                })
                .map(String::as_str)
                .collect::<Vec<_>>();
            (!credential_keys.is_empty()).then_some((key.as_str(), credential_keys))
        })
        .filter(|(key, _)| {
            key.bytes().enumerate().all(|(index, byte)| {
                byte == b'_' || byte.is_ascii_alphabetic() || (index != 0 && byte.is_ascii_digit())
            })
        })
        .collect::<HashMap<_, _>>();
    let credential_alias_assignment = |value: &str, credential_keys: &[&str]| {
        let mut remaining = value;
        let mut assignment = Value::default();
        let mut normalized = String::new();
        while !remaining.is_empty() {
            let next_credential = credential_keys
                .iter()
                .filter_map(|credential_key| {
                    let dummy = discovered.get(*credential_key)?;
                    let real = real_credential_value(credential_key)?;
                    let allowed_key = allowed_brokered_keys
                        .iter()
                        .find(|allowed_key| allowed_key.as_str() == *credential_key)
                        .or_else(|| {
                            allowed_brokered_keys.iter().find(|allowed_key| {
                                real_credential_value(allowed_key) == Some(real)
                            })
                        })?;
                    remaining
                        .find(dummy)
                        .map(|index| (index, allowed_key.as_str(), dummy.as_str()))
                })
                .min_by_key(|(index, _, _)| *index);
            let Some((index, credential_key, dummy)) = next_credential else {
                assignment
                    .parts
                    .push(ValuePart::Literal(remaining.to_string()));
                normalized.push_str(remaining);
                break;
            };
            if index != 0 {
                assignment
                    .parts
                    .push(ValuePart::Literal(remaining[..index].to_string()));
                normalized.push_str(&remaining[..index]);
            }
            assignment.parts.push(ValuePart::Credential {
                key: credential_key.to_string(),
            });
            normalized.push_str(allowed.get(credential_key)?);
            remaining = &remaining[index + dummy.len()..];
        }
        Some((assignment, normalized))
    };
    let credential_alias_is_allowed = |credential_keys: &[&str]| {
        credential_keys.iter().all(|credential_key| {
            real_credential_value(credential_key).is_some_and(|real| {
                allowed_brokered_keys
                    .iter()
                    .any(|allowed_key| real_credential_value(allowed_key) == Some(real))
            })
        })
    };
    let is_disallowed_credential_alias =
        |key: &str, virtualize_text: &mut dyn FnMut(&mut String) -> bool| {
            original.get(key).is_some_and(|value| {
                let mut virtualized = value.clone();
                !virtualize_text(&mut virtualized)
            })
        };
    let mut alias_values = HashMap::new();
    let mut rejected_alias_keys = Vec::new();
    let mut invalid_export = false;
    let exports = captured
        .exports
        .iter()
        .filter_map(|export| {
            let line = export.source.as_ref();
            let key = export.key;

            if (!allowed.contains_key(key)
                && (original.contains_key(key) || !is_allowed_unset(key)))
                || brokered_keys
                    .iter()
                    .any(|credential_key| credential_key == key)
                || brokered_alias_keys
                    .iter()
                    .any(|credential_key| credential_key == key)
                || is_disallowed_credential_alias(key, &mut virtualize_text)
            {
                return None;
            }

            if let Some(credential_keys) = credential_aliases.remove(key) {
                if !credential_alias_is_allowed(&credential_keys) {
                    rejected_alias_keys.push(key.to_string());
                    return None;
                }
                let declaration = line
                    .split_once('=')
                    .map_or(line.trim_end(), |(prefix, _)| prefix);
                let (assignment, value) =
                    credential_alias_assignment(allowed.get(key)?, &credential_keys)?;
                alias_values.insert(key.to_string(), value);
                if line
                    .split_whitespace()
                    .skip(/*n*/ 1)
                    .take_while(|word| word.starts_with('-'))
                    .any(|flags| flags.contains('T'))
                {
                    invalid_export = true;
                    return Some(Export::Captured(line));
                }
                return Some(Export::Assignment {
                    declaration,
                    value: assignment,
                });
            }

            Some(Export::Captured(line))
        })
        .collect::<Vec<_>>();
    let mut snapshot = render::render(captured.state, captured.aliases, &exports)?;
    if invalid_export {
        return None;
    }

    for (key, credential_keys) in credential_aliases {
        if !is_disallowed_credential_alias(key, &mut virtualize_text)
            && credential_alias_is_allowed(&credential_keys)
            && let Some((_, value)) = allowed
                .get(key)
                .and_then(|value| credential_alias_assignment(value, &credential_keys))
        {
            // Native capture can deliberately omit readonly exports; retain only their metadata.
            alias_values.insert(key.to_string(), value);
        } else {
            rejected_alias_keys.push(key.to_string());
        }
    }

    if !virtualize_text(&mut snapshot) {
        return None;
    }

    Some(PreparedSnapshot {
        script: snapshot,
        aliases: alias_values,
        rejected_alias_keys,
    })
}
