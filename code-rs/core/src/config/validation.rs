use super::ConfigToml;
use std::io::ErrorKind;
use toml::Value as TomlValue;

pub(crate) fn apply_toml_override(root: &mut TomlValue, path: &str, value: TomlValue) {
    use toml::value::Table;

    let segments: Vec<&str> = path.split('.').collect();
    let mut current = root;

    for (idx, segment) in segments.iter().enumerate() {
        let is_last = idx == segments.len() - 1;

        if is_last {
            match current {
                TomlValue::Table(table) => {
                    table.insert(segment.to_string(), value);
                }
                _ => {
                    let mut table = Table::new();
                    table.insert(segment.to_string(), value);
                    *current = TomlValue::Table(table);
                }
            }
            return;
        }

        // Traverse or create intermediate object.
        match current {
            TomlValue::Table(table) => {
                current = table
                    .entry(segment.to_string())
                    .or_insert_with(|| TomlValue::Table(Table::new()));
            }
            _ => {
                *current = TomlValue::Table(Table::new());
                if let TomlValue::Table(tbl) = current {
                    current = tbl
                        .entry(segment.to_string())
                        .or_insert_with(|| TomlValue::Table(Table::new()));
                }
            }
        }
    }
}

fn warn_on_suspicious_cli_overrides(cli_paths: &[String]) {
    if cli_paths.is_empty() {
        return;
    }

    for cli_path in cli_paths {
        if cli_path == "auto_drive.use_chat_model"
            || (cli_path.starts_with("auto_drive.") && cli_path.ends_with(".use_chat_model"))
        {
            eprintln!(
                "Warning: unknown config override `{cli_path}` (ignored). Did you mean `auto_drive_use_chat_model`?"
            );
        }

        if cli_path == "auto_review_enabled" {
            eprintln!(
                "Warning: unknown config override `{cli_path}` (ignored). Did you mean `tui.auto_review_enabled`?"
            );
        }
    }
}

fn warn_on_unknown_cli_overrides(cli_paths: &[String], ignored_paths: &[String]) {
    if cli_paths.is_empty() || ignored_paths.is_empty() {
        return;
    }

    for cli_path in cli_paths {
        let mut ignored = false;
        for ignored_path in ignored_paths {
            if cli_path == ignored_path
                || cli_path.starts_with(&format!("{ignored_path}."))
                || ignored_path.starts_with(&format!("{cli_path}."))
            {
                ignored = true;
                break;
            }
        }
        if !ignored {
            continue;
        }

        // Avoid duplicate warnings for known common confusions.
        if cli_path == "auto_review_enabled"
            || cli_path == "auto_drive.use_chat_model"
            || (cli_path.starts_with("auto_drive.") && cli_path.ends_with(".use_chat_model"))
        {
            continue;
        }

        eprintln!("Warning: unknown config override `{cli_path}` (ignored). See `code exec --help` for valid keys.");
    }
}

pub(crate) fn deserialize_config_toml_with_cli_warnings(
    root_value: &TomlValue,
    cli_paths: &[String],
) -> std::io::Result<ConfigToml> {
    // Note: We intentionally deserialize via `serde_json::Value` so that we can
    // reliably detect unknown fields via `serde_ignored`. Some deserializers
    // (including TOML implementations) may filter unknown struct fields before
    // they reach Serde's ignored-field machinery.
    let deserializer = serde_json::to_value(root_value).map_err(|e| {
        tracing::error!("Failed to convert overridden config for deserialization: {e}");
        std::io::Error::new(ErrorKind::InvalidData, e)
    })?;
    let mut ignored_paths: Vec<String> = Vec::new();

    let cfg: ConfigToml = serde_ignored::deserialize(deserializer, |path| {
        ignored_paths.push(path.to_string());
    })
    .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e))?;

    warn_on_suspicious_cli_overrides(cli_paths);
    warn_on_unknown_cli_overrides(cli_paths, &ignored_paths);

    Ok(cfg)
}

pub(crate) fn upgrade_legacy_model_slugs(cfg: &mut ConfigToml) {
    fn maybe_upgrade(field: &mut Option<String>) {
        if let Some(old) = field.clone() {
            if let Some(new) = upgrade_legacy_model_slug(&old) {
                tracing::info!(
                    target: "code.config",
                    old,
                    new,
                    "upgrading legacy model slug to newer default",
                );
                *field = Some(new);
            }
        }
    }

    maybe_upgrade(&mut cfg.model);
    maybe_upgrade(&mut cfg.review_model);

    for profile in cfg.profiles.values_mut() {
        maybe_upgrade(&mut profile.model);
        maybe_upgrade(&mut profile.review_model);
    }
}

fn upgrade_legacy_model_slug(slug: &str) -> Option<String> {
    if slug.starts_with("gpt-5.2") || slug.starts_with("test-gpt-5.2") {
        return None;
    }

    match slug {
        "gpt-5.1-codex" => return Some("gpt-5.1-codex-max".to_string()),
        "gpt-4.1" => return Some("gpt-4.1-2024-04-09".to_string()),
        "gpt-4.1-mini" => return Some("gpt-4.1-mini-2024-04-09".to_string()),
        "gpt-4.1-nano" => return Some("gpt-4.1-nano-2024-04-09".to_string()),
        _ => {}
    }

    if let Some(rest) = slug.strip_prefix("test-gpt-5-codex") {
        return Some(format!("test-gpt-5.1-codex{rest}"));
    }

    if let Some(rest) = slug.strip_prefix("gpt-5-codex") {
        return Some(format!("gpt-5.1-codex{rest}"));
    }

    // Upgrade Anthropic Opus 4.1 to 4.5
    if slug.eq_ignore_ascii_case("claude-opus-4.1") {
        return Some("claude-opus-4.5".to_string());
    }

    // Upgrade Gemini 2.5 Pro to Gemini 3 Pro (or preview alias)
    if slug.eq_ignore_ascii_case("gemini-2.5-pro") || slug.eq_ignore_ascii_case("gemini-3-pro-preview") {
        return Some("gemini-3-pro".to_string());
    }

    // Upgrade Gemini 2.5 Flash to Gemini 3 Flash
    if slug.eq_ignore_ascii_case("gemini-2.5-flash") {
        return Some("gemini-3-flash".to_string());
    }

    // Keep codex variants on their existing line; upgrades are surfaced via the
    // migration prompt instead of silently rewriting explicit config.
    if slug.starts_with("gpt-5.1-codex") || slug.starts_with("test-gpt-5.1-codex") {
        return None;
    }

    if let Some(rest) = slug.strip_prefix("test-gpt-5.1") {
        return Some(format!("test-gpt-5.2{rest}"));
    }

    if let Some(rest) = slug.strip_prefix("gpt-5.1") {
        return Some(format!("gpt-5.2{rest}"));
    }

    if let Some(rest) = slug.strip_prefix("test-gpt-5") {
        return Some(format!("test-gpt-5.2{rest}"));
    }

    if let Some(rest) = slug.strip_prefix("gpt-5") {
        return Some(format!("gpt-5.2{rest}"));
    }

    None
}
