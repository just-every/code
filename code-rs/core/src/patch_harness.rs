use crate::config_types::{validation_tool_category, GithubConfig, ValidationCategory, ValidationConfig};
use crate::workflow_validation::maybe_run_actionlint;
use code_apply_patch::{ApplyPatchAction, ApplyPatchFileChange};
use serde_json as json;
use std::collections::{BTreeSet, HashMap, HashSet};
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
        let mut lines = run_tool("markdownlint", &[], &markdown_files, markdownlint_group_enabled);
        if lines.is_empty() {
            lines = run_tool("markdownlint-cli2", &[], &markdown_files, markdownlint_group_enabled);
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
        findings.extend(run_tool("yamllint", &["-f", "parsable"], &yaml_files, yamllint_group_enabled));
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

    let prettier_exts = [
        "js", "jsx", "ts", "tsx", "json", "css", "scss", "less", "html", "yml", "yaml",
    ];
    let prettier_files: Vec<PathBuf> = changed_paths
        .iter()
        .filter(|path| path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| prettier_exts.contains(&ext))
            .unwrap_or(false))
        .cloned()
        .collect();
    let prettier_group = validation_tool_category("prettier");
    let prettier_group_enabled = category_enabled(prettier_group);
    if prettier_group_enabled && cfg.tools.prettier.unwrap_or(true) && !prettier_files.is_empty() {
        if which(Path::new("prettier")).is_some() {
            record_ran("prettier");
        }
        findings.extend(run_tool("prettier", &["--check"], &prettier_files, prettier_group_enabled));
    }

    let ts_files: Vec<PathBuf> = changed_paths
        .iter()
        .filter(|path| matches!(path.extension().and_then(|ext| ext.to_str()), Some("ts" | "tsx")))
        .cloned()
        .collect();
    if functional_enabled && cfg.tools.tsc.unwrap_or(true) && !ts_files.is_empty() && is_allowed("tsc") {
        if let Some(exe) = which(Path::new("tsc")) {
            record_ran("tsc");
            let ts_timeout = timeout.max(20);
            let project = find_nearest_config(cwd, &ts_files, &["tsconfig.json", "tsconfig.base.json", "tsconfig.app.json", "tsconfig.build.json", "tsconfig.lib.json"]);
            match WorkspaceOverlay::apply(action) {
                Ok(_overlay) => {
                    let mut cmd = std::process::Command::new(&exe);
                    cmd.current_dir(cwd);
                    cmd.arg("--noEmit");
                    cmd.arg("--pretty");
                    cmd.arg("false");
                    if let Some(config) = project {
                        cmd.arg("--project");
                        cmd.arg(config);
                    } else {
                        for path in &ts_files {
                            cmd.arg(path);
                        }
                    }
                    match run_with_timeout(cmd, ts_timeout) {
                        Some(output) => {
                            if output.status.map_or(true, |status| !status.success()) {
                                let mut lines = collect_output_lines(&output.stdout, &output.stderr);
                                if lines.is_empty() {
                                    lines.push("tsc failed (no output)".to_string());
                                }
                                for line in lines.into_iter().take(24) {
                                    findings.push(HarnessFinding { tool: "tsc".to_string(), file: None, message: line });
                                }
                            }
                        }
                        None => findings.push(HarnessFinding {
                            tool: "tsc".to_string(),
                            file: None,
                            message: format!("tsc timed out after {ts_timeout} second(s)"),
                        }),
                    }
                }
                Err(err) => findings.push(HarnessFinding {
                    tool: "tsc".to_string(),
                    file: None,
                    message: format!("failed to stage workspace for tsc: {err}"),
                }),
            }
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
        if let Some(exe) = which(Path::new("eslint")) {
            record_ran("eslint");
            let lint_timeout = timeout.max(15);
            match WorkspaceOverlay::apply(action) {
                Ok(_overlay) => {
                    let mut cmd = std::process::Command::new(&exe);
                    cmd.current_dir(cwd);
                    cmd.args(["--max-warnings", "0", "--format", "unix"]);
                    for path in &eslint_files {
                        cmd.arg(path);
                    }
                    match run_with_timeout(cmd, lint_timeout) {
                        Some(output) => {
                            if output.status.map_or(true, |status| !status.success()) {
                                let mut lines = collect_output_lines(&output.stdout, &output.stderr);
                                if lines.is_empty() {
                                    lines.push("eslint failed (no output)".to_string());
                                }
                                for line in lines.into_iter().take(24) {
                                    findings.push(HarnessFinding { tool: "eslint".to_string(), file: None, message: line });
                                }
                            }
                        }
                        None => findings.push(HarnessFinding {
                            tool: "eslint".to_string(),
                            file: None,
                            message: format!("eslint timed out after {lint_timeout} second(s)"),
                        }),
                    }
                }
                Err(err) => findings.push(HarnessFinding {
                    tool: "eslint".to_string(),
                    file: None,
                    message: format!("failed to stage workspace for eslint: {err}"),
                }),
            }
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
