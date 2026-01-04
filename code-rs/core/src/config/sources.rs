use crate::config_loader::{load_config_as_toml_blocking, LoaderOverrides};
use crate::config_types::{
    AutoDriveContinueMode,
    AutoDriveSettings,
    CachedTerminalBackground,
    McpServerConfig,
    McpServerTransportConfig,
    ReasoningEffort,
    ThemeColors,
    ThemeName,
};
use crate::protocol::{ApprovedCommandMatchKind, AskForApproval};
use code_protocol::config_types::SandboxMode;
use dirs::home_dir;
use std::collections::{BTreeMap, HashMap};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::NamedTempFile;
use toml::Value as TomlValue;
use toml_edit::Array as TomlArray;
use toml_edit::ArrayOfTables as TomlArrayOfTables;
use toml_edit::DocumentMut;
use toml_edit::Item as TomlItem;
use toml_edit::Table as TomlTable;
use which::which;

use super::CONFIG_TOML_FILE;

pub fn load_config_as_toml(code_home: &Path) -> std::io::Result<TomlValue> {
    load_config_as_toml_blocking(code_home, LoaderOverrides::default())
}

pub fn load_global_mcp_servers(
    code_home: &Path,
) -> std::io::Result<BTreeMap<String, McpServerConfig>> {
    let root_value = load_config_as_toml(code_home)?;
    let Some(servers_value) = root_value.get("mcp_servers") else {
        return Ok(BTreeMap::new());
    };

    let servers: BTreeMap<String, McpServerConfig> = servers_value
        .clone()
        .try_into()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    for (name, cfg) in &servers {
        if let McpServerTransportConfig::Stdio { command, .. } = &cfg.transport {
            let command_looks_like_path = {
                let path = Path::new(command);
                path.components().count() > 1 || path.is_absolute()
            };
            if !command_looks_like_path && which(command).is_err() {
                let msg = format!(
                    "MCP server `{name}` command `{command}` not found on PATH. If the server is an npm package, set command = \"npx\" and keep the package name in args."
                );
                return Err(std::io::Error::new(ErrorKind::NotFound, msg));
            }
        }
    }

    Ok(servers)
}

pub fn write_global_mcp_servers(
    code_home: &Path,
    servers: &BTreeMap<String, McpServerConfig>,
) -> std::io::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents
            .parse::<DocumentMut>()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e),
    };

    doc.as_table_mut().remove("mcp_servers");

    if !servers.is_empty() {
        let mut table = TomlTable::new();
        table.set_implicit(true);
        doc["mcp_servers"] = TomlItem::Table(table);

        for (name, config) in servers {
            let mut entry = TomlTable::new();
            entry.set_implicit(false);
            match &config.transport {
                McpServerTransportConfig::Stdio { command, args, env } => {
                    entry["command"] = toml_edit::value(command.clone());

                    if !args.is_empty() {
                        let mut args_array = TomlArray::new();
                        for arg in args {
                            args_array.push(arg.clone());
                        }
                        entry["args"] = TomlItem::Value(args_array.into());
                    }

                    if let Some(env) = env
                        && !env.is_empty()
                    {
                        let mut env_table = TomlTable::new();
                        env_table.set_implicit(false);
                        let mut pairs: Vec<_> = env.iter().collect();
                        pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
                        for (key, value) in pairs {
                            env_table.insert(key, toml_edit::value(value.clone()));
                        }
                        entry["env"] = TomlItem::Table(env_table);
                    }
                }
                McpServerTransportConfig::StreamableHttp { url, bearer_token } => {
                    entry["url"] = toml_edit::value(url.clone());
                    if let Some(token) = bearer_token {
                        entry["bearer_token"] = toml_edit::value(token.clone());
                    }
                }
            }

            if let Some(timeout) = config.startup_timeout_sec {
                entry["startup_timeout_sec"] = toml_edit::value(timeout.as_secs_f64());
            }

            if let Some(timeout) = config.tool_timeout_sec {
                entry["tool_timeout_sec"] = toml_edit::value(timeout.as_secs_f64());
            }

            doc["mcp_servers"][name.as_str()] = TomlItem::Table(entry);
        }
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path).map_err(|err| err.error)?;

    Ok(())
}

/// Persist the currently active model selection back to `config.toml` so that it
/// becomes the default for future sessions.
pub async fn persist_model_selection(
    code_home: &Path,
    profile: Option<&str>,
    model: &str,
    effort: Option<ReasoningEffort>,
    preferred_effort: Option<ReasoningEffort>,
) -> anyhow::Result<()> {
    use tokio::fs;

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let existing = match fs::read_to_string(&read_path).await {
        Ok(raw) => Some(raw),
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => return Err(err.into()),
    };

    let mut doc = match existing {
        Some(raw) if raw.trim().is_empty() => DocumentMut::new(),
        Some(raw) => raw
            .parse::<DocumentMut>()
            .map_err(|e| anyhow::anyhow!("failed to parse config.toml: {e}"))?,
        None => DocumentMut::new(),
    };

    {
        let root = doc.as_table_mut();
        if let Some(profile_name) = profile {
            let profiles_item = root
                .entry("profiles")
                .or_insert_with(|| {
                    let mut table = TomlTable::new();
                    table.set_implicit(true);
                    TomlItem::Table(table)
                });

            let profiles_table = profiles_item
                .as_table_mut()
                .expect("profiles table should be a table");

            let profile_item = profiles_table
                .entry(profile_name)
                .or_insert_with(|| {
                    let mut table = TomlTable::new();
                    table.set_implicit(false);
                    TomlItem::Table(table)
                });

            let profile_table = profile_item
                .as_table_mut()
                .expect("profile entry should be a table");

            profile_table["model"] = toml_edit::value(model.to_string());

            if let Some(effort) = effort {
                profile_table["model_reasoning_effort"] =
                    toml_edit::value(effort.to_string());
            } else {
                profile_table.remove("model_reasoning_effort");
            }

            if let Some(preferred) = preferred_effort {
                profile_table["preferred_model_reasoning_effort"] =
                    toml_edit::value(preferred.to_string());
            } else {
                profile_table.remove("preferred_model_reasoning_effort");
            }
        } else {
            root["model"] = toml_edit::value(model.to_string());
            match effort {
                Some(effort) => {
                    root["model_reasoning_effort"] =
                        toml_edit::value(effort.to_string());
                }
                None => {
                    root.remove("model_reasoning_effort");
                }
            }

            match preferred_effort {
                Some(preferred) => {
                    root["preferred_model_reasoning_effort"] =
                        toml_edit::value(preferred.to_string());
                }
                None => {
                    root.remove("preferred_model_reasoning_effort");
                }
            }
        }
    }

    fs::create_dir_all(code_home).await?;
    let tmp_path = config_path.with_extension("tmp");
    fs::write(&tmp_path, doc.to_string()).await?;
    fs::rename(&tmp_path, &config_path).await?;

    Ok(())
}

/// Patch `CODEX_HOME/config.toml` project state.
/// Use with caution.
pub fn set_project_trusted(code_home: &Path, project_path: &Path) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    set_project_trusted_inner(&mut doc, project_path)?;

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

fn set_project_trusted_inner(doc: &mut DocumentMut, project_path: &Path) -> anyhow::Result<()> {
    // Ensure we render a human-friendly structure:
    //
    // [projects]
    // [projects."/path/to/project"]
    // trust_level = "trusted"
    //
    // rather than inline tables like:
    //
    // [projects]
    // "/path/to/project" = { trust_level = "trusted" }
    let project_key = project_path.to_string_lossy().to_string();

    // Ensure top-level `projects` exists as a non-inline, explicit table. If it
    // exists but was previously represented as a non-table (e.g., inline),
    // replace it with an explicit table.
    let mut created_projects_table = false;
    {
        let root = doc.as_table_mut();
        let needs_table = !root.contains_key("projects")
            || root.get("projects").and_then(|i| i.as_table()).is_none();
        if needs_table {
            root.insert("projects", toml_edit::table());
            created_projects_table = true;
        }
    }
    let Some(projects_tbl) = doc["projects"].as_table_mut() else {
        return Err(anyhow::anyhow!(
            "projects table missing after initialization"
        ));
    };

    // If we created the `projects` table ourselves, keep it implicit so we
    // don't render a standalone `[projects]` header.
    if created_projects_table {
        projects_tbl.set_implicit(true);
    }

    // Ensure the per-project entry is its own explicit table. If it exists but
    // is not a table (e.g., an inline table), replace it with an explicit table.
    let needs_proj_table = !projects_tbl.contains_key(project_key.as_str())
        || projects_tbl
            .get(project_key.as_str())
            .and_then(|i| i.as_table())
            .is_none();
    if needs_proj_table {
        projects_tbl.insert(project_key.as_str(), toml_edit::table());
    }
    let Some(proj_tbl) = projects_tbl
        .get_mut(project_key.as_str())
        .and_then(|i| i.as_table_mut())
    else {
        return Err(anyhow::anyhow!("project table missing for {}", project_key));
    };
    proj_tbl.set_implicit(false);
    proj_tbl["trust_level"] = toml_edit::value("trusted");

    Ok(())
}

/// Persist the selected TUI theme into `CODEX_HOME/config.toml` at `[tui.theme].name`.
pub fn set_tui_theme_name(code_home: &Path, theme: ThemeName) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Map enum to kebab-case string used in config
    let theme_str = match theme {
        ThemeName::LightPhoton => "light-photon",
        ThemeName::LightPhotonAnsi16 => "light-photon-ansi16",
        ThemeName::LightPrismRainbow => "light-prism-rainbow",
        ThemeName::LightVividTriad => "light-vivid-triad",
        ThemeName::LightPorcelain => "light-porcelain",
        ThemeName::LightSandbar => "light-sandbar",
        ThemeName::LightGlacier => "light-glacier",
        ThemeName::DarkCarbonNight => "dark-carbon-night",
        ThemeName::DarkCarbonAnsi16 => "dark-carbon-ansi16",
        ThemeName::DarkShinobiDusk => "dark-shinobi-dusk",
        ThemeName::DarkOledBlackPro => "dark-oled-black-pro",
        ThemeName::DarkAmberTerminal => "dark-amber-terminal",
        ThemeName::DarkAuroraFlux => "dark-aurora-flux",
        ThemeName::DarkCharcoalRainbow => "dark-charcoal-rainbow",
        ThemeName::DarkZenGarden => "dark-zen-garden",
        ThemeName::DarkPaperLightPro => "dark-paper-light-pro",
        ThemeName::Custom => "custom",
    };

    // Write `[tui.theme].name = "…"`
    doc["tui"]["theme"]["name"] = toml_edit::value(theme_str);
    // When switching away from the Custom theme, clear any lingering custom
    // overrides so built-in themes render true to spec on next startup.
    if theme != ThemeName::Custom {
        if let Some(tbl) = doc["tui"]["theme"].as_table_mut() {
            tbl.remove("label");
            tbl.remove("colors");
        }
    }

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Record the most recent terminal background autodetect result under `[tui.cached_terminal_background]`.
pub fn set_cached_terminal_background(
    code_home: &Path,
    cache: &CachedTerminalBackground,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let mut tbl = toml_edit::Table::new();
    tbl.set_implicit(false);
    tbl.insert("is_dark", toml_edit::value(cache.is_dark));
    if let Some(term) = &cache.term {
        tbl.insert("term", toml_edit::value(term.as_str()));
    }
    if let Some(term_program) = &cache.term_program {
        tbl.insert("term_program", toml_edit::value(term_program.as_str()));
    }
    if let Some(term_program_version) = &cache.term_program_version {
        tbl.insert(
            "term_program_version",
            toml_edit::value(term_program_version.as_str()),
        );
    }
    if let Some(colorfgbg) = &cache.colorfgbg {
        tbl.insert("colorfgbg", toml_edit::value(colorfgbg.as_str()));
    }
    if let Some(source) = &cache.source {
        tbl.insert("source", toml_edit::value(source.as_str()));
    }
    if let Some(rgb) = &cache.rgb {
        tbl.insert("rgb", toml_edit::value(rgb.as_str()));
    }

    doc["tui"]["cached_terminal_background"] = toml_edit::Item::Table(tbl);

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;
    Ok(())
}

/// Persist the selected spinner into `CODEX_HOME/config.toml` at `[tui.spinner].name`.
pub fn set_tui_spinner_name(code_home: &Path, spinner_name: &str) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Write `[tui.spinner].name = "…"`
    doc["tui"]["spinner"]["name"] = toml_edit::value(spinner_name);

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Save or update a custom spinner under `[tui.spinner.custom.<id>]` with a display `label`,
/// and set it active by writing `[tui.spinner].name = <id>`.
pub fn set_custom_spinner(
    code_home: &Path,
    id: &str,
    label: &str,
    interval: u64,
    frames: &[String],
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };
    // Write custom spinner
    let node = &mut doc["tui"]["spinner"]["custom"][id];
    node["interval"] = toml_edit::value(interval as i64);
    let mut arr = toml_edit::Array::default();
    for s in frames { arr.push(s.as_str()); }
    node["frames"] = toml_edit::value(arr);
    node["label"] = toml_edit::value(label);

    // Set as active
    doc["tui"]["spinner"]["name"] = toml_edit::value(id);

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Save or update a custom theme with a display `label` and color overrides
/// under `[tui.theme]`, and set it active by writing `[tui.theme].name = "custom"`.
pub fn set_custom_theme(
    code_home: &Path,
    label: &str,
    colors: &ThemeColors,
    set_active: bool,
    is_dark: Option<bool>,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Optionally activate custom theme and persist label
    if set_active {
        doc["tui"]["theme"]["name"] = toml_edit::value("custom");
    }
    doc["tui"]["theme"]["label"] = toml_edit::value(label);
    if let Some(d) = is_dark { doc["tui"]["theme"]["is_dark"] = toml_edit::value(d); }

    // Ensure colors table exists and write provided keys
    {
        use toml_edit::Item as It;
        if !doc["tui"]["theme"].is_table() {
            doc["tui"]["theme"] = It::Table(toml_edit::Table::new());
        }
        let theme_tbl = doc["tui"]["theme"].as_table_mut().unwrap();
        if !theme_tbl.contains_key("colors") {
            theme_tbl.insert("colors", It::Table(toml_edit::Table::new()));
        }
    let colors_tbl = theme_tbl["colors"].as_table_mut().unwrap();
        macro_rules! set_opt {
            ($key:ident) => {
                if let Some(ref v) = colors.$key { colors_tbl.insert(stringify!($key), toml_edit::value(v.clone())); }
            };
        }
        set_opt!(primary);
        set_opt!(secondary);
        set_opt!(background);
        set_opt!(foreground);
        set_opt!(border);
        set_opt!(border_focused);
        set_opt!(selection);
        set_opt!(cursor);
        set_opt!(success);
        set_opt!(warning);
        set_opt!(error);
        set_opt!(info);
        set_opt!(text);
        set_opt!(text_dim);
        set_opt!(text_bright);
        set_opt!(keyword);
        set_opt!(string);
        set_opt!(comment);
        set_opt!(function);
        set_opt!(spinner);
        set_opt!(progress);
    }

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Persist the alternate screen preference into `CODEX_HOME/config.toml` at `[tui].alternate_screen`.
pub fn set_tui_alternate_screen(code_home: &Path, enabled: bool) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Write `[tui].alternate_screen = true/false`
    doc["tui"]["alternate_screen"] = toml_edit::value(enabled);

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the TUI notifications preference into `CODEX_HOME/config.toml` at `[tui].notifications`.
pub fn set_tui_notifications(
    code_home: &Path,
    notifications: crate::config_types::Notifications,
) -> anyhow::Result<()> {
    use crate::config_types::Notifications;

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    match notifications {
        Notifications::Enabled(value) => {
            doc["tui"]["notifications"] = toml_edit::value(value);
        }
        Notifications::Custom(values) => {
            let mut array = TomlArray::default();
            for value in values {
                array.push(value);
            }
            doc["tui"]["notifications"] = TomlItem::Value(array.into());
        }
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the review auto-resolve preference into `CODEX_HOME/config.toml` at `[tui].review_auto_resolve`.
pub fn set_tui_review_auto_resolve(code_home: &Path, enabled: bool) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));

    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["tui"]["review_auto_resolve"] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the auto review preference into `CODEX_HOME/config.toml` at `[tui].auto_review_enabled`.
pub fn set_tui_auto_review_enabled(code_home: &Path, enabled: bool) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));

    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["tui"]["auto_review_enabled"] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the review model + reasoning effort into `CODEX_HOME/config.toml`.
pub fn set_review_model(
    code_home: &Path,
    model: &str,
    effort: ReasoningEffort,
    use_chat_model: bool,
) -> anyhow::Result<()> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return Err(anyhow::anyhow!("review model cannot be empty"));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["review_use_chat_model"] = toml_edit::value(use_chat_model);
    if !use_chat_model {
        doc["review_model"] = toml_edit::value(trimmed);
        doc["review_model_reasoning_effort"] =
            toml_edit::value(effort.to_string().to_ascii_lowercase());
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the resolve model + reasoning effort for `/review` auto-resolve flows.
pub fn set_review_resolve_model(
    code_home: &Path,
    model: &str,
    effort: ReasoningEffort,
    use_chat_model: bool,
) -> anyhow::Result<()> {
    let trimmed = model.trim();
    if !use_chat_model && trimmed.is_empty() {
        return Err(anyhow::anyhow!("review resolve model cannot be empty"));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["review_resolve_use_chat_model"] = toml_edit::value(use_chat_model);
    if !use_chat_model {
        doc["review_resolve_model"] = toml_edit::value(trimmed);
        doc["review_resolve_model_reasoning_effort"] =
            toml_edit::value(effort.to_string().to_ascii_lowercase());
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the planning model + reasoning effort into `CODEX_HOME/config.toml`.
pub fn set_planning_model(
    code_home: &Path,
    model: &str,
    effort: ReasoningEffort,
    use_chat_model: bool,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));

    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["planning_use_chat_model"] = toml_edit::value(use_chat_model);
    if !use_chat_model {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return Err(anyhow::anyhow!("planning model cannot be empty"));
        }
        doc["planning_model"] = toml_edit::value(trimmed);
        doc["planning_model_reasoning_effort"] =
            toml_edit::value(effort.to_string().to_ascii_lowercase());
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the Auto Review review model + reasoning effort.
pub fn set_auto_review_model(
    code_home: &Path,
    model: &str,
    effort: ReasoningEffort,
    use_chat_model: bool,
) -> anyhow::Result<()> {
    let trimmed = model.trim();
    if !use_chat_model && trimmed.is_empty() {
        return Err(anyhow::anyhow!("auto review model cannot be empty"));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));

    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["auto_review_use_chat_model"] = toml_edit::value(use_chat_model);
    if !use_chat_model {
        doc["auto_review_model"] = toml_edit::value(trimmed);
        doc["auto_review_model_reasoning_effort"] =
            toml_edit::value(effort.to_string().to_ascii_lowercase());
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the Auto Review resolve model + reasoning effort.
pub fn set_auto_review_resolve_model(
    code_home: &Path,
    model: &str,
    effort: ReasoningEffort,
    use_chat_model: bool,
) -> anyhow::Result<()> {
    let trimmed = model.trim();
    if !use_chat_model && trimmed.is_empty() {
        return Err(anyhow::anyhow!("auto review resolve model cannot be empty"));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));

    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["auto_review_resolve_use_chat_model"] = toml_edit::value(use_chat_model);
    if !use_chat_model {
        doc["auto_review_resolve_model"] = toml_edit::value(trimmed);
        doc["auto_review_resolve_model_reasoning_effort"] =
            toml_edit::value(effort.to_string().to_ascii_lowercase());
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist Auto Drive defaults under `[auto_drive]`.
pub fn set_auto_drive_settings(
    code_home: &Path,
    settings: &AutoDriveSettings,
    use_chat_model: bool,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));

    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    if let Some(tui_tbl) = doc["tui"].as_table_mut() {
        tui_tbl.remove("auto_drive");
    }

    doc["auto_drive_use_chat_model"] = toml_edit::value(use_chat_model);

    doc["auto_drive"]["review_enabled"] = toml_edit::value(settings.review_enabled);
    doc["auto_drive"]["agents_enabled"] = toml_edit::value(settings.agents_enabled);
    doc["auto_drive"]["qa_automation_enabled"] =
        toml_edit::value(settings.qa_automation_enabled);
    doc["auto_drive"]["cross_check_enabled"] =
        toml_edit::value(settings.cross_check_enabled);
    doc["auto_drive"]["observer_enabled"] =
        toml_edit::value(settings.observer_enabled);
    doc["auto_drive"]["coordinator_routing"] =
        toml_edit::value(settings.coordinator_routing);
    doc["auto_drive"]["model"] = toml_edit::value(settings.model.trim());
    doc["auto_drive"]["model_reasoning_effort"] = toml_edit::value(
        settings
            .model_reasoning_effort
            .to_string()
            .to_ascii_lowercase(),
    );
    doc["auto_drive"]["auto_resolve_review_attempts"] =
        toml_edit::value(settings.auto_resolve_review_attempts.get() as i64);
    doc["auto_drive"]["auto_review_followup_attempts"] =
        toml_edit::value(settings.auto_review_followup_attempts.get() as i64);
    doc["auto_drive"]["coordinator_turn_cap"] =
        toml_edit::value(settings.coordinator_turn_cap as i64);

    let mode_str = match settings.continue_mode {
        AutoDriveContinueMode::Immediate => "immediate",
        AutoDriveContinueMode::TenSeconds => "ten-seconds",
        AutoDriveContinueMode::SixtySeconds => "sixty-seconds",
        AutoDriveContinueMode::Manual => "manual",
    };
    doc["auto_drive"]["continue_mode"] = toml_edit::value(mode_str);

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Legacy helper: persist Auto Drive defaults under `[auto_drive]` while
/// accepting the former API surface.
#[deprecated(note = "use set_auto_drive_settings instead")]
pub fn set_tui_auto_drive_settings(
    code_home: &Path,
    settings: &AutoDriveSettings,
    use_chat_model: bool,
) -> anyhow::Result<()> {
    set_auto_drive_settings(code_home, settings, use_chat_model)
}

/// Persist the GitHub workflow check preference under `[github].check_workflows_on_push`.
pub fn set_github_check_on_push(code_home: &Path, enabled: bool) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Write `[github].check_workflows_on_push = <enabled>`
    doc["github"]["check_workflows_on_push"] = toml_edit::value(enabled);

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist `github.actionlint_on_patch = <enabled>`.
pub fn set_github_actionlint_on_patch(
    code_home: &Path,
    enabled: bool,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["github"]["actionlint_on_patch"] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Persist `[validation.groups.<group>] = <enabled>`.
pub fn set_validation_group_enabled(
    code_home: &Path,
    group: &str,
    enabled: bool,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["validation"]["groups"][group] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Persist `[validation.tools.<tool>] = <enabled>`.
pub fn set_validation_tool_enabled(
    code_home: &Path,
    tool: &str,
    enabled: bool,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["validation"]["tools"][tool] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Persist per-project access mode under `[projects."<path>"]` with
/// `approval_policy` and `sandbox_mode`.
pub fn set_project_access_mode(
    code_home: &Path,
    project_path: &Path,
    approval: AskForApproval,
    sandbox_mode: SandboxMode,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Ensure projects table and the per-project table exist
    let project_key = project_path.to_string_lossy().to_string();
    // Ensure `projects` is a table; if key exists but is not a table, replace it.
    let has_projects_table = doc
        .as_table()
        .get("projects")
        .and_then(|i| i.as_table())
        .is_some();
    if !has_projects_table {
        doc["projects"] = TomlItem::Table(toml_edit::Table::new());
    }
    let Some(projects_tbl) = doc["projects"].as_table_mut() else {
        return Err(anyhow::anyhow!("failed to prepare projects table"));
    };
    // Ensure per-project entry exists and is a table; replace if wrong type.
    let needs_proj_table = projects_tbl
        .get(project_key.as_str())
        .and_then(|i| i.as_table())
        .is_none();
    if needs_proj_table {
        projects_tbl.insert(project_key.as_str(), TomlItem::Table(toml_edit::Table::new()));
    }
    let proj_tbl = projects_tbl
        .get_mut(project_key.as_str())
        .and_then(|i| i.as_table_mut())
        .ok_or_else(|| anyhow::anyhow!(format!("failed to create projects.{} table", project_key)))?;

    // Write fields
    proj_tbl.insert(
        "approval_policy",
        TomlItem::Value(toml_edit::Value::from(format!("{}", approval))),
    );
    proj_tbl.insert(
        "sandbox_mode",
        TomlItem::Value(toml_edit::Value::from(format!("{}", sandbox_mode))),
    );

    // Harmonize trust_level with selected access mode:
    // - Full Access (Never + DangerFullAccess): set trust_level = "trusted" so future runs
    //   default to non-interactive behavior when no overrides are present.
    // - Other modes: remove trust_level to avoid conflicting with per-project policy.
    let full_access = matches!(
        (approval, sandbox_mode),
        (AskForApproval::Never, SandboxMode::DangerFullAccess)
    );
    if full_access {
        proj_tbl.insert(
            "trust_level",
            TomlItem::Value(toml_edit::Value::from("trusted")),
        );
    } else {
        proj_tbl.remove("trust_level");
    }

    // Ensure home exists; write atomically
    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;

    Ok(())
}

/// Append a command pattern to `[projects."<path>"].always_allow_commands`.
pub fn add_project_allowed_command(
    code_home: &Path,
    project_path: &Path,
    command: &[String],
    match_kind: ApprovedCommandMatchKind,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let project_key = project_path.to_string_lossy().to_string();
    if doc
        .as_table()
        .get("projects")
        .and_then(|i| i.as_table())
        .is_none()
    {
        doc["projects"] = TomlItem::Table(TomlTable::new());
    }

    let Some(projects_tbl) = doc["projects"].as_table_mut() else {
        return Err(anyhow::anyhow!("failed to prepare projects table"));
    };

    if projects_tbl
        .get(project_key.as_str())
        .and_then(|i| i.as_table())
        .is_none()
    {
        projects_tbl.insert(project_key.as_str(), TomlItem::Table(TomlTable::new()));
    }

    let project_tbl = projects_tbl
        .get_mut(project_key.as_str())
        .and_then(|i| i.as_table_mut())
        .ok_or_else(|| anyhow::anyhow!(format!("failed to create projects.{} table", project_key)))?;

    let mut argv_array = TomlArray::new();
    for arg in command {
        argv_array.push(arg.clone());
    }

    let mut table = TomlTable::new();
    table.insert("argv", TomlItem::Value(toml_edit::Value::Array(argv_array)));
    let match_str = match match_kind {
        ApprovedCommandMatchKind::Exact => "exact",
        ApprovedCommandMatchKind::Prefix => "prefix",
    };
    table.insert(
        "match_kind",
        TomlItem::Value(toml_edit::Value::from(match_str)),
    );

    if let Some(existing) = project_tbl
        .get_mut("always_allow_commands")
        .and_then(|item| item.as_array_of_tables_mut())
    {
        let exists = existing.iter().any(|tbl| {
            let argv_match = tbl
                .get("argv")
                .and_then(|item| item.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(ToString::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let match_kind = tbl
                .get("match_kind")
                .and_then(|item| item.as_str())
                .unwrap_or("exact");
            argv_match == command && match_kind.eq_ignore_ascii_case(match_str)
        });
        if !exists {
            existing.push(table);
        }
    } else {
        let mut arr = TomlArrayOfTables::new();
        arr.push(table);
        project_tbl.insert("always_allow_commands", TomlItem::ArrayOfTables(arr));
    }

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;

    Ok(())
}

/// List MCP servers from `CODEX_HOME/config.toml`.
/// Returns `(enabled, disabled)` lists of `(name, McpServerConfig)`.
pub fn list_mcp_servers(code_home: &Path) -> anyhow::Result<(
    Vec<(String, McpServerConfig)>,
    Vec<(String, McpServerConfig)>,
)> {
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let doc_str = std::fs::read_to_string(&read_path).unwrap_or_default();
    let doc = doc_str.parse::<DocumentMut>().unwrap_or_else(|_| DocumentMut::new());

    fn table_to_list(tbl: &toml_edit::Table) -> Vec<(String, McpServerConfig)> {
        let mut out = Vec::new();
        for (name, item) in tbl.iter() {
            if let Some(t) = item.as_table() {
                let transport = if let Some(command) = t.get("command").and_then(|v| v.as_str()) {
                    let args: Vec<String> = t
                        .get("args")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|i| i.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let env = t
                        .get("env")
                        .and_then(|v| {
                            if let Some(tbl) = v.as_inline_table() {
                                Some(
                                    tbl.iter()
                                        .filter_map(|(k, v)| {
                                            v.as_str().map(|s| (k.to_string(), s.to_string()))
                                        })
                                        .collect::<HashMap<_, _>>(),
                                )
                            } else if let Some(table) = v.as_table() {
                                Some(
                                    table
                                        .iter()
                                        .filter_map(|(k, v)| {
                                            v.as_str().map(|s| (k.to_string(), s.to_string()))
                                        })
                                        .collect::<HashMap<_, _>>(),
                                )
                            } else {
                                None
                            }
                        });

                    McpServerTransportConfig::Stdio {
                        command: command.to_string(),
                        args,
                        env,
                    }
                } else if let Some(url) = t.get("url").and_then(|v| v.as_str()) {
                    let bearer_token = t
                        .get("bearer_token")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    McpServerTransportConfig::StreamableHttp {
                        url: url.to_string(),
                        bearer_token,
                    }
                } else {
                    continue;
                };

                let startup_timeout_sec = t
                    .get("startup_timeout_sec")
                    .and_then(|v| {
                        v.as_float()
                            .map(|f| Duration::try_from_secs_f64(f).ok())
                            .or_else(|| {
                                Some(v.as_integer().map(|i| Duration::from_secs(i as u64)))
                            })
                    })
                    .flatten()
                    .or_else(|| {
                        t.get("startup_timeout_ms")
                            .and_then(|v| v.as_integer())
                            .map(|ms| Duration::from_millis(ms as u64))
                    });

                let tool_timeout_sec = t
                    .get("tool_timeout_sec")
                    .and_then(|v| {
                        v.as_float()
                            .map(|f| Duration::try_from_secs_f64(f).ok())
                            .or_else(|| {
                                Some(v.as_integer().map(|i| Duration::from_secs(i as u64)))
                            })
                    })
                    .flatten();

                out.push((
                    name.to_string(),
                    McpServerConfig {
                        transport,
                        startup_timeout_sec,
                        tool_timeout_sec,
                    },
                ));
            }
        }
        out
    }

    let enabled = doc
        .as_table()
        .get("mcp_servers")
        .and_then(|i| i.as_table())
        .map(table_to_list)
        .unwrap_or_default();

    let disabled = doc
        .as_table()
        .get("mcp_servers_disabled")
        .and_then(|i| i.as_table())
        .map(table_to_list)
        .unwrap_or_default();

    Ok((enabled, disabled))
}

/// Add or update an MCP server under `[mcp_servers.<name>]`. If the same
/// server exists under `mcp_servers_disabled`, it will be removed from there.
pub fn add_mcp_server(
    code_home: &Path,
    name: &str,
    cfg: McpServerConfig,
) -> anyhow::Result<()> {
    // Validate server name for safety and compatibility with MCP tool naming.
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(anyhow::anyhow!(
            "invalid server name '{}': must match ^[a-zA-Z0-9_-]+$",
            name
        ));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Ensure target tables exist
    if !doc.as_table().contains_key("mcp_servers") {
        doc["mcp_servers"] = TomlItem::Table(toml_edit::Table::new());
    }
    let tbl = doc["mcp_servers"].as_table_mut().unwrap();

    let McpServerConfig {
        transport,
        startup_timeout_sec,
        tool_timeout_sec,
    } = cfg;

    // Build table for this server
    let mut server_tbl = toml_edit::Table::new();
    match transport {
        McpServerTransportConfig::Stdio { command, args, env } => {
            server_tbl.insert("command", toml_edit::value(command));
            if !args.is_empty() {
                let mut arr = toml_edit::Array::new();
                for a in args {
                    arr.push(toml_edit::Value::from(a));
                }
                server_tbl.insert("args", TomlItem::Value(toml_edit::Value::Array(arr)));
            }
            if let Some(env) = env {
                let mut it = toml_edit::InlineTable::new();
                for (k, v) in env {
                    it.insert(&k, toml_edit::Value::from(v));
                }
                server_tbl.insert("env", TomlItem::Value(toml_edit::Value::InlineTable(it)));
            }
        }
        McpServerTransportConfig::StreamableHttp { url, bearer_token } => {
            server_tbl.insert("url", toml_edit::value(url));
            if let Some(token) = bearer_token {
                server_tbl.insert("bearer_token", toml_edit::value(token));
            }
        }
    }

    if let Some(duration) = startup_timeout_sec {
        server_tbl.insert("startup_timeout_sec", toml_edit::value(duration.as_secs_f64()));
    }
    if let Some(duration) = tool_timeout_sec {
        server_tbl.insert("tool_timeout_sec", toml_edit::value(duration.as_secs_f64()));
    }

    // Write into enabled table
    tbl.insert(name, TomlItem::Table(server_tbl));

    // Remove from disabled if present
    if let Some(disabled_tbl) = doc["mcp_servers_disabled"].as_table_mut() {
        disabled_tbl.remove(name);
    }

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Enable/disable an MCP server by moving it between `[mcp_servers]` and
/// `[mcp_servers_disabled]`. Returns `true` if a change was made.
pub fn set_mcp_server_enabled(
    code_home: &Path,
    name: &str,
    enabled: bool,
) -> anyhow::Result<bool> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Helper to ensure table exists
    fn ensure_table<'a>(doc: &'a mut DocumentMut, key: &'a str) -> &'a mut toml_edit::Table {
        if !doc.as_table().contains_key(key) {
            doc[key] = TomlItem::Table(toml_edit::Table::new());
        }
        doc[key].as_table_mut().unwrap()
    }

    let mut changed = false;
    if enabled {
        // Move from disabled -> enabled
        let moved = {
            let disabled_tbl = ensure_table(&mut doc, "mcp_servers_disabled");
            disabled_tbl.remove(name)
        };
        if let Some(item) = moved {
            let enabled_tbl = ensure_table(&mut doc, "mcp_servers");
            enabled_tbl.insert(name, item);
            changed = true;
        }
    } else {
        // Move from enabled -> disabled
        let moved = {
            let enabled_tbl = ensure_table(&mut doc, "mcp_servers");
            enabled_tbl.remove(name)
        };
        if let Some(item) = moved {
            let disabled_tbl = ensure_table(&mut doc, "mcp_servers_disabled");
            disabled_tbl.insert(name, item);
            changed = true;
        }
    }

    if changed {
        std::fs::create_dir_all(code_home)?;
        let tmp = NamedTempFile::new_in(code_home)?;
        std::fs::write(tmp.path(), doc.to_string())?;
        tmp.persist(config_path)?;
    }

    Ok(changed)
}

/// Apply a single dotted-path override onto a TOML value.

fn env_path(var: &str) -> std::io::Result<Option<PathBuf>> {
    match std::env::var(var) {
        Ok(val) if !val.trim().is_empty() => {
            let canonical = PathBuf::from(val).canonicalize()?;
            Ok(Some(canonical))
        }
        _ => Ok(None),
    }
}

fn env_overrides_present() -> bool {
    matches!(std::env::var("CODE_HOME"), Ok(ref v) if !v.trim().is_empty())
        || matches!(std::env::var("CODEX_HOME"), Ok(ref v) if !v.trim().is_empty())
}

fn default_code_home_dir() -> Option<PathBuf> {
    let mut path = home_dir()?;
    path.push(".code");
    Some(path)
}

fn compute_legacy_code_home_dir() -> Option<PathBuf> {
    if env_overrides_present() {
        return None;
    }
    let Some(home) = home_dir() else {
        return None;
    };
    let candidate = home.join(".codex");
    if path_exists(&candidate) {
        Some(candidate)
    } else {
        None
    }
}

fn legacy_code_home_dir() -> Option<PathBuf> {
    #[cfg(test)]
    {
        return compute_legacy_code_home_dir();
    }

    #[cfg(not(test))]
    {
        static LEGACY: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
        LEGACY
            .get_or_init(compute_legacy_code_home_dir)
            .clone()
    }
}

fn path_exists(path: &Path) -> bool {
    std::fs::metadata(path).is_ok()
}

/// Resolve the filesystem path used for *reading* Codex state that may live in
/// a legacy `~/.codex` directory. Writes should continue targeting `code_home`.
pub fn resolve_code_path_for_read(code_home: &Path, relative: &Path) -> PathBuf {
    let default_path = code_home.join(relative);

    if env_overrides_present() {
        return default_path;
    }

    if path_exists(&default_path) {
        return default_path;
    }

    if let Some(default_home) = default_code_home_dir() {
        if default_home != code_home {
            return default_path;
        }
    }

    if let Some(legacy) = legacy_code_home_dir() {
        let candidate = legacy.join(relative);
        if path_exists(&candidate) {
            return candidate;
        }
    }

    default_path
}

/// Returns the path to the Code/Codex configuration directory, which can be
/// specified by the `CODE_HOME` or `CODEX_HOME` environment variables. If not set,
/// defaults to `~/.code` for the fork.
///
/// - If `CODE_HOME` or `CODEX_HOME` is set, the value will be canonicalized and this
///   function will Err if the path does not exist.
/// - If neither is set, this function does not verify that the directory exists.
pub fn find_code_home() -> std::io::Result<PathBuf> {
    if let Some(path) = env_path("CODE_HOME")? {
        return Ok(path);
    }

    if let Some(path) = env_path("CODEX_HOME")? {
        return Ok(path);
    }

    let home = home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not find home directory",
        )
    })?;

    let mut write_path = home;
    write_path.push(".code");
    Ok(write_path)
}

pub(crate) fn load_instructions(code_dir: Option<&Path>) -> Option<String> {
    let code_home = code_dir?;
    let read_path = resolve_code_path_for_read(code_home, Path::new("AGENTS.md"));

    let contents = match std::fs::read_to_string(&read_path) {
        Ok(s) => s,
        Err(_) => {
            if env_overrides_present() {
                return None;
            }
            let Some(legacy_home) = legacy_code_home_dir() else {
                return None;
            };
            let legacy_path = legacy_home.join("AGENTS.md");
            match std::fs::read_to_string(&legacy_path) {
                Ok(s) => s,
                Err(_) => return None,
            }
        }
    };

    let trimmed = contents.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn read_override_file(
    path: Option<&PathBuf>,
    cwd: &Path,
    description: &str,
) -> std::io::Result<Option<String>> {
    let p = match path.as_ref() {
        None => return Ok(None),
        Some(p) => p,
    };

    // Resolve relative paths against the provided cwd to make CLI
    // overrides consistent regardless of where the process was launched
    // from.
    let full_path = if p.is_relative() {
        cwd.join(p)
    } else {
        p.to_path_buf()
    };

    let contents = std::fs::read_to_string(&full_path).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!("failed to read {description} {}: {e}", full_path.display()),
        )
    })?;

    let s = contents.trim().to_string();
    if s.is_empty() {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{description} is empty: {}", full_path.display()),
        ))
    } else {
        Ok(Some(s))
    }
}

pub(crate) fn get_base_instructions(
    path: Option<&PathBuf>,
    cwd: &Path,
) -> std::io::Result<Option<String>> {
    read_override_file(path, cwd, "experimental instructions file")
}

pub(crate) fn get_compact_prompt_override(
    path: Option<&PathBuf>,
    cwd: &Path,
) -> std::io::Result<Option<String>> {
    read_override_file(path, cwd, "compact prompt override file")
}
