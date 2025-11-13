use crate::config::resolve_code_path_for_read;
use crate::config_types::SubagentCommandConfig;
use anyhow::Result;
use std::path::Path;
use tempfile::NamedTempFile;
use toml_edit::DocumentMut;

pub const CONFIG_KEY_MODEL: &str = "model";
pub const CONFIG_KEY_EFFORT: &str = "model_reasoning_effort";
const CONFIG_TOML_FILE: &str = "config.toml";

#[derive(Copy, Clone)]
enum NoneBehavior {
    Skip,
    Remove,
}

/// Persist overrides into `config.toml` using explicit key segments per
/// override. This avoids ambiguity with keys that contain dots or spaces.
pub async fn persist_overrides(
    code_home: &Path,
    profile: Option<&str>,
    overrides: &[(&[&str], &str)],
) -> Result<()> {
    let with_options: Vec<(&[&str], Option<&str>)> = overrides
        .iter()
        .map(|(segments, value)| (*segments, Some(*value)))
        .collect();

    persist_overrides_with_behavior(code_home, profile, &with_options, NoneBehavior::Skip).await
}

/// Persist overrides where values may be optional. Any entries with `None`
/// values are skipped. If all values are `None`, this becomes a no-op and
/// returns `Ok(())` without touching the file.
pub async fn persist_non_null_overrides(
    code_home: &Path,
    profile: Option<&str>,
    overrides: &[(&[&str], Option<&str>)],
) -> Result<()> {
    persist_overrides_with_behavior(code_home, profile, overrides, NoneBehavior::Skip).await
}

/// Persist overrides where `None` values clear any existing values from the
/// configuration file.
pub async fn persist_overrides_and_clear_if_none(
    code_home: &Path,
    profile: Option<&str>,
    overrides: &[(&[&str], Option<&str>)],
) -> Result<()> {
    persist_overrides_with_behavior(code_home, profile, overrides, NoneBehavior::Remove).await
}

/// Apply a single override onto a `toml_edit` document while preserving
/// existing formatting/comments.
/// The key is expressed as explicit segments to correctly handle keys that
/// contain dots or spaces.
fn apply_toml_edit_override_segments(
    doc: &mut DocumentMut,
    segments: &[&str],
    value: toml_edit::Item,
) {
    use toml_edit::Item;

    if segments.is_empty() {
        return;
    }

    let mut current = doc.as_table_mut();
    for seg in &segments[..segments.len() - 1] {
        if !current.contains_key(seg) {
            current[*seg] = Item::Table(toml_edit::Table::new());
            if let Some(t) = current[*seg].as_table_mut() {
                t.set_implicit(true);
            }
        }

        let maybe_item = current.get_mut(seg);
        let Some(item) = maybe_item else { return };

        if !item.is_table() {
            *item = Item::Table(toml_edit::Table::new());
            if let Some(t) = item.as_table_mut() {
                t.set_implicit(true);
            }
        }

        let Some(tbl) = item.as_table_mut() else {
            return;
        };
        current = tbl;
    }

    let last = segments[segments.len() - 1];
    current[last] = value;
}

/// Upsert a `[[subagents.commands]]` entry by `name`.
/// If an entry with the same (case-insensitive) name exists, it is updated; otherwise a new entry is appended.
pub async fn upsert_subagent_command(code_home: &Path, cmd: &SubagentCommandConfig) -> Result<()> {
    const CONFIG_TOML_FILE: &str = "config.toml";
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match tokio::fs::read_to_string(&read_path).await {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    // Ensure [subagents] exists
    if !doc.as_table().contains_key("subagents") {
        doc["subagents"] = toml_edit::table();
        if let Some(t) = doc["subagents"].as_table_mut() { t.set_implicit(false); }
    }

    // Search for existing by name (case-insensitive) and rebuild commands array
    let mut updated = false;
    let mut new_commands = toml_edit::ArrayOfTables::new();
    if let Some(arr) = doc["subagents"].get("commands").and_then(|i| i.as_array_of_tables()) {
        for tbl_ref in arr.iter() {
            let mut tbl = tbl_ref.clone();
            let same = tbl
                .get("name")
                .and_then(|i| i.as_str())
                .map(|s| s.eq_ignore_ascii_case(&cmd.name))
                .unwrap_or(false);
            if same {
                // Update fields
                tbl["name"] = toml_edit::value(cmd.name.clone());
                tbl["read-only"] = toml_edit::value(cmd.read_only);
                let agents = toml_edit::Array::from_iter(cmd.agents.iter().cloned());
                tbl["agents"] = toml_edit::Item::Value(toml_edit::Value::Array(agents));
                if let Some(s) = &cmd.orchestrator_instructions { tbl["orchestrator-instructions"] = toml_edit::value(s.clone()); } else { tbl.remove("orchestrator-instructions"); }
                if let Some(s) = &cmd.agent_instructions { tbl["agent-instructions"] = toml_edit::value(s.clone()); } else { tbl.remove("agent-instructions"); }
                updated = true;
            }
            new_commands.push(tbl);
        }
    }
    if !updated {
        let mut t = toml_edit::Table::new();
        t.set_implicit(true);
        t["name"] = toml_edit::value(cmd.name.clone());
        t["read-only"] = toml_edit::value(cmd.read_only);
        let agents = toml_edit::Array::from_iter(cmd.agents.iter().cloned());
        t["agents"] = toml_edit::Item::Value(toml_edit::Value::Array(agents));
        if let Some(s) = &cmd.orchestrator_instructions {
            t["orchestrator-instructions"] = toml_edit::value(s.clone());
        }
        if let Some(s) = &cmd.agent_instructions {
            t["agent-instructions"] = toml_edit::value(s.clone());
        }
        new_commands.push(t);
    }

    doc["subagents"]["commands"] = toml_edit::Item::ArrayOfTables(new_commands);

    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;
    Ok(())
}

/// Delete a `[[subagents.commands]]` entry by name. Returns true if removed.
pub async fn delete_subagent_command(code_home: &Path, name: &str) -> Result<bool> {
    const CONFIG_TOML_FILE: &str = "config.toml";
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match tokio::fs::read_to_string(&read_path).await {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };

    let Some(arr) = doc["subagents"].get_mut("commands").and_then(|i| i.as_array_of_tables_mut()) else {
        return Ok(false);
    };

    let before = arr.len();
    arr.retain(|t| {
        !t.get("name")
            .and_then(|i| i.as_str())
            .map(|s| s.eq_ignore_ascii_case(name))
            .unwrap_or(false)
    });
    let removed = arr.len() != before;
    if removed {
        let tmp_file = NamedTempFile::new_in(code_home)?;
        tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
        tmp_file.persist(config_path)?;
    }
    Ok(removed)
}

/// Upsert an `[[agents]]` entry by `name`. If an entry with the same
/// (case-insensitive) name exists, update selected fields; otherwise append a
/// new entry with the provided values. Fields not managed by the editor are
/// preserved when updating.
pub async fn upsert_agent_config(
    code_home: &Path,
    name: &str,
    enabled: Option<bool>,
    args: Option<&[String]>,
    args_read_only: Option<&[String]>,
    args_write: Option<&[String]>,
    instructions: Option<&str>,
    description: Option<&str>,
    command: Option<&str>,
) -> Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match tokio::fs::read_to_string(&read_path).await {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    // Search existing [[agents]] for a case‑insensitive name match
    let mut found = false;
    if let Some(item) = doc.as_table().get("agents").cloned() {
        let Some(arr) = item.as_array_of_tables() else { /* not an array, treat as missing */ return write_new_or_append(doc, code_home, config_path, name, enabled, args, args_read_only, args_write, instructions, description, command).await };
        let mut new_arr = toml_edit::ArrayOfTables::new();
        for tbl_ref in arr.iter() {
            let mut tbl = tbl_ref.clone();
            let same = tbl
                .get("name")
                .and_then(|i| i.as_str())
                .map(|s| s.eq_ignore_ascii_case(name))
                .unwrap_or(false);
            if same {
                if let Some(val) = enabled { tbl["enabled"] = toml_edit::value(val); }
                if let Some(a) = args { tbl["args"] = toml_edit::value(toml_edit::Array::from_iter(a.iter().cloned())); }
                if let Some(ro) = args_read_only {
                    tbl["args-read-only"] = toml_edit::value(toml_edit::Array::from_iter(ro.iter().cloned()));
                }
                if let Some(w) = args_write {
                    tbl["args-write"] = toml_edit::value(toml_edit::Array::from_iter(w.iter().cloned()));
                }
                if let Some(instr) = instructions {
                    if instr.trim().is_empty() { tbl.remove("instructions"); }
                    else { tbl["instructions"] = toml_edit::value(instr.to_string()); }
                }
                if let Some(desc) = description {
                    if desc.trim().is_empty() {
                        tbl.remove("description");
                    } else {
                        tbl["description"] = toml_edit::value(desc.to_string());
                    }
                }
                if let Some(cmd) = command {
                    if cmd.trim().is_empty() {
                        tbl.remove("command");
                    } else {
                        tbl["command"] = toml_edit::value(cmd.to_string());
                    }
                }
                found = true;
            }
            new_arr.push(tbl);
        }
        doc["agents"] = toml_edit::Item::ArrayOfTables(new_arr);
    }

    if !found {
        // Append a new entry safely
        append_agent_entry(&mut doc, name, enabled, args, args_read_only, args_write, instructions, description, command);
    }

    // Write back atomically
    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;
    Ok(())
}

// Helper: append a single [[agents]] entry (no-alloc fallible path wrapper above)
fn append_agent_entry(
    doc: &mut DocumentMut,
    name: &str,
    enabled: Option<bool>,
    args: Option<&[String]>,
    args_read_only: Option<&[String]>,
    args_write: Option<&[String]>,
    instructions: Option<&str>,
    description: Option<&str>,
    command: Option<&str>,
) {
    let mut t = toml_edit::Table::new();
    t.set_implicit(true);
    t["name"] = toml_edit::value(name.to_string());
    if let Some(val) = enabled { t["enabled"] = toml_edit::value(val); }
    if let Some(a) = args { t["args"] = toml_edit::value(toml_edit::Array::from_iter(a.iter().cloned())); }
    if let Some(ro) = args_read_only { t["args-read-only"] = toml_edit::value(toml_edit::Array::from_iter(ro.iter().cloned())); }
    if let Some(w) = args_write { t["args-write"] = toml_edit::value(toml_edit::Array::from_iter(w.iter().cloned())); }
    if let Some(instr) = instructions { if !instr.trim().is_empty() { t["instructions"] = toml_edit::value(instr.to_string()); } }
    if let Some(desc) = description { if !desc.trim().is_empty() { t["description"] = toml_edit::value(desc.to_string()); } }
    if let Some(cmd) = command {
        if !cmd.trim().is_empty() {
            t["command"] = toml_edit::value(cmd.to_string());
        }
    }

    let mut arr = doc
        .as_table()
        .get("agents")
        .and_then(|i| i.as_array_of_tables().cloned())
        .unwrap_or_else(toml_edit::ArrayOfTables::new);
    arr.push(t);
    doc["agents"] = toml_edit::Item::ArrayOfTables(arr);
}

async fn write_new_or_append(
    mut doc: DocumentMut,
    code_home: &Path,
    config_path: std::path::PathBuf,
    name: &str,
    enabled: Option<bool>,
    args: Option<&[String]>,
    args_read_only: Option<&[String]>,
    args_write: Option<&[String]>,
    instructions: Option<&str>,
    description: Option<&str>,
    command: Option<&str>,
) -> Result<()> {
    append_agent_entry(&mut doc, name, enabled, args, args_read_only, args_write, instructions, description, command);
    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;
    Ok(())
}


// Internal helper to support persist_* variants above.
async fn persist_overrides_with_behavior(
    code_home: &Path,
    profile: Option<&str>,
    overrides: &[(&[&str], Option<&str>)],
    none_behavior: NoneBehavior,
) -> Result<()> {
    if overrides.is_empty() {
        return Ok(());
    }

    let should_skip = match none_behavior {
        NoneBehavior::Skip => overrides.iter().all(|(_, value)| value.is_none()),
        NoneBehavior::Remove => false,
    };

    if should_skip {
        return Ok(());
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let read_result = tokio::fs::read_to_string(&read_path).await;
    let mut doc = match read_result {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if overrides.iter().all(|(_, value)| value.is_none() && matches!(none_behavior, NoneBehavior::Remove)) {
                return Ok(());
            }
            tokio::fs::create_dir_all(code_home).await?;
            DocumentMut::new()
        }
        Err(e) => return Err(e.into()),
    };

    let effective_profile = if let Some(p) = profile { Some(p.to_owned()) } else { doc.get("profile").and_then(|i| i.as_str()).map(|s| s.to_string()) };

    let mut mutated = false;
    for (segments, value) in overrides.iter().copied() {
        let mut seg_buf: Vec<&str> = Vec::new();
        let segments_to_apply: &[&str];
        if let Some(ref name) = effective_profile {
            if segments.first().copied() == Some("profiles") {
                segments_to_apply = segments;
            } else {
                seg_buf.reserve(2 + segments.len());
                seg_buf.push("profiles");
                seg_buf.push(name.as_str());
                seg_buf.extend_from_slice(segments);
                segments_to_apply = seg_buf.as_slice();
            }
        } else {
            segments_to_apply = segments;
        }
        match value {
            Some(v) => {
                let trimmed = v.trim();
                let item_value = match trimmed.parse::<bool>() {
                    Ok(parsed_bool) => toml_edit::value(parsed_bool),
                    Err(_) => toml_edit::value(v),
                };
                apply_toml_edit_override_segments(&mut doc, segments_to_apply, item_value);
                mutated = true;
            }
            None => {
                if matches!(none_behavior, NoneBehavior::Remove) && remove_toml_edit_segments(&mut doc, segments_to_apply) {
                    mutated = true;
                }
            }
        }
    }
    if !mutated { return Ok(()); }
    let tmp_file = NamedTempFile::new_in(code_home)?;
    tokio::fs::write(tmp_file.path(), doc.to_string()).await?;
    tmp_file.persist(config_path)?;
    Ok(())
}

fn remove_toml_edit_segments(doc: &mut DocumentMut, segments: &[&str]) -> bool {
    use toml_edit::Item;
    if segments.is_empty() { return false; }
    let mut current = doc.as_table_mut();
    for seg in &segments[..segments.len() - 1] {
        let Some(item) = current.get_mut(seg) else { return false }; 
        match item { Item::Table(table) => { current = table; } _ => { return false; } }
    }
    current.remove(segments[segments.len() - 1]).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    /// Verifies model and effort are written at top-level when no profile is set.
    #[tokio::test]
    async fn set_default_model_and_effort_top_level_when_no_profile() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_overrides(
            code_home,
            None,
            &[
                (&[CONFIG_KEY_MODEL], "gpt-5.1-codex"),
                (&[CONFIG_KEY_EFFORT], "high"),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let table = parsed.as_table().expect("root table");
        assert_eq!(
            table
                .get(CONFIG_KEY_MODEL)
                .and_then(|value| value.as_str()),
            Some("gpt-5.1-codex")
        );
        assert_eq!(
            table
                .get(CONFIG_KEY_EFFORT)
                .and_then(|value| value.as_str()),
            Some("high")
        );
    }

    /// Verifies values are written under the active profile when `profile` is set.
    #[tokio::test]
    async fn set_defaults_update_profile_when_profile_set() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Seed config with a profile selection but without profiles table
        let seed = "profile = \"o3\"\n";
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_overrides(
            code_home,
            None,
            &[
                (&[CONFIG_KEY_MODEL], "o3"),
                (&[CONFIG_KEY_EFFORT], "minimal"),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"profile = "o3"

[profiles.o3]
model = "o3"
model_reasoning_effort = "minimal"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies profile names with dots/spaces are preserved via explicit segments.
    #[tokio::test]
    async fn set_defaults_update_profile_with_dot_and_space() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Seed config with a profile name that contains a dot and a space
        let seed = "profile = \"my.team name\"\n";
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_overrides(
            code_home,
            None,
            &[
                (&[CONFIG_KEY_MODEL], "o3"),
                (&[CONFIG_KEY_EFFORT], "minimal"),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"profile = "my.team name"

[profiles."my.team name"]
model = "o3"
model_reasoning_effort = "minimal"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies explicit profile override writes under that profile even without active profile.
    #[tokio::test]
    async fn set_defaults_update_when_profile_override_supplied() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // No profile key in config.toml
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), "")
            .await
            .expect("seed write");

        // Persist with an explicit profile override
        persist_overrides(
            code_home,
            Some("o3"),
            &[(&[CONFIG_KEY_MODEL], "o3"), (&[CONFIG_KEY_EFFORT], "high")],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"[profiles.o3]
model = "o3"
model_reasoning_effort = "high"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies nested tables are created as needed when applying overrides.
    #[tokio::test]
    async fn persist_overrides_creates_nested_tables() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_overrides(
            code_home,
            None,
            &[
                (&["a", "b", "c"], "v"),
                (&["x"], "y"),
                (&["profiles", "p1", CONFIG_KEY_MODEL], "gpt-5.1-codex"),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let table = parsed.as_table().expect("root table");
        assert_eq!(
            table.get("x").and_then(toml::Value::as_str),
            Some("y")
        );
        let a_table = table
            .get("a")
            .and_then(toml::Value::as_table)
            .expect("a table");
        let b_table = a_table
            .get("b")
            .and_then(toml::Value::as_table)
            .expect("b table");
        assert_eq!(b_table.get("c").and_then(toml::Value::as_str), Some("v"));
        let profiles = table
            .get("profiles")
            .and_then(toml::Value::as_table)
            .expect("profiles table");
        let p1 = profiles
            .get("p1")
            .and_then(toml::Value::as_table)
            .expect("profile p1");
        assert_eq!(
            p1.get(CONFIG_KEY_MODEL).and_then(toml::Value::as_str),
            Some("gpt-5.1-codex")
        );
    }

    #[tokio::test]
    async fn persist_overrides_writes_boolean_literals() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_overrides(code_home, None, &[(&["auto_upgrade_enabled"], "true")])
            .await
            .expect("persist");

        let contents = read_config(code_home).await;
        assert!(contents.contains("auto_upgrade_enabled = true"));

        persist_overrides(code_home, None, &[(&["auto_upgrade_enabled"], "false")])
            .await
            .expect("persist");

        let contents = read_config(code_home).await;
        assert!(contents.contains("auto_upgrade_enabled = false"));
    }

    /// Verifies a scalar key becomes a table when nested keys are written.
    #[tokio::test]
    async fn persist_overrides_replaces_scalar_with_table() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();
        let seed = "foo = \"bar\"\n";
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_overrides(code_home, None, &[(&["foo", "bar", "baz"], "ok")])
            .await
            .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"[foo.bar]
baz = "ok"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies comments and spacing are preserved when writing under active profile.
    #[tokio::test]
    async fn set_defaults_preserve_comments() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Seed a config with comments and spacing we expect to preserve
        let seed = r#"# Global comment
# Another line

profile = "o3"

# Profile settings
[profiles.o3]
# keep me
existing = "keep"
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        // Apply defaults; since profile is set, it should write under [profiles.o3]
        persist_overrides(
            code_home,
            None,
            &[(&[CONFIG_KEY_MODEL], "o3"), (&[CONFIG_KEY_EFFORT], "high")],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"# Global comment
# Another line

profile = "o3"

# Profile settings
[profiles.o3]
# keep me
existing = "keep"
model = "o3"
model_reasoning_effort = "high"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies comments and spacing are preserved when writing at top level.
    #[tokio::test]
    async fn set_defaults_preserve_global_comments() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Seed a config WITHOUT a profile, containing comments and spacing
        let seed = r#"# Top-level comments
# should be preserved

existing = "keep"
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        // Since there is no profile, the defaults should be written at top-level
        persist_overrides(
            code_home,
            None,
            &[
                (&[CONFIG_KEY_MODEL], "gpt-5.1-codex"),
                (&[CONFIG_KEY_EFFORT], "minimal"),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"# Top-level comments
# should be preserved

existing = "keep"
model = "gpt-5.1-codex"
model_reasoning_effort = "minimal"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies errors on invalid TOML propagate and file is not clobbered.
    #[tokio::test]
    async fn persist_overrides_errors_on_parse_failure() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Write an intentionally invalid TOML file
        let invalid = "invalid = [unclosed";
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), invalid)
            .await
            .expect("seed write");

        // Attempting to persist should return an error and must not clobber the file.
        let res = persist_overrides(code_home, None, &[(&["x"], "y")]).await;
        assert!(res.is_err(), "expected parse error to propagate");

        // File should be unchanged
        let contents = read_config(code_home).await;
        assert_eq!(contents, invalid);
    }

    /// Verifies changing model only preserves existing effort at top-level.
    #[tokio::test]
    async fn changing_only_model_preserves_existing_effort_top_level() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Seed with an effort value only
        let seed = "model_reasoning_effort = \"minimal\"\n";
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        // Change only the model
        persist_overrides(code_home, None, &[(&[CONFIG_KEY_MODEL], "o3")])
            .await
            .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"model_reasoning_effort = "minimal"
model = "o3"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies changing effort only preserves existing model at top-level.
    #[tokio::test]
    async fn changing_only_effort_preserves_existing_model_top_level() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Seed with a model value only
        let seed = "model = \"gpt-5.1-codex\"\n";
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        // Change only the effort
        persist_overrides(code_home, None, &[(&[CONFIG_KEY_EFFORT], "high")])
            .await
            .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"model = "gpt-5.1-codex"
model_reasoning_effort = "high"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies changing model only preserves existing effort in active profile.
    #[tokio::test]
    async fn changing_only_model_preserves_effort_in_active_profile() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // Seed with an active profile and an existing effort under that profile
        let seed = r#"profile = "p1"

[profiles.p1]
model_reasoning_effort = "low"
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_overrides(code_home, None, &[(&[CONFIG_KEY_MODEL], "o4-mini")])
            .await
            .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"profile = "p1"

[profiles.p1]
model_reasoning_effort = "low"
model = "o4-mini"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies changing effort only preserves existing model in a profile override.
    #[tokio::test]
    async fn changing_only_effort_preserves_model_in_profile_override() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        // No active profile key; we'll target an explicit override
        let seed = r#"[profiles.team]
model = "gpt-5.1-codex"
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_overrides(
            code_home,
            Some("team"),
            &[(&[CONFIG_KEY_EFFORT], "minimal")],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"[profiles.team]
model = "gpt-5.1-codex"
model_reasoning_effort = "minimal"
"#;
        assert_eq!(contents, expected);
    }

    /// Verifies `persist_non_null_overrides` skips `None` entries and writes only present values at top-level.
    #[tokio::test]
    async fn persist_non_null_skips_none_top_level() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_non_null_overrides(
            code_home,
            None,
            &[
                (&[CONFIG_KEY_MODEL], Some("gpt-5.1-codex")),
                (&[CONFIG_KEY_EFFORT], None),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let table = parsed.as_table().expect("root table");
        assert_eq!(
            table
                .get(CONFIG_KEY_MODEL)
                .and_then(|value| value.as_str()),
            Some("gpt-5.1-codex")
        );
        assert!(table.get(CONFIG_KEY_EFFORT).is_none());
    }

    /// Verifies no-op behavior when all provided overrides are `None` (no file created/modified).
    #[tokio::test]
    async fn persist_non_null_noop_when_all_none() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_non_null_overrides(
            code_home,
            None,
            &[(&["a"], None), (&["profiles", "p", "x"], None)],
        )
        .await
        .expect("persist");

        // Should not create config.toml on a pure no-op
        assert!(!code_home.join(CONFIG_TOML_FILE).exists());
    }

    /// Verifies entries are written under the specified profile and `None` entries are skipped.
    #[tokio::test]
    async fn persist_non_null_respects_profile_override() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_non_null_overrides(
            code_home,
            Some("team"),
            &[
                (&[CONFIG_KEY_MODEL], Some("o3")),
                (&[CONFIG_KEY_EFFORT], None),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
        let table = parsed.as_table().expect("root table");
        let profiles = table
            .get("profiles")
            .and_then(|value| value.as_table())
            .expect("profiles table");
        let team = profiles
            .get("team")
            .and_then(|value| value.as_table())
            .expect("team profile");
        assert_eq!(
            team
                .get(CONFIG_KEY_MODEL)
                .and_then(|value| value.as_str()),
            Some("o3")
        );
        assert!(team.get(CONFIG_KEY_EFFORT).is_none());
    }

    #[tokio::test]
    async fn persist_clear_none_removes_top_level_value() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let seed = r#"model = "gpt-5.1-codex"
model_reasoning_effort = "medium"
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_overrides_and_clear_if_none(
            code_home,
            None,
            &[
                (&[CONFIG_KEY_MODEL], None),
                (&[CONFIG_KEY_EFFORT], Some("high")),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = "model_reasoning_effort = \"high\"\n";
        assert_eq!(contents, expected);
    }

    #[tokio::test]
    async fn persist_clear_none_respects_active_profile() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        let seed = r#"profile = "team"

[profiles.team]
model = "gpt-4"
model_reasoning_effort = "minimal"
"#;
        tokio::fs::write(code_home.join(CONFIG_TOML_FILE), seed)
            .await
            .expect("seed write");

        persist_overrides_and_clear_if_none(
            code_home,
            None,
            &[
                (&[CONFIG_KEY_MODEL], None),
                (&[CONFIG_KEY_EFFORT], Some("high")),
            ],
        )
        .await
        .expect("persist");

        let contents = read_config(code_home).await;
        let expected = r#"profile = "team"

[profiles.team]
model_reasoning_effort = "high"
"#;
        assert_eq!(contents, expected);
    }

    #[tokio::test]
    async fn persist_clear_none_noop_when_file_missing() {
        let tmpdir = tempdir().expect("tmp");
        let code_home = tmpdir.path();

        persist_overrides_and_clear_if_none(code_home, None, &[(&[CONFIG_KEY_MODEL], None)])
            .await
            .expect("persist");

        assert!(!code_home.join(CONFIG_TOML_FILE).exists());
    }

    // Test helper moved to bottom per review guidance.
    async fn read_config(code_home: &Path) -> String {
        let p = code_home.join(CONFIG_TOML_FILE);
        tokio::fs::read_to_string(p).await.unwrap_or_default()
    }
}
