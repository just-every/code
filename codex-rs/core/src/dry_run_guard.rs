use std::collections::HashMap;
use std::path::Path;

use shlex::split as shlex_split;

use crate::parse_command::{ParsedCommand, parse_command_impl};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DryRunGuardKey {
    CargoFmt,
    CargoFix,
    CargoClippyFix,
    Rustfmt,
    PackageLint(PackageManager),
    PackageFormat(PackageManager),
    EslintFix,
    PrettierWrite,
    Black,
    RuffFormat,
    Isort,
    Gofmt,
    Rustywind,
    DartFormat,
    SwiftFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
}

impl PackageManager {
    fn label(&self) -> &'static str {
        match self {
            PackageManager::Npm => "npm",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DryRunDisposition {
    DryRun,
    Mutating,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DryRunAnalysis {
    pub key: DryRunGuardKey,
    pub disposition: DryRunDisposition,
    tokens: Vec<String>,
    display_name: String,
    custom_suggested_dry_run: Option<String>,
}

#[derive(Clone, Default)]
pub struct DryRunGuardState {
    command_counter: u64,
    last_dry_run: HashMap<DryRunGuardKey, u64>,
    last_mutating: HashMap<DryRunGuardKey, u64>,
}

impl DryRunGuardState {
    pub fn has_recent_dry_run(&self, key: DryRunGuardKey) -> bool {
        match self.last_dry_run.get(&key).copied() {
            Some(dry_seq) => {
                let last_mut = self.last_mutating.get(&key).copied().unwrap_or(0);
                dry_seq > last_mut
            }
            None => false,
        }
    }

    pub fn note_execution(&mut self, analysis: &DryRunAnalysis) {
        self.command_counter = self.command_counter.saturating_add(1);
        let seq = self.command_counter;
        match analysis.disposition {
            DryRunDisposition::DryRun => {
                self.last_dry_run.insert(analysis.key, seq);
            }
            DryRunDisposition::Mutating => {
                self.last_mutating.insert(analysis.key, seq);
            }
        }
    }
}

impl DryRunAnalysis {
    pub fn suggested_dry_run(&self) -> Option<String> {
        if let Some(custom) = &self.custom_suggested_dry_run {
            return Some(custom.clone());
        }

        match self.key {
            DryRunGuardKey::CargoFmt => self.suggest_cargo_fmt_dry_run(),
            DryRunGuardKey::CargoFix => self.suggest_cargo_fix_dry_run(),
            DryRunGuardKey::CargoClippyFix => self.suggest_cargo_clippy_dry_run(),
            DryRunGuardKey::Rustfmt => self.suggest_rustfmt_dry_run(),
            DryRunGuardKey::PackageLint(manager) => {
                self.suggest_package_script_dry_run(manager, "--dry-run", Some("--check"))
            }
            DryRunGuardKey::PackageFormat(manager) => {
                self.suggest_package_script_dry_run(manager, "--check", Some("--dry-run"))
            }
            DryRunGuardKey::EslintFix => self.suggest_eslint_fix_dry_run(),
            DryRunGuardKey::PrettierWrite => self.suggest_prettier_write_dry_run(),
            DryRunGuardKey::Black => self.suggest_append_flag("--check"),
            DryRunGuardKey::RuffFormat => self.suggest_append_flag("--check"),
            DryRunGuardKey::Isort => self.suggest_append_flag("--check-only"),
            DryRunGuardKey::Gofmt => self.suggest_gofmt_dry_run(),
            DryRunGuardKey::Rustywind => self.suggest_append_flag("--check"),
            DryRunGuardKey::DartFormat => self.suggest_append_flag("--dry-run"),
            DryRunGuardKey::SwiftFormat => self.suggest_swiftformat_dry_run(),
        }
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    fn suggest_cargo_fmt_dry_run(&self) -> Option<String> {
        let mut out = self.tokens.clone();
        if let Some(pos) = out.iter().position(|t| t == "--") {
            if !out
                .iter()
                .skip(pos + 1)
                .any(|t| equal_ignore_case(t, "--check"))
            {
                out.insert(pos + 1, "--check".to_string());
            }
        } else {
            out.push("--".to_string());
            out.push("--check".to_string());
        }
        Some(out.join(" "))
    }

    fn suggest_cargo_fix_dry_run(&self) -> Option<String> {
        let mut out = self.tokens.clone();
        if !out.iter().any(|t| equal_ignore_case(t, "--dry-run")) {
            let insert_pos = out.iter().position(|t| t == "--").unwrap_or(out.len());
            out.insert(insert_pos, "--dry-run".to_string());
        }
        Some(out.join(" "))
    }

    fn suggest_cargo_clippy_dry_run(&self) -> Option<String> {
        let mut removed = false;
        let filtered: Vec<String> = self
            .tokens
            .iter()
            .filter(|t| {
                if !removed && equal_ignore_case(t, "--fix") {
                    removed = true;
                    return false;
                }
                true
            })
            .cloned()
            .collect();
        Some(filtered.join(" "))
    }

    fn suggest_rustfmt_dry_run(&self) -> Option<String> {
        let mut out = self.tokens.clone();
        if !out.iter().any(|t| equal_ignore_case(t, "--check")) {
            out.push("--check".to_string());
        }
        Some(out.join(" "))
    }

    fn suggest_package_script_dry_run(
        &self,
        _manager: PackageManager,
        preferred_flag: &str,
        fallback_flag: Option<&str>,
    ) -> Option<String> {
        let mut out = self.tokens.clone();
        if !out.iter().any(|t| {
            equal_ignore_case(t, preferred_flag)
                || fallback_flag.is_some_and(|flag| equal_ignore_case(t, flag))
        }) {
            out.push("--".to_string());
            out.push(preferred_flag.to_string());
        }
        Some(out.join(" "))
    }

    fn suggest_eslint_fix_dry_run(&self) -> Option<String> {
        if self
            .tokens
            .iter()
            .any(|t| equal_ignore_case(t, "--fix-dry-run") || equal_ignore_case(t, "--dry-run"))
        {
            return Some(self.tokens.join(" "));
        }
        let mut out = self.tokens.clone();
        let insert_pos = out.iter().position(|t| t == "--").unwrap_or(out.len());
        out.insert(insert_pos, "--fix-dry-run".to_string());
        Some(out.join(" "))
    }

    fn suggest_prettier_write_dry_run(&self) -> Option<String> {
        if self
            .tokens
            .iter()
            .any(|t| equal_ignore_case(t, "--check") || equal_ignore_case(t, "--list-different"))
        {
            return Some(self.tokens.join(" "));
        }

        let mut out = self.tokens.clone();
        for token in out.iter_mut() {
            if equal_ignore_case(token.as_str(), "--write") || token.as_str() == "-w" {
                *token = "--check".to_string();
                return Some(out.join(" "));
            }
        }
        out.push("--check".to_string());
        Some(out.join(" "))
    }

    fn suggest_append_flag(&self, flag: &str) -> Option<String> {
        if self.tokens.iter().any(|t| equal_ignore_case(t, flag)) {
            return Some(self.tokens.join(" "));
        }
        let mut out = self.tokens.clone();
        out.push(flag.to_string());
        Some(out.join(" "))
    }

    fn suggest_gofmt_dry_run(&self) -> Option<String> {
        let mut out = Vec::with_capacity(self.tokens.len());
        let mut replaced = false;
        for token in &self.tokens {
            if !replaced && (token == "-w" || token.starts_with("-w=")) {
                out.push("-d".to_string());
                replaced = true;
                continue;
            }
            out.push(token.clone());
        }
        if !replaced {
            out.push("-d".to_string());
        }
        Some(out.join(" "))
    }

    fn suggest_swiftformat_dry_run(&self) -> Option<String> {
        if self
            .tokens
            .iter()
            .any(|t| equal_ignore_case(t, "--lint") || equal_ignore_case(t, "--dryrun"))
        {
            return Some(self.tokens.join(" "));
        }
        let mut out = self.tokens.clone();
        out.push("--lint".to_string());
        Some(out.join(" "))
    }
}

fn first_command_tokens(argv: &[String]) -> Option<Vec<String>> {
    let parsed = parse_command_impl(argv);
    let first = parsed.into_iter().next()?;
    let cmd = match first {
        ParsedCommand::Read { cmd, .. }
        | ParsedCommand::ListFiles { cmd, .. }
        | ParsedCommand::Search { cmd, .. }
        | ParsedCommand::ReadCommand { cmd }
        | ParsedCommand::Unknown { cmd } => cmd,
    };
    match shlex_split(&cmd) {
        Some(tokens) => Some(tokens),
        None => Some(cmd.split_whitespace().map(|s| s.to_string()).collect()),
    }
}

fn strip_wrappers(tokens: &[String]) -> Vec<String> {
    let mut idx = 0usize;
    while idx < tokens.len() {
        let token = tokens[idx].as_str();
        if is_env_assignment(token) {
            idx += 1;
            continue;
        }
        if is_wrapper(token) {
            idx += 1;
            while idx < tokens.len() && tokens[idx].starts_with('-') {
                idx += 1;
            }
            continue;
        }
        break;
    }
    tokens[idx..].to_vec()
}

fn is_env_assignment(token: &str) -> bool {
    token.contains('=') && !token.starts_with('-') && !token.starts_with('=')
}

fn is_wrapper(token: &str) -> bool {
    matches!(
        token,
        "env" | "sudo" | "command" | "time" | "nohup" | "nice"
    )
}

fn command_basename(token: &str) -> String {
    let trimmed = token.trim_matches('"').trim_matches('\'');
    Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase()
}

fn equal_ignore_case<S: AsRef<str>, T: AsRef<str>>(left: S, right: T) -> bool {
    left.as_ref().eq_ignore_ascii_case(right.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_vec(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn analyze(raw: &str) -> DryRunAnalysis {
        analyze_command(&to_vec(&["bash", "-lc", raw])).expect("analysis")
    }

    #[test]
    fn detects_cargo_fmt_mutating() {
        let analysis = analyze("cargo fmt");
        assert_eq!(analysis.key, DryRunGuardKey::CargoFmt);
        assert_eq!(analysis.disposition, DryRunDisposition::Mutating);
    }

    #[test]
    fn detects_cargo_fmt_check() {
        let analysis = analyze("cargo fmt -- --check");
        assert_eq!(analysis.disposition, DryRunDisposition::DryRun);
    }

    #[test]
    fn detects_cargo_fix_mutating() {
        let analysis = analyze("cargo fix");
        assert_eq!(analysis.key, DryRunGuardKey::CargoFix);
        assert_eq!(analysis.disposition, DryRunDisposition::Mutating);
    }

    #[test]
    fn detects_cargo_fix_dry_run() {
        let analysis = analyze("cargo fix --dry-run");
        assert_eq!(analysis.disposition, DryRunDisposition::DryRun);
    }

    #[test]
    fn detects_cargo_clippy_fix() {
        let analysis = analyze("cargo clippy --fix");
        assert_eq!(analysis.key, DryRunGuardKey::CargoClippyFix);
        assert_eq!(analysis.disposition, DryRunDisposition::Mutating);
    }

    #[test]
    fn detects_rustfmt_check() {
        let analysis = analyze("rustfmt --check src/lib.rs");
        assert_eq!(analysis.key, DryRunGuardKey::Rustfmt);
        assert_eq!(analysis.disposition, DryRunDisposition::DryRun);
    }

    #[test]
    fn detects_npm_lint_without_dry_run() {
        let analysis = analyze("npm run lint");
        assert_eq!(
            analysis.key,
            DryRunGuardKey::PackageLint(PackageManager::Npm)
        );
        assert_eq!(analysis.disposition, DryRunDisposition::Mutating);
    }

    #[test]
    fn detects_npm_lint_with_dry_run() {
        let analysis = analyze("npm run lint -- --dry-run");
        assert_eq!(analysis.disposition, DryRunDisposition::DryRun);
    }

    #[test]
    fn detects_package_format_script() {
        let analysis = analyze("yarn format");
        assert_eq!(
            analysis.key,
            DryRunGuardKey::PackageFormat(PackageManager::Yarn)
        );
        assert_eq!(analysis.disposition, DryRunDisposition::Mutating);
    }

    #[test]
    fn detects_eslint_fix() {
        let analysis = analyze("npx eslint --fix src");
        assert_eq!(analysis.key, DryRunGuardKey::EslintFix);
        assert_eq!(analysis.disposition, DryRunDisposition::Mutating);
    }

    #[test]
    fn detects_prettier_write() {
        let analysis = analyze("pnpm exec prettier --write src/index.ts");
        assert_eq!(analysis.key, DryRunGuardKey::PrettierWrite);
        assert_eq!(analysis.disposition, DryRunDisposition::Mutating);
    }

    #[test]
    fn detects_black_check() {
        let analysis = analyze("black --check app.py");
        assert_eq!(analysis.key, DryRunGuardKey::Black);
        assert_eq!(analysis.disposition, DryRunDisposition::DryRun);
    }

    #[test]
    fn detects_black_mutating() {
        let analysis = analyze("black app.py");
        assert_eq!(analysis.disposition, DryRunDisposition::Mutating);
    }

    #[test]
    fn detects_ruff_format() {
        let analysis = analyze("ruff format src");
        assert_eq!(analysis.key, DryRunGuardKey::RuffFormat);
        assert_eq!(analysis.disposition, DryRunDisposition::Mutating);
    }

    #[test]
    fn detects_gofmt_w() {
        let analysis = analyze("gofmt -w main.go");
        assert_eq!(analysis.key, DryRunGuardKey::Gofmt);
        assert_eq!(analysis.disposition, DryRunDisposition::Mutating);
    }

    #[test]
    fn detects_dart_format() {
        let analysis = analyze("dart format lib");
        assert_eq!(analysis.key, DryRunGuardKey::DartFormat);
        assert_eq!(analysis.disposition, DryRunDisposition::Mutating);
    }

    #[test]
    fn detects_swiftformat_lint() {
        let analysis = analyze("swiftformat --lint Sources");
        assert_eq!(analysis.key, DryRunGuardKey::SwiftFormat);
        assert_eq!(analysis.disposition, DryRunDisposition::DryRun);
    }

    #[test]
    fn dry_run_state_requires_new_check() {
        let mut state = DryRunGuardState::default();
        let mut analysis = analyze("cargo fmt -- --check");
        state.note_execution(&analysis);
        assert!(state.has_recent_dry_run(DryRunGuardKey::CargoFmt));

        analysis = analyze("cargo fmt");
        state.note_execution(&analysis);
        assert!(!state.has_recent_dry_run(DryRunGuardKey::CargoFmt));
    }
}
pub fn analyze_command(command: &[String]) -> Option<DryRunAnalysis> {
    let first_tokens = first_command_tokens(command)?;
    if first_tokens.is_empty() {
        return None;
    }

    let tokens = strip_wrappers(&first_tokens);
    if tokens.is_empty() {
        return None;
    }

    let analysis = analyze_tokens(&tokens)?;
    Some(DryRunAnalysis {
        key: analysis.key,
        disposition: analysis.disposition,
        tokens,
        display_name: analysis.display_name,
        custom_suggested_dry_run: analysis.suggested_dry_run,
    })
}

struct InternalAnalysis {
    key: DryRunGuardKey,
    disposition: DryRunDisposition,
    display_name: String,
    suggested_dry_run: Option<String>,
}

fn analyze_tokens(tokens: &[String]) -> Option<InternalAnalysis> {
    analyze_cargo(tokens)
        .or_else(|| analyze_rustfmt(tokens))
        .or_else(|| analyze_package_manager(tokens, PackageManager::Npm))
        .or_else(|| analyze_package_manager(tokens, PackageManager::Pnpm))
        .or_else(|| analyze_package_manager(tokens, PackageManager::Yarn))
        .or_else(|| analyze_direct_formatter(tokens))
}

fn analyze_cargo(tokens: &[String]) -> Option<InternalAnalysis> {
    if tokens.is_empty() || !equal_ignore_case(&tokens[0], "cargo") {
        return None;
    }

    let mut idx = 1usize;
    while idx < tokens.len() && tokens[idx].starts_with('+') {
        idx += 1;
    }
    if idx >= tokens.len() {
        return None;
    }

    let sub = command_basename(&tokens[idx]);
    let args = if idx + 1 < tokens.len() {
        &tokens[idx + 1..]
    } else {
        &[]
    };

    match sub.as_str() {
        "fmt" => {
            let is_dry = slice_contains_flag(args, "--check");
            Some(InternalAnalysis {
                key: DryRunGuardKey::CargoFmt,
                disposition: if is_dry {
                    DryRunDisposition::DryRun
                } else {
                    DryRunDisposition::Mutating
                },
                display_name: "cargo fmt".to_string(),
                suggested_dry_run: None,
            })
        }
        "fix" => {
            let is_dry = slice_contains_flag(args, "--dry-run");
            Some(InternalAnalysis {
                key: DryRunGuardKey::CargoFix,
                disposition: if is_dry {
                    DryRunDisposition::DryRun
                } else {
                    DryRunDisposition::Mutating
                },
                display_name: "cargo fix".to_string(),
                suggested_dry_run: None,
            })
        }
        "clippy" => {
            let has_fix = slice_contains_flag(args, "--fix");
            if !has_fix {
                return None;
            }
            let is_dry = slice_contains_flag(args, "--dry-run");
            Some(InternalAnalysis {
                key: DryRunGuardKey::CargoClippyFix,
                disposition: if is_dry {
                    DryRunDisposition::DryRun
                } else {
                    DryRunDisposition::Mutating
                },
                display_name: "cargo clippy --fix".to_string(),
                suggested_dry_run: None,
            })
        }
        _ => None,
    }
}

fn analyze_rustfmt(tokens: &[String]) -> Option<InternalAnalysis> {
    if tokens.is_empty() || !equal_ignore_case(&tokens[0], "rustfmt") {
        return None;
    }

    let args = if tokens.len() > 1 { &tokens[1..] } else { &[] };
    let is_dry = slice_contains_flag(args, "--check");
    Some(InternalAnalysis {
        key: DryRunGuardKey::Rustfmt,
        disposition: if is_dry {
            DryRunDisposition::DryRun
        } else {
            DryRunDisposition::Mutating
        },
        display_name: "rustfmt".to_string(),
        suggested_dry_run: None,
    })
}

fn analyze_package_manager(tokens: &[String], manager: PackageManager) -> Option<InternalAnalysis> {
    if tokens.is_empty() || !equal_ignore_case(&tokens[0], manager.label()) {
        return None;
    }

    if tokens.len() < 2 {
        return None;
    }

    let (script_idx, script) = locate_package_script(tokens, manager)?;
    if script.eq_ignore_ascii_case("exec") || script.eq_ignore_ascii_case("dlx") {
        return analyze_direct_formatter(tokens);
    }

    let script_lower = script.to_ascii_lowercase();
    let args = if script_idx + 1 < tokens.len() {
        &tokens[script_idx + 1..]
    } else {
        &[]
    };

    let (key, display_name) = if script_lower.contains("lint") {
        (
            DryRunGuardKey::PackageLint(manager),
            format!("{} {}", manager.label(), script),
        )
    } else if script_lower.contains("format")
        || script_lower.contains("fmt")
        || script_lower.contains("prettier")
    {
        (
            DryRunGuardKey::PackageFormat(manager),
            format!("{} {}", manager.label(), script),
        )
    } else {
        return None;
    };

    let is_dry = args.iter().any(|arg| {
        let lower = arg.to_ascii_lowercase();
        equal_ignore_case(arg, "--check")
            || equal_ignore_case(arg, "--dry-run")
            || equal_ignore_case(arg, "--no-fix")
            || lower.contains("fix=false")
            || lower.contains("write=false")
            || lower.contains("mode=check")
    });

    Some(InternalAnalysis {
        key,
        disposition: if is_dry {
            DryRunDisposition::DryRun
        } else {
            DryRunDisposition::Mutating
        },
        display_name,
        suggested_dry_run: None,
    })
}

fn locate_package_script(tokens: &[String], manager: PackageManager) -> Option<(usize, String)> {
    match manager {
        PackageManager::Npm => locate_npm_script(tokens),
        PackageManager::Pnpm => locate_simple_script(tokens),
        PackageManager::Yarn => locate_simple_script(tokens),
    }
}

fn locate_npm_script(tokens: &[String]) -> Option<(usize, String)> {
    if tokens.len() < 2 {
        return None;
    }

    if equal_ignore_case(&tokens[1], "run") {
        let script = tokens.get(2)?.clone();
        Some((2, script))
    } else {
        let script = tokens.get(1)?.clone();
        Some((1, script))
    }
}

fn locate_simple_script(tokens: &[String]) -> Option<(usize, String)> {
    let script = tokens.get(1)?.clone();
    Some((1, script))
}

fn analyze_direct_formatter(tokens: &[String]) -> Option<InternalAnalysis> {
    if tokens.is_empty() {
        return None;
    }

    let actual = peel_exec_wrappers(tokens);
    if actual.is_empty() {
        return None;
    }

    let base = command_basename(&actual[0]);
    let args = if actual.len() > 1 { &actual[1..] } else { &[] };

    match base.as_str() {
        "eslint" => {
            let has_fix = actual.iter().any(|t| equal_ignore_case(t, "--fix"));
            if !has_fix {
                return None;
            }
            let is_dry = actual.iter().any(|t| {
                equal_ignore_case(t, "--fix-dry-run") || equal_ignore_case(t, "--dry-run")
            });
            Some(InternalAnalysis {
                key: DryRunGuardKey::EslintFix,
                disposition: if is_dry {
                    DryRunDisposition::DryRun
                } else {
                    DryRunDisposition::Mutating
                },
                display_name: "eslint --fix".to_string(),
                suggested_dry_run: None,
            })
        }
        "prettier" => {
            let has_write = actual
                .iter()
                .any(|t| equal_ignore_case(t, "--write") || t == "-w");
            if !has_write {
                return None;
            }
            let is_dry = actual.iter().any(|t| {
                equal_ignore_case(t, "--check")
                    || equal_ignore_case(t, "--list-different")
                    || equal_ignore_case(t, "--dry-run")
            });
            Some(InternalAnalysis {
                key: DryRunGuardKey::PrettierWrite,
                disposition: if is_dry {
                    DryRunDisposition::DryRun
                } else {
                    DryRunDisposition::Mutating
                },
                display_name: "prettier --write".to_string(),
                suggested_dry_run: None,
            })
        }
        "black" => {
            let is_dry = actual.iter().any(|t| equal_ignore_case(t, "--check"));
            Some(InternalAnalysis {
                key: DryRunGuardKey::Black,
                disposition: if is_dry {
                    DryRunDisposition::DryRun
                } else {
                    DryRunDisposition::Mutating
                },
                display_name: "black".to_string(),
                suggested_dry_run: None,
            })
        }
        "ruff" => {
            if actual.len() < 2 || !equal_ignore_case(&actual[1], "format") {
                return None;
            }
            let is_dry = args.iter().any(|t| equal_ignore_case(t, "--check"));
            Some(InternalAnalysis {
                key: DryRunGuardKey::RuffFormat,
                disposition: if is_dry {
                    DryRunDisposition::DryRun
                } else {
                    DryRunDisposition::Mutating
                },
                display_name: "ruff format".to_string(),
                suggested_dry_run: None,
            })
        }
        "isort" => {
            let is_dry = actual.iter().any(|t| equal_ignore_case(t, "--check-only"));
            Some(InternalAnalysis {
                key: DryRunGuardKey::Isort,
                disposition: if is_dry {
                    DryRunDisposition::DryRun
                } else {
                    DryRunDisposition::Mutating
                },
                display_name: "isort".to_string(),
                suggested_dry_run: None,
            })
        }
        "gofmt" => {
            let has_write = actual.iter().any(|t| t == "-w" || t.starts_with("-w="));
            if !has_write {
                return None;
            }
            let is_dry = actual.iter().any(|t| t == "-d" || t == "-l" || t == "-n");
            Some(InternalAnalysis {
                key: DryRunGuardKey::Gofmt,
                disposition: if is_dry {
                    DryRunDisposition::DryRun
                } else {
                    DryRunDisposition::Mutating
                },
                display_name: "gofmt -w".to_string(),
                suggested_dry_run: None,
            })
        }
        "rustywind" => {
            let is_dry = actual.iter().any(|t| equal_ignore_case(t, "--check"));
            Some(InternalAnalysis {
                key: DryRunGuardKey::Rustywind,
                disposition: if is_dry {
                    DryRunDisposition::DryRun
                } else {
                    DryRunDisposition::Mutating
                },
                display_name: "rustywind".to_string(),
                suggested_dry_run: None,
            })
        }
        "dart" => {
            if actual.len() < 2 || !equal_ignore_case(&actual[1], "format") {
                return None;
            }
            let is_dry = actual.iter().any(|t| equal_ignore_case(t, "--dry-run"));
            Some(InternalAnalysis {
                key: DryRunGuardKey::DartFormat,
                disposition: if is_dry {
                    DryRunDisposition::DryRun
                } else {
                    DryRunDisposition::Mutating
                },
                display_name: "dart format".to_string(),
                suggested_dry_run: None,
            })
        }
        "swiftformat" => {
            let is_dry = actual
                .iter()
                .any(|t| equal_ignore_case(t, "--lint") || equal_ignore_case(t, "--dryrun"));
            Some(InternalAnalysis {
                key: DryRunGuardKey::SwiftFormat,
                disposition: if is_dry {
                    DryRunDisposition::DryRun
                } else {
                    DryRunDisposition::Mutating
                },
                display_name: "swiftformat".to_string(),
                suggested_dry_run: None,
            })
        }
        _ => None,
    }
}

fn peel_exec_wrappers<'a>(mut tokens: &'a [String]) -> &'a [String] {
    loop {
        if tokens.is_empty() {
            return tokens;
        }
        let base = command_basename(&tokens[0]);
        match base.as_str() {
            "npx" | "bunx" => {
                tokens = skip_leading_flags(&tokens[1..]);
                continue;
            }
            "pnpm" => {
                if tokens.len() >= 2 && matches!(tokens[1].as_str(), "exec" | "dlx") {
                    tokens = skip_leading_flags(&tokens[2..]);
                    continue;
                }
            }
            "yarn" => {
                if tokens.len() >= 2 {
                    let second = command_basename(&tokens[1]);
                    if matches!(second.as_str(), "exec" | "dlx") {
                        tokens = skip_leading_flags(&tokens[2..]);
                        continue;
                    }
                }
            }
            _ => {}
        }
        break;
    }
    tokens
}

fn skip_leading_flags<'a>(tokens: &'a [String]) -> &'a [String] {
    let mut idx = 0usize;
    while idx < tokens.len() {
        let token = tokens[idx].as_str();
        if token == "--" {
            idx += 1;
            break;
        }
        if token.starts_with('-') {
            idx += 1;
            continue;
        }
        break;
    }
    &tokens[idx..]
}

fn slice_contains_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| equal_ignore_case(arg, flag))
}
