use crate::config_types::{validation_tool_category, GithubConfig, ValidationCategory, ValidationConfig};
use crate::workflow_validation::maybe_run_actionlint;
use code_apply_patch::{ApplyPatchAction, ApplyPatchFileChange};
use serde_json as json;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use tempfile::TempDir;

#[derive(Debug, Clone)]
pub struct HarnessFinding {
    pub tool: String,
    pub file: Option<PathBuf>,
    pub message: String,
}

#[derive(Debug, Clone)]
struct ResolvedExternalTool {
    executable: PathBuf,
    env_updates: Vec<(OsString, OsString)>,
}

impl ResolvedExternalTool {
    fn apply_to_command(&self, cmd: &mut std::process::Command) {
        for (key, value) in &self.env_updates {
            cmd.env(key, value);
        }
    }
}

#[derive(Debug, Clone)]
struct ProjectFileGroup {
    project_root: PathBuf,
    files: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct TypeScriptProjectGroup {
    project_root: PathBuf,
    config: Option<PathBuf>,
    files: Vec<PathBuf>,
}

/// Run fast validations on the files touched by a patch. Returns `None` when the
/// harness is disabled and no checks were executed.
pub fn run_patch_harness(
    action: &ApplyPatchAction,
    cwd: &Path,
    cfg: &ValidationConfig,
    github: &GithubConfig,
) -> Option<(Vec<HarnessFinding>, Vec<String>)> {
    let functional_enabled = cfg.groups.functional;
    let stylistic_enabled = cfg.groups.stylistic;

    if !functional_enabled && !stylistic_enabled {
        return None;
    }

    let mut findings: Vec<HarnessFinding> = Vec::new();
    let mut ran: Vec<String> = Vec::new();
    let mut record_ran = |name: &str| {
        if !ran.iter().any(|existing| existing == name) {
            ran.push(name.to_string());
        }
    };

    let category_enabled = |category: ValidationCategory| -> bool {
        match category {
            ValidationCategory::Functional => functional_enabled,
            ValidationCategory::Stylistic => stylistic_enabled,
        }
    };

    // 1) Built-in structural parses (JSON/TOML/YAML).
    for (path, change) in action.changes() {
        let (analysis_path, contents_opt) = match change {
            ApplyPatchFileChange::Add { content } => (path.as_path(), Some(content)),
            ApplyPatchFileChange::Update { new_content, move_path, .. } => (
                move_path.as_ref().map_or(path.as_path(), |dest| dest.as_path()),
                Some(new_content),
            ),
            ApplyPatchFileChange::Delete { .. } => (path.as_path(), None),
        };

        if !functional_enabled {
            continue;
        }
        let Some(contents) = contents_opt else { continue };
        match analysis_path.extension().and_then(|e| e.to_str()).unwrap_or("") {
            "json" => {
                record_ran("json-parse");
                if let Err(err) = json::from_str::<json::Value>(contents) {
                    findings.push(HarnessFinding {
                        tool: "json-parse".to_string(),
                        file: Some(analysis_path.to_path_buf()),
                        message: format!("invalid JSON: {err}"),
                    });
                }
            }
            "toml" => {
                record_ran("toml-parse");
                if let Err(err) = toml::from_str::<toml::Value>(contents) {
                    findings.push(HarnessFinding {
                        tool: "toml-parse".to_string(),
                        file: Some(analysis_path.to_path_buf()),
                        message: format!("invalid TOML: {err}"),
                    });
                }
            }
            "yml" | "yaml" => {
                record_ran("yaml-parse");
                if let Err(err) = serde_yaml::from_str::<serde_yaml::Value>(contents) {
                    findings.push(HarnessFinding {
                        tool: "yaml-parse".to_string(),
                        file: Some(analysis_path.to_path_buf()),
                        message: format!("invalid YAML: {err}"),
                    });
                }
            }
            _ => {}
        }
    }

    // 2) Workflow checks (actionlint plugin).
    if functional_enabled {
        if let Some(lines) = maybe_run_actionlint(action, cwd, github) {
            if !lines.is_empty() {
                record_ran("actionlint");
                for line in lines.into_iter().take(24) {
                    findings.push(HarnessFinding { tool: "actionlint".to_string(), file: None, message: line });
                }
            }
        }
    }

    // 3) External tools (shellcheck, markdownlint, etc.).
    let allow = cfg.tools_allowlist.clone().unwrap_or_default();
    let timeout = cfg.timeout_seconds.unwrap_or(6);

    // Stage touched files into a temporary workspace so external tools can run safely.
    let temp = TempDir::new().ok()?;
    let staged_root = temp.path();

    let mut changed_paths: Vec<PathBuf> = Vec::new();
    for (path, change) in action.changes() {
        match change {
            ApplyPatchFileChange::Add { content } => {
                if let Some(rel) = stage_file(staged_root, cwd, path, content) {
                    changed_paths.push(rel);
                }
            }
            ApplyPatchFileChange::Update { new_content, move_path, .. } => {
                let dest_path = move_path.as_ref().unwrap_or(path);
                if let Some(rel) = stage_file(staged_root, cwd, dest_path, new_content) {
                    changed_paths.push(rel);
                }
                if move_path.is_some() && move_path.as_ref().map(|p| p.as_path()) != Some(path.as_path()) {
                    remove_staged_file(staged_root, cwd, path);
                }
            }
            ApplyPatchFileChange::Delete { .. } => {
                remove_staged_file(staged_root, cwd, path);
            }
        }
    }

    changed_paths.sort();
    changed_paths.dedup();

    let is_allowed = |tool: &str| allow.is_empty() || allow.iter().any(|entry| entry == tool);
    let run_tool = |tool: &str, args: &[&str], files: &[PathBuf], group_enabled: bool| -> Vec<HarnessFinding> {
        if !group_enabled || files.is_empty() || !is_allowed(tool) {
            return Vec::new();
        }
        let Some(exe) = which(Path::new(tool)) else { return Vec::new() };
        let mut cmd = std::process::Command::new(exe);
        cmd.current_dir(staged_root);
        cmd.args(args);
        cmd.args(files);
        match run_with_timeout(cmd, timeout) {
            Some(output) => collect_output_lines(&output.stdout, &output.stderr)
                .into_iter()
                .map(|message| HarnessFinding { tool: tool.to_string(), file: None, message })
                .collect(),
            None => vec![HarnessFinding {
                tool: tool.to_string(),
                file: None,
                message: format!("{tool} timed out after {timeout} second(s)"),
            }],
        }
    };
    let run_overlay_tool = |tool: &str, args: &[&str], files: &[PathBuf], group_enabled: bool| -> Vec<HarnessFinding> {
        if !group_enabled || files.is_empty() || !is_allowed(tool) {
            return Vec::new();
        }
        let Some(exe) = which(Path::new(tool)) else { return Vec::new() };
        match WorkspaceOverlay::apply(action) {
            Ok(_overlay) => {
                let mut cmd = std::process::Command::new(exe);
                cmd.current_dir(cwd);
                cmd.args(args);
                cmd.args(files);
                match run_with_timeout(cmd, timeout) {
                    Some(output) => collect_output_lines(&output.stdout, &output.stderr)
                        .into_iter()
                        .map(|message| HarnessFinding { tool: tool.to_string(), file: None, message })
                        .collect(),
                    None => vec![HarnessFinding {
                        tool: tool.to_string(),
                        file: None,
                        message: format!("{tool} timed out after {timeout} second(s)"),
                    }],
                }
            }
            Err(err) => vec![HarnessFinding {
                tool: tool.to_string(),
                file: None,
                message: format!("failed to stage workspace for {tool}: {err}"),
            }],
        }
    };

    let shell_scripts: Vec<PathBuf> = changed_paths
        .iter()
        .filter(|path| is_shell_script(staged_root, path))
        .cloned()
        .collect();
    let shellcheck_group = validation_tool_category("shellcheck");
    let shellcheck_group_enabled = category_enabled(shellcheck_group);
    if shellcheck_group_enabled && cfg.tools.shellcheck.unwrap_or(true) && !shell_scripts.is_empty() {
        if which(Path::new("shellcheck")).is_some() {
            record_ran("shellcheck");
        }
        findings.extend(run_tool("shellcheck", &["-f", "gcc"], &shell_scripts, shellcheck_group_enabled));
    }

    let markdown_files: Vec<PathBuf> = changed_paths
        .iter()
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .cloned()
        .collect();
    let markdownlint_group = validation_tool_category("markdownlint");
    let markdownlint_group_enabled = category_enabled(markdownlint_group);
    if markdownlint_group_enabled && cfg.tools.markdownlint.unwrap_or(true) && !markdown_files.is_empty() {
        if which(Path::new("markdownlint")).is_some() || which(Path::new("markdownlint-cli2")).is_some() {
            record_ran("markdownlint");
        }
        let mut lines = run_overlay_tool("markdownlint", &[], &markdown_files, markdownlint_group_enabled);
        if lines.is_empty() {
            lines = run_overlay_tool("markdownlint-cli2", &[], &markdown_files, markdownlint_group_enabled);
        }
        findings.extend(lines);
    }

    let docker_files: Vec<PathBuf> = changed_paths
        .iter()
        .filter(|path| is_dockerfile(path))
        .cloned()
        .collect();
    let hadolint_group = validation_tool_category("hadolint");
    let hadolint_group_enabled = category_enabled(hadolint_group);
    if hadolint_group_enabled && cfg.tools.hadolint.unwrap_or(true) && !docker_files.is_empty() {
        if which(Path::new("hadolint")).is_some() {
            record_ran("hadolint");
        }
        findings.extend(run_tool("hadolint", &[], &docker_files, hadolint_group_enabled));
    }

    let yaml_files: Vec<PathBuf> = changed_paths
        .iter()
        .filter(|path| matches!(path.extension().and_then(|ext| ext.to_str()), Some("yml" | "yaml")))
        .cloned()
        .collect();
    let yamllint_group = validation_tool_category("yamllint");
    let yamllint_group_enabled = category_enabled(yamllint_group);
    if yamllint_group_enabled && cfg.tools.yamllint.unwrap_or(true) && !yaml_files.is_empty() {
        if which(Path::new("yamllint")).is_some() {
            record_ran("yamllint");
        }
        findings.extend(run_overlay_tool("yamllint", &["-f", "parsable"], &yaml_files, yamllint_group_enabled));
    }

    let rust_files: Vec<PathBuf> = changed_paths
        .iter()
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rs"))
        .cloned()
        .collect();

    let shfmt_group = validation_tool_category("shfmt");
    let shfmt_group_enabled = category_enabled(shfmt_group);
    if shfmt_group_enabled && cfg.tools.shfmt.unwrap_or(true) && !shell_scripts.is_empty() {
        if which(Path::new("shfmt")).is_some() {
            record_ran("shfmt");
        }
        findings.extend(run_tool("shfmt", &["-d"], &shell_scripts, shfmt_group_enabled));
    }

    let prettier_files: Vec<PathBuf> = changed_paths
        .iter()
        .filter(|path| is_prettier_path(path))
        .cloned()
        .collect();
    let prettier_group = validation_tool_category("prettier");
    let prettier_group_enabled = category_enabled(prettier_group);
    if prettier_group_enabled && cfg.tools.prettier.unwrap_or(true) && !prettier_files.is_empty() {
        let prettier_groups = group_files_by_project_root(cwd, &prettier_files, |relative| {
            find_nearest_prettier_root_for_file(cwd, relative)
        });
        if prettier_groups
            .iter()
            .any(|group| resolve_node_tool(&group.project_root, "prettier").is_some())
        {
            record_ran("prettier");
        }
        for group in prettier_groups {
            let Some(tool) = resolve_node_tool(&group.project_root, "prettier") else {
                continue;
            };
            findings.extend(run_overlay_command(
                action,
                &group.project_root,
                "prettier",
                &tool,
                &[OsString::from("--check")],
                &group.files,
                timeout,
            ));
        }
    }

    let ts_files: Vec<PathBuf> = changed_paths
        .iter()
        .filter(|path| matches!(path.extension().and_then(|ext| ext.to_str()), Some("ts" | "tsx")))
        .cloned()
        .collect();
    if functional_enabled && cfg.tools.tsc.unwrap_or(true) && !ts_files.is_empty() && is_allowed("tsc") {
        let ts_projects = group_typescript_files_by_project(cwd, &ts_files);
        if ts_projects
            .iter()
            .any(|group| resolve_node_tool(&group.project_root, "tsc").is_some())
        {
            record_ran("tsc");
        }
        let ts_timeout = timeout.max(20);
        for project in ts_projects {
            let Some(tool) = resolve_node_tool(&project.project_root, "tsc") else {
                continue;
            };
            let mut args = vec![
                OsString::from("--noEmit"),
                OsString::from("--pretty"),
                OsString::from("false"),
            ];
            if let Some(config) = &project.config {
                args.push(OsString::from("--project"));
                args.push(config.clone().into_os_string());
            } else {
                args.extend(project.files.iter().cloned().map(PathBuf::into_os_string));
            }
            findings.extend(run_overlay_command(
                action,
                &project.project_root,
                "tsc",
                &tool,
                &args,
                &[],
                ts_timeout,
            ));
        }
    }

    let eslint_files: Vec<PathBuf> = changed_paths
        .iter()
        .filter(|path| matches!(path.extension().and_then(|ext| ext.to_str()), Some("js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs")))
        .cloned()
        .collect();
    if functional_enabled
        && cfg.tools.eslint.unwrap_or(true)
        && !eslint_files.is_empty()
        && is_allowed("eslint")
        && has_eslint_config(cwd, &eslint_files)
    {
        let eslint_groups = group_files_by_project_root(cwd, &eslint_files, |relative| {
            find_nearest_eslint_root_for_file(cwd, relative)
        });
        if eslint_groups
            .iter()
            .any(|group| resolve_node_tool(&group.project_root, "eslint").is_some())
        {
            record_ran("eslint");
        }
        let lint_timeout = timeout.max(15);
        for group in eslint_groups {
            let relative_files = relativize_paths(&group.project_root, &group.files);
            if !has_eslint_config(&group.project_root, &relative_files) {
                continue;
            }
            let Some(tool) = resolve_node_tool(&group.project_root, "eslint") else {
                continue;
            };
            findings.extend(run_overlay_command(
                action,
                &group.project_root,
                "eslint",
                &tool,
                &[OsString::from("--max-warnings"), OsString::from("0")],
                &group.files,
                lint_timeout,
            ));
        }
    }

    let php_files: Vec<PathBuf> = changed_paths
        .iter()
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("php"))
        .cloned()
        .collect();
    if functional_enabled
        && cfg.tools.phpstan.unwrap_or(true)
        && !php_files.is_empty()
        && is_allowed("phpstan")
        && has_phpstan_config(cwd, &php_files)
    {
        if let Some(exe) = which(Path::new("phpstan")) {
            record_ran("phpstan");
            let phpstan_timeout = timeout.max(20);
            match WorkspaceOverlay::apply(action) {
                Ok(_overlay) => {
                    let mut cmd = std::process::Command::new(&exe);
                    cmd.current_dir(cwd);
                    cmd.args(["analyse", "--error-format=raw", "--no-progress"]);
                    for path in &php_files {
                        cmd.arg(path);
                    }
                    match run_with_timeout(cmd, phpstan_timeout) {
                        Some(output) => {
                            if output.status.map_or(true, |status| !status.success()) {
                                let mut lines = collect_output_lines(&output.stdout, &output.stderr);
                                if lines.is_empty() {
                                    lines.push("phpstan failed (no output)".to_string());
                                }
                                for line in lines.into_iter().take(24) {
                                    findings.push(HarnessFinding { tool: "phpstan".to_string(), file: None, message: line });
                                }
                            }
                        }
                        None => findings.push(HarnessFinding {
                            tool: "phpstan".to_string(),
                            file: None,
                            message: format!("phpstan timed out after {phpstan_timeout} second(s)"),
                        }),
                    }
                }
                Err(err) => findings.push(HarnessFinding {
                    tool: "phpstan".to_string(),
                    file: None,
                    message: format!("failed to stage workspace for phpstan: {err}"),
                }),
            }
        }
    }

    if functional_enabled
        && cfg.tools.psalm.unwrap_or(true)
        && !php_files.is_empty()
        && is_allowed("psalm")
        && has_psalm_config(cwd, &php_files)
    {
        if let Some(exe) = which(Path::new("psalm")) {
            record_ran("psalm");
            let psalm_timeout = timeout.max(20);
            match WorkspaceOverlay::apply(action) {
                Ok(_overlay) => {
                    let mut cmd = std::process::Command::new(&exe);
                    cmd.current_dir(cwd);
                    cmd.args(["--no-progress", "--output-format=compact", "--threads=2"]);
                    for path in &php_files {
                        cmd.arg(path);
                    }
                    match run_with_timeout(cmd, psalm_timeout) {
                        Some(output) => {
                            if output.status.map_or(true, |status| !status.success()) {
                                let mut lines = collect_output_lines(&output.stdout, &output.stderr);
                                if lines.is_empty() {
                                    lines.push("psalm failed (no output)".to_string());
                                }
                                for line in lines.into_iter().take(24) {
                                    findings.push(HarnessFinding { tool: "psalm".to_string(), file: None, message: line });
                                }
                            }
                        }
                        None => findings.push(HarnessFinding {
                            tool: "psalm".to_string(),
                            file: None,
                            message: format!("psalm timed out after {psalm_timeout} second(s)"),
                        }),
                    }
                }
                Err(err) => findings.push(HarnessFinding {
                    tool: "psalm".to_string(),
                    file: None,
                    message: format!("failed to stage workspace for psalm: {err}"),
                }),
            }
        }
    }

    let py_files: Vec<PathBuf> = changed_paths
        .iter()
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("py"))
        .cloned()
        .collect();
    if functional_enabled && cfg.tools.mypy.unwrap_or(true) && !py_files.is_empty() && is_allowed("mypy") {
        if let Some(exe) = which(Path::new("mypy")) {
            record_ran("mypy");
            let mypy_timeout = timeout.max(20);
            match WorkspaceOverlay::apply(action) {
                Ok(_overlay) => {
                    let mut cmd = std::process::Command::new(&exe);
                    cmd.current_dir(cwd);
                    cmd.args(["--no-color-output", "--hide-error-context"]);
                    for path in &py_files {
                        cmd.arg(path);
                    }
                    match run_with_timeout(cmd, mypy_timeout) {
                        Some(output) => {
                            if output.status.map_or(true, |status| !status.success()) {
                                let mut lines = collect_output_lines(&output.stdout, &output.stderr);
                                if lines.is_empty() {
                                    lines.push("mypy failed (no output)".to_string());
                                }
                                for line in lines.into_iter().take(24) {
                                    findings.push(HarnessFinding { tool: "mypy".to_string(), file: None, message: line });
                                }
                            }
                        }
                        None => findings.push(HarnessFinding {
                            tool: "mypy".to_string(),
                            file: None,
                            message: format!("mypy timed out after {mypy_timeout} second(s)"),
                        }),
                    }
                }
                Err(err) => findings.push(HarnessFinding {
                    tool: "mypy".to_string(),
                    file: None,
                    message: format!("failed to stage workspace for mypy: {err}"),
                }),
            }
        }
    }

    if functional_enabled && cfg.tools.pyright.unwrap_or(true) && !py_files.is_empty() && is_allowed("pyright") {
        if let Some(exe) = which(Path::new("pyright")) {
            record_ran("pyright");
            let pyright_timeout = timeout.max(20);
            match WorkspaceOverlay::apply(action) {
                Ok(_overlay) => {
                    let mut cmd = std::process::Command::new(&exe);
                    cmd.current_dir(cwd);
                    cmd.arg("--warnings");
                    for path in &py_files {
                        cmd.arg(path);
                    }
                    match run_with_timeout(cmd, pyright_timeout) {
                        Some(output) => {
                            if output.status.map_or(true, |status| !status.success()) {
                                let mut lines = collect_output_lines(&output.stdout, &output.stderr);
                                if lines.is_empty() {
                                    lines.push("pyright failed (no output)".to_string());
                                }
                                for line in lines.into_iter().take(24) {
                                    findings.push(HarnessFinding { tool: "pyright".to_string(), file: None, message: line });
                                }
                            }
                        }
                        None => findings.push(HarnessFinding {
                            tool: "pyright".to_string(),
                            file: None,
                            message: format!("pyright timed out after {pyright_timeout} second(s)"),
                        }),
                    }
                }
                Err(err) => findings.push(HarnessFinding {
                    tool: "pyright".to_string(),
                    file: None,
                    message: format!("failed to stage workspace for pyright: {err}"),
                }),
            }
        }
    }

    let go_files: Vec<PathBuf> = changed_paths
        .iter()
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("go"))
        .cloned()
        .collect();
    if functional_enabled
        && cfg.tools.golangci_lint.unwrap_or(true)
        && !go_files.is_empty()
        && is_allowed("golangci-lint")
        && has_go_module(cwd)
    {
        if let Some(exe) = which(Path::new("golangci-lint")) {
            record_ran("golangci-lint");
            let lint_timeout = timeout.max(20);
            match WorkspaceOverlay::apply(action) {
                Ok(_overlay) => {
                    let mut cmd = std::process::Command::new(&exe);
                    cmd.current_dir(cwd);
                    cmd.args(["run", "./..."]);
                    match run_with_timeout(cmd, lint_timeout) {
                        Some(output) => {
                            if output.status.map_or(true, |status| !status.success()) {
                                let mut lines = collect_output_lines(&output.stdout, &output.stderr);
                                if lines.is_empty() {
                                    lines.push("golangci-lint failed (no output)".to_string());
                                }
                                for line in lines.into_iter().take(24) {
                                    findings.push(HarnessFinding { tool: "golangci-lint".to_string(), file: None, message: line });
                                }
                            }
                        }
                        None => findings.push(HarnessFinding {
                            tool: "golangci-lint".to_string(),
                            file: None,
                            message: format!("golangci-lint timed out after {lint_timeout} second(s)"),
                        }),
                    }
                }
                Err(err) => findings.push(HarnessFinding {
                    tool: "golangci-lint".to_string(),
                    file: None,
                    message: format!("failed to stage workspace for golangci-lint: {err}"),
                }),
            }
        }
    }

    if functional_enabled && cfg.tools.cargo_check.unwrap_or(true) && !rust_files.is_empty() {
        if which(Path::new("cargo")).is_none() {
            findings.push(HarnessFinding {
                tool: "cargo-check".to_string(),
                file: None,
                message: "cargo executable not found; install the Rust toolchain".to_string(),
            });
        } else {
            match WorkspaceOverlay::apply(action) {
            Ok(overlay) => {
                let manifests = collect_rust_manifests(cwd, &rust_files);
                let manifest_hints = compute_rust_target_hints(cwd, &rust_files);
                let rust_timeout = timeout.max(30);
                for manifest in manifests {
                    let label = manifest
                        .strip_prefix(cwd)
                        .unwrap_or(&manifest)
                        .display()
                        .to_string();
                    let mut cmd = std::process::Command::new("cargo");
                    cmd.current_dir(cwd);
                    cmd.arg("check");
                    cmd.arg("--quiet");
                    let hints = manifest_hints.get(&manifest).copied().unwrap_or_default();
                    // `cargo check` does not support `--no-dev-deps`; compiling dev deps is
                    // avoided by limiting targets instead.
                    if hints.include_tests {
                        cmd.arg("--tests");
                    }
                    if hints.include_benches {
                        cmd.arg("--benches");
                    }
                    if hints.include_examples {
                        cmd.arg("--examples");
                    }
                    cmd.arg("--manifest-path");
                    cmd.arg(manifest.to_string_lossy().to_string());
                    cmd.env("RUSTFLAGS", "-Dwarnings");

                    match run_with_timeout(cmd, rust_timeout) {
                        Some(output) => {
                            record_ran(&format!("cargo-check({label})"));
                            if output.status.map_or(true, |status| !status.success()) {
                                let mut lines = collect_output_lines(&output.stdout, &output.stderr);
                                if lines.is_empty() {
                                    lines.push("cargo check failed (no output)".to_string());
                                }
                                for line in lines.into_iter().take(24) {
                                    findings.push(HarnessFinding {
                                        tool: format!("cargo-check({label})"),
                                        file: None,
                                        message: line,
                                    });
                                }
                            }
                        }
                        None => {
                            findings.push(HarnessFinding {
                                tool: format!("cargo-check({label})"),
                                file: None,
                                message: format!(
                                    "cargo check timed out after {rust_timeout} second(s)"
                                ),
                            });
                        }
                    }
                }
                drop(overlay);
            }
            Err(err) => {
                findings.push(HarnessFinding {
                    tool: "cargo-check".to_string(),
                    file: None,
                    message: format!("failed to stage workspace for cargo check: {err}"),
                });
            }
        }
        }
    }

    if findings.is_empty() && ran.is_empty() {
        None
    } else {
        Some((findings, ran))
    }
}

fn is_shell_script(staged_root: &Path, relative: &Path) -> bool {
    match relative.extension().and_then(|ext| ext.to_str()) {
        Some("sh") => true,
        _ => {
            let staged = staged_root.join(relative);
            std::fs::read(staged)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .map(|contents| contents.starts_with("#!/"))
                .unwrap_or(false)
        }
    }
}

fn is_dockerfile(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return false };
    name.eq_ignore_ascii_case("Dockerfile") || name.starts_with("Dockerfile.")
}

fn is_prettier_path(path: &Path) -> bool {
    let prettier_exts = [
        "js", "jsx", "ts", "tsx", "json", "css", "scss", "less", "html", "yml", "yaml", "md", "mdx",
    ];
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| prettier_exts.contains(&ext))
        .unwrap_or(false)
    {
        return true;
    }
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else { return false };
    matches!(
        name,
        ".prettierrc"
            | ".prettierrc.json"
            | ".prettierrc.json5"
            | ".prettierrc.yml"
            | ".prettierrc.yaml"
            | ".prettierrc.js"
            | ".prettierrc.cjs"
            | ".prettierrc.mjs"
            | ".prettierrc.ts"
            | "prettier.config.js"
            | "prettier.config.cjs"
            | "prettier.config.mjs"
            | "prettier.config.ts"
    )
}

fn which(exe: &Path) -> Option<PathBuf> {
    if exe.is_absolute() {
        return exe.exists().then(|| exe.to_path_buf());
    }
    let name = exe.as_os_str();
    let paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_else(Vec::new);
    for dir in paths {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn resolve_node_tool(project_root: &Path, tool: &str) -> Option<ResolvedExternalTool> {
    if let Some(executable) = find_node_tool_in_dir(project_root, tool) {
        let mut env_updates: Vec<(OsString, OsString)> = Vec::new();
        if let Some(bin_dir) = executable.parent() {
            env_updates.push((OsString::from("PATH"), prepend_path(bin_dir)));
        }
        return Some(ResolvedExternalTool {
            executable,
            env_updates,
        });
    }
    which(Path::new(tool)).map(|executable| ResolvedExternalTool {
        executable,
        env_updates: Vec::new(),
    })
}

fn find_node_tool_in_dir(project_root: &Path, tool: &str) -> Option<PathBuf> {
    for candidate in [
        project_root.join("node_modules").join(".bin").join(tool),
        project_root.join("node_modules").join(".bin").join(format!("{tool}.cmd")),
        project_root.join("node_modules").join(".bin").join(format!("{tool}.ps1")),
        project_root.join("node_modules").join(".bin").join(format!("{tool}.exe")),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn group_files_by_key<F>(cwd: &Path, files: &[PathBuf], mut key_for_file: F) -> Vec<(PathBuf, Vec<PathBuf>)>
where
    F: FnMut(&PathBuf) -> PathBuf,
{
    let mut groups: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    for relative in files {
        let key = key_for_file(relative);
        groups.entry(key).or_default().push(cwd.join(relative));
    }
    groups.into_iter().collect()
}

fn find_nearest_config_for_file(cwd: &Path, relative: &Path, candidates: &[&str]) -> Option<PathBuf> {
    let mut current = cwd.join(relative).parent().map(Path::to_path_buf);
    while let Some(dir) = current {
        for candidate in candidates {
            let candidate_path = dir.join(candidate);
            if candidate_path.exists() {
                return Some(candidate_path);
            }
        }
        if dir == cwd {
            break;
        }
        current = dir.parent().map(Path::to_path_buf);
    }
    None
}

fn find_nearest_package_root_for_file(cwd: &Path, relative: &Path) -> Option<PathBuf> {
    find_nearest_config_for_file(cwd, relative, &["package.json"])
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

fn find_nearest_eslint_root_for_file(cwd: &Path, relative: &Path) -> PathBuf {
    let config_candidates = [
        ".eslintrc",
        ".eslintrc.js",
        ".eslintrc.cjs",
        ".eslintrc.mjs",
        ".eslintrc.json",
        ".eslintrc.yml",
        ".eslintrc.yaml",
        "eslint.config.js",
        "eslint.config.cjs",
        "eslint.config.mjs",
        "eslint.config.ts",
    ];
    if let Some(config) = find_nearest_config_for_file(cwd, relative, &config_candidates) {
        return config.parent().unwrap_or(cwd).to_path_buf();
    }
    if let Some(package_json) = find_nearest_config_for_file(cwd, relative, &["package.json"])
        && package_json_has_key(&package_json, "eslintConfig")
    {
        return package_json.parent().unwrap_or(cwd).to_path_buf();
    }
    find_nearest_package_root_for_file(cwd, relative).unwrap_or_else(|| cwd.to_path_buf())
}

fn find_nearest_prettier_root_for_file(cwd: &Path, relative: &Path) -> PathBuf {
    let config_candidates = [
        ".prettierrc",
        ".prettierrc.json",
        ".prettierrc.json5",
        ".prettierrc.yml",
        ".prettierrc.yaml",
        ".prettierrc.js",
        ".prettierrc.cjs",
        ".prettierrc.mjs",
        ".prettierrc.ts",
        "prettier.config.js",
        "prettier.config.cjs",
        "prettier.config.mjs",
        "prettier.config.ts",
    ];
    if let Some(config) = find_nearest_config_for_file(cwd, relative, &config_candidates) {
        return config.parent().unwrap_or(cwd).to_path_buf();
    }
    find_nearest_package_root_for_file(cwd, relative).unwrap_or_else(|| cwd.to_path_buf())
}

fn group_files_by_project_root<F>(cwd: &Path, files: &[PathBuf], root_for_file: F) -> Vec<ProjectFileGroup>
where
    F: FnMut(&PathBuf) -> PathBuf,
{
    group_files_by_key(cwd, files, root_for_file)
        .into_iter()
        .map(|(project_root, files)| ProjectFileGroup { project_root, files })
        .collect()
}

fn group_typescript_files_by_project(cwd: &Path, files: &[PathBuf]) -> Vec<TypeScriptProjectGroup> {
    let ts_config_candidates = [
        "tsconfig.json",
        "tsconfig.base.json",
        "tsconfig.app.json",
        "tsconfig.build.json",
        "tsconfig.lib.json",
    ];
    let mut groups: BTreeMap<(PathBuf, Option<PathBuf>), Vec<PathBuf>> = BTreeMap::new();
    for relative in files {
        let config = find_nearest_config_for_file(cwd, relative, &ts_config_candidates);
        let project_root = config
            .as_ref()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(|| find_nearest_package_root_for_file(cwd, relative))
            .unwrap_or_else(|| cwd.to_path_buf());
        groups
            .entry((project_root, config))
            .or_default()
            .push(cwd.join(relative));
    }
    groups
        .into_iter()
        .map(|((project_root, config), files)| TypeScriptProjectGroup {
            project_root,
            config,
            files,
        })
        .collect()
}

fn relativize_paths(base: &Path, paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter_map(|path| path.strip_prefix(base).ok().map(Path::to_path_buf))
        .collect()
}

fn prepend_path(bin_dir: &Path) -> OsString {
    let path_entries = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<PathBuf>>())
        .unwrap_or_default();
    let mut combined = Vec::with_capacity(path_entries.len().saturating_add(1));
    combined.push(bin_dir.to_path_buf());
    combined.extend(path_entries);
    std::env::join_paths(combined).unwrap_or_else(|_| bin_dir.as_os_str().to_os_string())
}

fn run_with_timeout(mut cmd: std::process::Command, timeout_secs: u64) -> Option<CommandCapture> {
    use std::process::Stdio;
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let Ok(mut child) = crate::spawn::spawn_std_command_with_retry(&mut cmd) else { return None };

    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().ok().flatten() {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            if let Some(mut out) = child.stdout.take() {
                let _ = std::io::Read::read_to_end(&mut out, &mut stdout);
            }
            if let Some(mut err) = child.stderr.take() {
                let _ = std::io::Read::read_to_end(&mut err, &mut stderr);
            }
            return Some(CommandCapture { status: Some(status), stdout, stderr });
        }

        if start.elapsed().as_secs() >= timeout_secs {
            let _ = child.kill();
            return None;
        }

        std::thread::sleep(std::time::Duration::from_millis(40));
    }
}

fn stage_file(staged_root: &Path, cwd: &Path, path: &Path, contents: &str) -> Option<PathBuf> {
    let relative = path.strip_prefix(cwd).ok()?;
    let dest = staged_root.join(relative);
    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = fs::File::create(&dest) {
        if file.write_all(contents.as_bytes()).is_ok() {
            return Some(relative.to_path_buf());
        }
    }
    None
}

fn remove_staged_file(staged_root: &Path, cwd: &Path, path: &Path) {
    if let Ok(relative) = path.strip_prefix(cwd) {
        let dest = staged_root.join(relative);
        let _ = fs::remove_file(dest);
    }
}

fn collect_output_lines(stdout: &[u8], stderr: &[u8]) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    if !stdout.is_empty() {
        lines.extend(String::from_utf8_lossy(stdout).lines().map(|s| s.to_string()));
    }
    if !stderr.is_empty() {
        lines.extend(String::from_utf8_lossy(stderr).lines().map(|s| s.to_string()));
    }
    lines.retain(|line| !line.trim().is_empty());
    lines
}

fn run_overlay_command(
    action: &ApplyPatchAction,
    run_dir: &Path,
    tool_name: &str,
    tool: &ResolvedExternalTool,
    args: &[OsString],
    files: &[PathBuf],
    timeout_secs: u64,
) -> Vec<HarnessFinding> {
    match WorkspaceOverlay::apply(action) {
        Ok(_overlay) => {
            let mut cmd = std::process::Command::new(&tool.executable);
            cmd.current_dir(run_dir);
            tool.apply_to_command(&mut cmd);
            cmd.args(args);
            cmd.args(files);
            match run_with_timeout(cmd, timeout_secs) {
                Some(output) => {
                    if output.status.is_some_and(|status| status.success()) {
                        return Vec::new();
                    }
                    let mut lines = collect_output_lines(&output.stdout, &output.stderr);
                    if lines.is_empty() {
                        lines.push(format!("{tool_name} failed (no output)"));
                    }
                    lines
                        .into_iter()
                        .take(24)
                        .map(|message| HarnessFinding {
                            tool: tool_name.to_string(),
                            file: None,
                            message,
                        })
                        .collect()
                }
                None => vec![HarnessFinding {
                    tool: tool_name.to_string(),
                    file: None,
                    message: format!("{tool_name} timed out after {timeout_secs} second(s)"),
                }],
            }
        }
        Err(err) => vec![HarnessFinding {
            tool: tool_name.to_string(),
            file: None,
            message: format!("failed to stage workspace for {tool_name}: {err}"),
        }],
    }
}

#[derive(Default, Clone, Copy)]
struct RustTargetHints {
    include_tests: bool,
    include_benches: bool,
    include_examples: bool,
}

impl RustTargetHints {
    fn observe_path(&mut self, path: &Path) {
        if touches_tests(path) {
            self.include_tests = true;
        }
        if touches_benches(path) {
            self.include_benches = true;
        }
        if touches_examples(path) {
            self.include_examples = true;
        }
    }
}

fn compute_rust_target_hints(
    cwd: &Path,
    rust_files: &[PathBuf],
) -> HashMap<PathBuf, RustTargetHints> {
    let mut hints: HashMap<PathBuf, RustTargetHints> = HashMap::new();
    for relative in rust_files {
        if let Some(manifest) = find_manifest(cwd, relative) {
            hints.entry(manifest).or_default().observe_path(relative);
        }
    }
    hints
}

fn touches_tests(path: &Path) -> bool {
    if path.iter().filter_map(|segment| segment.to_str()).any(|segment| {
        matches_segment(segment, &["tests", "test", "integration-tests", "integration_tests"])
    }) {
        return true;
    }
    matches_stem(path, &["test", "tests"], &["_test", "_tests"])
}

fn touches_benches(path: &Path) -> bool {
    if path
        .iter()
        .filter_map(|segment| segment.to_str())
        .any(|segment| matches_segment(segment, &["benches", "bench", "benchmark"]))
    {
        return true;
    }
    matches_stem(path, &["bench", "benches"], &["_bench", "_benches"])
}

fn touches_examples(path: &Path) -> bool {
    if path
        .iter()
        .filter_map(|segment| segment.to_str())
        .any(|segment| matches_segment(segment, &["examples", "example"]))
    {
        return true;
    }
    matches_stem(path, &["example", "examples"], &["_example", "_examples"])
}

fn matches_segment(segment: &str, needles: &[&str]) -> bool {
    needles
        .iter()
        .any(|needle| segment.eq_ignore_ascii_case(needle))
}

fn matches_stem(path: &Path, exact: &[&str], suffixes: &[&str]) -> bool {
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else { return false };
    let stem_lower = stem.to_ascii_lowercase();
    if exact.iter().any(|needle| stem_lower == *needle) {
        return true;
    }
    suffixes.iter().any(|suffix| stem_lower.ends_with(suffix))
}

fn find_nearest_config(cwd: &Path, files: &[PathBuf], candidates: &[&str]) -> Option<PathBuf> {
    for relative in files {
        let mut current = cwd.join(relative).parent().map(Path::to_path_buf);
        while let Some(dir) = current {
            for candidate in candidates {
                let candidate_path = dir.join(candidate);
                if candidate_path.exists() {
                    return Some(candidate_path);
                }
            }
            if dir == cwd {
                break;
            }
            current = dir.parent().map(Path::to_path_buf);
        }
    }
    for candidate in candidates {
        let candidate_path = cwd.join(candidate);
        if candidate_path.exists() {
            return Some(candidate_path);
        }
    }
    None
}

fn package_json_has_key(path: &Path, key: &str) -> bool {
    let Ok(contents) = std::fs::read_to_string(path) else { return false };
    let Ok(value) = json::from_str::<json::Value>(&contents) else { return false };
    value.get(key).is_some()
}

fn composer_requires_package(path: &Path, package: &str) -> bool {
    let Ok(contents) = std::fs::read_to_string(path) else { return false };
    let Ok(value) = json::from_str::<json::Value>(&contents) else { return false };
    for section in ["require", "require-dev"] {
        if value
            .get(section)
            .and_then(|deps| deps.get(package))
            .is_some()
        {
            return true;
        }
    }
    false
}

fn has_eslint_config(cwd: &Path, files: &[PathBuf]) -> bool {
    let config_candidates = [
        ".eslintrc",
        ".eslintrc.js",
        ".eslintrc.cjs",
        ".eslintrc.mjs",
        ".eslintrc.json",
        ".eslintrc.yml",
        ".eslintrc.yaml",
        "eslint.config.js",
        "eslint.config.cjs",
        "eslint.config.mjs",
        "eslint.config.ts",
    ];
    if find_nearest_config(cwd, files, &config_candidates).is_some() {
        return true;
    }
    if let Some(package_json) = find_nearest_config(cwd, files, &["package.json"]) {
        return package_json_has_key(&package_json, "eslintConfig");
    }
    false
}

fn has_phpstan_config(cwd: &Path, files: &[PathBuf]) -> bool {
    if find_nearest_config(cwd, files, &["phpstan.neon", "phpstan.neon.dist"]).is_some() {
        return true;
    }
    if let Some(composer_json) = find_nearest_config(cwd, files, &["composer.json"]) {
        return composer_requires_package(&composer_json, "phpstan/phpstan");
    }
    false
}

fn has_psalm_config(cwd: &Path, files: &[PathBuf]) -> bool {
    let config_candidates = [
        "psalm.xml",
        "psalm.xml.dist",
        ".psalm/config.xml",
        ".psalm/config.xml.dist",
    ];
    if find_nearest_config(cwd, files, &config_candidates).is_some() {
        return true;
    }
    if let Some(composer_json) = find_nearest_config(cwd, files, &["composer.json"]) {
        return composer_requires_package(&composer_json, "vimeo/psalm");
    }
    false
}

fn has_go_module(cwd: &Path) -> bool { cwd.join("go.mod").exists() }

fn collect_rust_manifests(cwd: &Path, rust_files: &[PathBuf]) -> Vec<PathBuf> {
    let mut manifests = BTreeSet::new();
    for relative in rust_files {
        if let Some(manifest) = find_manifest(cwd, relative) {
            manifests.insert(manifest);
        }
    }
    manifests.into_iter().collect()
}

fn find_manifest(cwd: &Path, relative: &Path) -> Option<PathBuf> {
    let absolute = cwd.join(relative);
    let mut current = absolute.parent()?;
    loop {
        let candidate = current.join("Cargo.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        if current == cwd {
            break;
        }
        current = current.parent()?;
    }
    None
}

struct WorkspaceOverlay {
    backups: Vec<(PathBuf, Option<Vec<u8>>)>,
    created_dirs: Vec<PathBuf>,
}

impl WorkspaceOverlay {
    fn apply(action: &ApplyPatchAction) -> std::io::Result<Self> {
        let mut overlay = WorkspaceOverlay { backups: Vec::new(), created_dirs: Vec::new() };
        let mut seen: HashSet<PathBuf> = HashSet::new();

        for (path, change) in action.changes() {
            match change {
                ApplyPatchFileChange::Add { content } => {
                    overlay.write_file(path, content, &mut seen)?;
                }
                ApplyPatchFileChange::Update { new_content, move_path, .. } => {
                    if let Some(dest) = move_path {
                        overlay.write_file(dest, new_content, &mut seen)?;
                        if dest != path {
                            overlay.remove_file(path, &mut seen)?;
                        }
                    } else {
                        overlay.write_file(path, new_content, &mut seen)?;
                    }
                }
                ApplyPatchFileChange::Delete { .. } => {
                    overlay.remove_file(path, &mut seen)?;
                }
            }
        }

        Ok(overlay)
    }

    fn write_file(
        &mut self,
        path: &Path,
        contents: &str,
        seen: &mut HashSet<PathBuf>,
    ) -> std::io::Result<()> {
        self.backup_if_needed(path, seen)?;
        if let Some(parent) = path.parent() {
            self.ensure_dir(parent)?;
        }
        let mut file = fs::File::create(path)?;
        file.write_all(contents.as_bytes())?;
        Ok(())
    }

    fn remove_file(&mut self, path: &Path, seen: &mut HashSet<PathBuf>) -> std::io::Result<()> {
        self.backup_if_needed(path, seen)?;
        if let Err(err) = fs::remove_file(path) {
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(err);
            }
        }
        Ok(())
    }

    fn backup_if_needed(&mut self, path: &Path, seen: &mut HashSet<PathBuf>) -> std::io::Result<()> {
        if !seen.insert(path.to_path_buf()) {
            return Ok(());
        }
        let original = match fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => return Err(err),
        };
        self.backups.push((path.to_path_buf(), original));
        Ok(())
    }

    fn ensure_dir(&mut self, dir: &Path) -> std::io::Result<()> {
        if dir.exists() {
            return Ok(());
        }
        let mut to_create: Vec<PathBuf> = Vec::new();
        let mut current = dir.to_path_buf();
        while !current.exists() {
            to_create.push(current.clone());
            if let Some(parent) = current.parent() {
                current = parent.to_path_buf();
            } else {
                break;
            }
        }
        for path in to_create.iter().rev() {
            fs::create_dir(path)?;
            self.created_dirs.push(path.clone());
        }
        Ok(())
    }
}

impl Drop for WorkspaceOverlay {
    fn drop(&mut self) {
        for (path, original) in self.backups.iter().rev() {
            match original {
                Some(bytes) => {
                    if let Some(parent) = path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    let _ = fs::File::create(path).and_then(|mut file| file.write_all(bytes));
                }
                None => {
                    let _ = fs::remove_file(path);
                }
            }
        }

        for dir in self.created_dirs.iter().rev() {
            let _ = fs::remove_dir(dir);
        }
    }
}

struct CommandCapture {
    status: Option<ExitStatus>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::{is_prettier_path, resolve_node_tool, run_patch_harness, which};
    use crate::config_types::{GithubConfig, ValidationConfig, ValidationGroups, ValidationTools};
    use code_apply_patch::ApplyPatchAction;
    use std::ffi::OsString;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::Path;
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    fn prettier_matches_markdown_and_dot_config_files() {
        assert!(is_prettier_path(Path::new("README.md")));
        assert!(is_prettier_path(Path::new(".prettierrc")));
        assert!(is_prettier_path(Path::new("prettier.config.ts")));
        assert!(is_prettier_path(Path::new(".eslintrc.js")));
        assert!(!is_prettier_path(Path::new("README.txt")));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_node_tool_prefers_valid_symlinked_local_bin() {
        let repo = TempDir::new().expect("tempdir");
        let project_root = repo.path().join("every-code-webui");
        let bin_dir = project_root.join("node_modules/.bin");
        let tool_impls = project_root.join("tool-impls");
        fs::create_dir_all(&bin_dir).expect("create local bin dir");
        fs::create_dir_all(&tool_impls).expect("create tool impl dir");
        write_shell_tool(&tool_impls.join("prettier-local"), "#!/bin/sh\nexit 0\n");
        symlink("../../tool-impls/prettier-local", bin_dir.join("prettier"))
            .expect("create symlinked prettier shim");

        let resolved = resolve_node_tool(&project_root, "prettier").expect("resolve local prettier");

        assert_eq!(resolved.executable, bin_dir.join("prettier"));
        assert!(resolved.executable.is_file(), "symlinked local bin should resolve to a valid file");
    }

    #[cfg(unix)]
    #[test]
    #[serial]
    fn nested_package_validators_prefer_local_node_tooling() {
        let repo = TempDir::new().expect("tempdir");
        let cwd = repo.path();
        let global_bin = cwd.join("global-bin");
        let app_root = cwd.join("every-code-webui");
        let app_bin = app_root.join("node_modules/.bin");
        let tool_impls = app_root.join("tool-impls");
        let src_dir = app_root.join("src");

        fs::create_dir_all(&global_bin).expect("create global bin");
        fs::create_dir_all(&app_bin).expect("create app bin");
        fs::create_dir_all(&tool_impls).expect("create tool impls");
        fs::create_dir_all(&src_dir).expect("create src dir");

        fs::write(app_root.join("package.json"), "{\"name\":\"every-code-webui\",\"type\":\"module\"}\n")
            .expect("write package json");
        fs::write(app_root.join(".prettierrc"), "{}\n").expect("write prettier config");
        fs::write(app_root.join("eslint.config.js"), "export default [];\n")
            .expect("write eslint config");
        fs::write(
            app_root.join("tsconfig.json"),
            "{\"compilerOptions\":{\"rewriteRelativeImportExtensions\":true}}\n",
        )
        .expect("write tsconfig");

        write_shell_tool(
            &global_bin.join("prettier"),
            "#!/bin/sh\necho global prettier used\nexit 1\n",
        );
        write_shell_tool(
            &global_bin.join("eslint"),
            "#!/bin/sh\necho global eslint used\nexit 1\n",
        );
        write_shell_tool(
            &global_bin.join("tsc"),
            "#!/bin/sh\necho unknown compiler option rewriteRelativeImportExtensions\nexit 1\n",
        );

        write_shell_tool(
            &tool_impls.join("prettier-local"),
            "#!/bin/sh\n[ -f package.json ] || { echo missing package.json; exit 1; }\n[ -f .prettierrc ] || { echo missing prettier config; exit 1; }\nexit 0\n",
        );
        write_shell_tool(
            &tool_impls.join("eslint-local"),
            "#!/bin/sh\n[ -f package.json ] || { echo missing package.json; exit 1; }\n[ -f eslint.config.js ] || { echo missing eslint config; exit 1; }\nfor arg in \"$@\"; do\n  [ \"$arg\" = \"unix\" ] && { echo unexpected unix formatter; exit 1; }\ndone\nexit 0\n",
        );
        write_shell_tool(
            &tool_impls.join("tsc-local"),
            &format!(
                "#!/bin/sh\nfor arg in \"$@\"; do\n  [ \"$arg\" = \"{}\" ] && exit 0\ndone\necho missing local tsconfig\nexit 1\n",
                app_root.join("tsconfig.json").display()
            ),
        );

        symlink("../../tool-impls/prettier-local", app_bin.join("prettier"))
            .expect("symlink local prettier");
        symlink("../../tool-impls/eslint-local", app_bin.join("eslint"))
            .expect("symlink local eslint");
        symlink("../../tool-impls/tsc-local", app_bin.join("tsc"))
            .expect("symlink local tsc");

        let path_guard = ScopedEnvVar::set("PATH", Some(global_bin.into_os_string()));

        let prettier_action = ApplyPatchAction::new_add_for_test(
            &src_dir.join("prettier.ts"),
            "export const prettierValue = 1;\n".to_string(),
        );
        let (prettier_findings, prettier_ran) = run_patch_harness(
            &prettier_action,
            cwd,
            &validator_config("prettier", false, true),
            &GithubConfig::default(),
        )
        .expect("prettier harness result");
        assert!(prettier_findings.is_empty(), "unexpected prettier findings: {prettier_findings:?}");
        assert_eq!(prettier_ran, vec!["prettier".to_string()]);

        let eslint_action = ApplyPatchAction::new_add_for_test(
            &src_dir.join("eslint.ts"),
            "export const eslintValue = 2;\n".to_string(),
        );
        let (eslint_findings, eslint_ran) = run_patch_harness(
            &eslint_action,
            cwd,
            &validator_config("eslint", true, false),
            &GithubConfig::default(),
        )
        .expect("eslint harness result");
        assert!(eslint_findings.is_empty(), "unexpected eslint findings: {eslint_findings:?}");
        assert_eq!(eslint_ran, vec!["eslint".to_string()]);

        let tsc_action = ApplyPatchAction::new_add_for_test(
            &src_dir.join("tsc.ts"),
            "export const tscValue = 3;\n".to_string(),
        );
        let (tsc_findings, tsc_ran) = run_patch_harness(
            &tsc_action,
            cwd,
            &validator_config("tsc", true, false),
            &GithubConfig::default(),
        )
        .expect("tsc harness result");
        drop(path_guard);

        assert!(tsc_findings.is_empty(), "unexpected tsc findings: {tsc_findings:?}");
        assert_eq!(tsc_ran, vec!["tsc".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    #[serial]
    fn prettier_runs_from_repo_root_so_repo_config_is_visible() {
        let repo = TempDir::new().expect("tempdir");
        let bin_dir = repo.path().join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        write_shell_tool(
            &bin_dir.join("prettier"),
            "#!/bin/sh\n[ -f .prettierrc ] || { echo missing prettier config; exit 1; }\nexit 0\n",
        );
        fs::write(repo.path().join(".prettierrc"), "{}\n").expect("write config");

        let path_guard = ScopedEnvVar::set("PATH", Some(bin_dir.into_os_string()));
        assert!(which(Path::new("prettier")).is_some(), "fake prettier not found on PATH");
        let action = ApplyPatchAction::new_add_for_test(&repo.path().join("src/index.ts"), "const x = 1;\n".to_string());
        let (findings, ran) = run_patch_harness(&action, repo.path(), &stylistic_config("prettier", false), &GithubConfig::default())
            .expect("prettier harness result");
        drop(path_guard);

        assert!(findings.is_empty(), "unexpected findings: {findings:?}");
        assert_eq!(ran, vec!["prettier".to_string()]);
    }

    fn validator_config(tool: &str, functional: bool, stylistic: bool) -> ValidationConfig {
        ValidationConfig {
            tools_allowlist: Some(vec![tool.to_string()]),
            groups: ValidationGroups {
                functional,
                stylistic,
            },
            ..ValidationConfig::default()
        }
    }

    #[cfg(unix)]
    #[test]
    #[serial]
    fn markdownlint_runs_from_repo_root_so_repo_config_is_visible() {
        let repo = TempDir::new().expect("tempdir");
        let bin_dir = repo.path().join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        write_shell_tool(
            &bin_dir.join("markdownlint"),
            "#!/bin/sh\n[ -f .markdownlint.json ] || { echo missing markdownlint config; exit 1; }\nexit 0\n",
        );
        fs::write(repo.path().join(".markdownlint.json"), "{}\n").expect("write config");

        let path_guard = ScopedEnvVar::set("PATH", Some(bin_dir.into_os_string()));
        assert!(which(Path::new("markdownlint")).is_some(), "fake markdownlint not found on PATH");
        let action = ApplyPatchAction::new_add_for_test(&repo.path().join("README.md"), "# Title\n".to_string());
        let (findings, ran) = run_patch_harness(&action, repo.path(), &stylistic_config("markdownlint", true), &GithubConfig::default())
            .expect("markdownlint harness result");
        drop(path_guard);

        assert!(findings.is_empty(), "unexpected findings: {findings:?}");
        assert_eq!(ran, vec!["markdownlint".to_string()]);
    }

    fn stylistic_config(tool: &str, disable_prettier: bool) -> ValidationConfig {
        ValidationConfig {
            tools_allowlist: Some(vec![tool.to_string()]),
            groups: ValidationGroups { functional: false, stylistic: true },
            tools: ValidationTools {
                prettier: disable_prettier.then_some(false),
                ..ValidationTools::default()
            },
            ..ValidationConfig::default()
        }
    }

    #[cfg(unix)]
    fn write_shell_tool(path: &Path, script: &str) {
        fs::write(path, script).expect("write shell tool");
        let mut permissions = fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("chmod");
    }

    #[cfg(unix)]
    struct ScopedEnvVar {
        key: &'static str,
        previous: Option<OsString>,
    }

    #[cfg(unix)]
    impl ScopedEnvVar {
        fn set(key: &'static str, value: Option<OsString>) -> Self {
            let previous = std::env::var_os(key);
            unsafe {
                match &value {
                    Some(current) => std::env::set_var(key, current),
                    None => std::env::remove_var(key),
                }
            }
            Self { key, previous }
        }
    }

    #[cfg(unix)]
    impl Drop for ScopedEnvVar {
        fn drop(&mut self) {
            unsafe {
                match &self.previous {
                    Some(previous) => std::env::set_var(self.key, previous),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }
}
