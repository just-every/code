use std::future::Future;
use std::path::Path;
use std::path::PathBuf;

use code_core::config::resolve_code_path_for_read;
use code_core::CODEX_APPLY_PATCH_ARG1;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use tempfile::TempDir;

const LINUX_SANDBOX_ARG0: &str = "codex-linux-sandbox";
const APPLY_PATCH_ARG0: &str = "apply_patch";
const MISSPELLED_APPLY_PATCH_ARG0: &str = "applypatch";

/// While we want to deploy the Codex CLI as a single executable for simplicity,
/// we also want to expose some of its functionality as distinct CLIs, so we use
/// the "arg0 trick" to determine which CLI to dispatch. This effectively allows
/// us to simulate deploying multiple executables as a single binary on Mac and
/// Linux (but not Windows).
///
/// When the current executable is invoked through the hard-link or alias named
/// `codex-linux-sandbox` we *directly* execute
/// [`code_linux_sandbox::run_main`] (which never returns). Otherwise we:
///
/// 1.  Use [`dotenvy::from_path`] and [`dotenvy::dotenv`] to modify the
///     environment before creating any threads.
/// 2.  Construct a Tokio multi-thread runtime.
/// 3.  Derive the path to the current executable (so children can re-invoke the
///     sandbox) when running on Linux.
/// 4.  Execute the provided async `main_fn` inside that runtime, forwarding any
///     error. Note that `main_fn` receives `code_linux_sandbox_exe:
///     Option<PathBuf>`, as an argument, which is generally needed as part of
///     constructing [`code_core::config::Config`].
///
/// This function should be used to wrap any `main()` function in binary crates
/// in this workspace that depends on these helper CLIs.
pub fn arg0_dispatch_or_else<F, Fut>(main_fn: F) -> anyhow::Result<()>
where
    F: FnOnce(Option<PathBuf>) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    // Determine if we were invoked via the special alias.
    let mut args = std::env::args_os();
    let argv0 = args.next().unwrap_or_default();
    let exe_name = Path::new(&argv0)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if exe_name == LINUX_SANDBOX_ARG0 {
        // Safety: [`run_main`] never returns.
        code_linux_sandbox::run_main();
    } else if exe_name == APPLY_PATCH_ARG0 || exe_name == MISSPELLED_APPLY_PATCH_ARG0 {
        code_apply_patch::main();
    }

    let argv1 = args.next().unwrap_or_default();
    if argv1 == CODEX_APPLY_PATCH_ARG1 {
        let patch_arg = args.next().and_then(|s| s.to_str().map(str::to_owned));
        let exit_code = match patch_arg {
            Some(patch_arg) => {
                let mut stdout = std::io::stdout();
                let mut stderr = std::io::stderr();
                match code_apply_patch::apply_patch(&patch_arg, &mut stdout, &mut stderr) {
                    Ok(()) => 0,
                    Err(_) => 1,
                }
            }
            None => {
                eprintln!("Error: {CODEX_APPLY_PATCH_ARG1} requires a UTF-8 PATCH argument.");
                1
            }
        };
        std::process::exit(exit_code);
    }

    // This modifies the environment, which is not thread-safe, so do this
    // before creating any threads/the Tokio runtime.
    load_dotenv();

    // Retain the TempDir so it exists for the lifetime of the invocation of
    // this executable. Admittedly, we could invoke `keep()` on it, but it
    // would be nice to avoid leaving temporary directories behind, if possible.
    let _path_entry = match prepend_path_entry_for_apply_patch() {
        Ok(path_entry) => Some(path_entry),
        Err(err) => {
            // It is possible that Codex will proceed successfully even if
            // updating the PATH fails, so warn the user and move on.
            eprintln!("WARNING: proceeding, even though we could not update PATH: {err}");
            None
        }
    };

    // Regular invocation – create a Tokio runtime and execute the provided
    // async entry-point.
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let code_linux_sandbox_exe: Option<PathBuf> = if cfg!(target_os = "linux") {
            std::env::current_exe().ok()
        } else {
            None
        };

        main_fn(code_linux_sandbox_exe).await
    })
}

const ILLEGAL_ENV_VAR_PREFIX: &str = "CODEX_";

/// Load env vars from ~/.code/.env (legacy ~/.codex/.env is still read) and `$(pwd)/.env`.
///
/// Security: Do not allow `.env` files to create or modify any variables
/// with names starting with `CODEX_`.
fn load_dotenv() {
    // 1) Load from global ~/.code/.env (or ~/.codex/.env) first.
    if let Ok(code_home) = code_core::config::find_code_home() {
        let global_env_path = resolve_code_path_for_read(&code_home, Path::new(".env"));
        if let Ok(iter) = dotenvy::from_path_iter(global_env_path) {
            // Global env may legitimately contain provider keys for Code usage.
            set_filtered(iter);
        }
    }

    // 2) Load from the current project's .env, but with extra safety:
    //    - Do NOT import provider API keys by default (e.g., OPENAI_API_KEY, AZURE_OPENAI_API_KEY).
    //    - Users can opt back in via CODEX_ALLOW_PROJECT_OPENAI_KEYS=1 (either exported
    //      in the shell or placed in ~/.code/.env).
    if let Ok(iter) = dotenvy::dotenv_iter() {
        // Filtered setter that always blocks provider keys from the project's .env.
        for (key, value) in iter.into_iter().flatten() {
            let upper = key.to_ascii_uppercase();
            // Never allow CODEX_* to be set from .env files for safety.
            if upper.starts_with(ILLEGAL_ENV_VAR_PREFIX) && upper != "CODEX_HOME" { continue; }
            // Always ignore provider keys from project .env (must be set globally or in shell).
            if upper == "OPENAI_API_KEY" || upper == "AZURE_OPENAI_API_KEY" { continue; }
            // Safe: still single-threaded during startup.
            unsafe { std::env::set_var(&key, &value) };
        }
    }

    // Bridge CODE_HOME to CODEX_HOME for legacy components that still read only CODEX_HOME.
    let codex_home_missing = std::env::var("CODEX_HOME")
        .map(|v| v.trim().is_empty())
        .unwrap_or(true);
    if codex_home_missing {
        if let Ok(code_home) = std::env::var("CODE_HOME") {
            if !code_home.trim().is_empty() {
                // Safe: still single-threaded during startup.
                unsafe { std::env::set_var("CODEX_HOME", code_home) };
            }
        }
    }
}

/// Helper to set vars from a dotenvy iterator while filtering out `CODEX_` keys.
fn set_filtered<I>(iter: I)
where
    I: IntoIterator<Item = Result<(String, String), dotenvy::Error>>,
{
    for (key, value) in iter.into_iter().flatten() {
        if !key.to_ascii_uppercase().starts_with(ILLEGAL_ENV_VAR_PREFIX) {
            // It is safe to call set_var() because our process is
            // single-threaded at this point in its execution.
            unsafe { std::env::set_var(&key, &value) };
        }
    }
}

/// Creates a temporary directory with either:
///
/// - UNIX: `apply_patch` symlink to the current executable
/// - WINDOWS: `apply_patch.bat` batch script to invoke the current executable
///   with the "secret" --codex-run-as-apply-patch flag.
///
/// This temporary directory is prepended to the PATH environment variable so
/// that `apply_patch` can be on the PATH without requiring the user to
/// install a separate `apply_patch` executable, simplifying the deployment of
/// Codex CLI.
///
/// IMPORTANT: This function modifies the PATH environment variable, so it MUST
/// be called before multiple threads are spawned.
fn prepend_path_entry_for_apply_patch() -> std::io::Result<TempDir> {
    let temp_dir = TempDir::new()?;
    let path = temp_dir.path();

    for filename in &[APPLY_PATCH_ARG0, MISSPELLED_APPLY_PATCH_ARG0] {
        let exe = std::env::current_exe()?;

        #[cfg(unix)]
        {
            let link = path.join(filename);
            symlink(&exe, &link)?;
        }

        #[cfg(windows)]
        {
            let batch_script = path.join(format!("{filename}.bat"));
            std::fs::write(
                &batch_script,
                format!(
                    r#"@echo off
"{}" {CODEX_APPLY_PATCH_ARG1} %*
"#,
                    exe.display()
                ),
            )?;
        }
    }

    #[cfg(unix)]
    const PATH_SEPARATOR: &str = ":";

    #[cfg(windows)]
    const PATH_SEPARATOR: &str = ";";

    let path_element = path.display();

    let mut existing_path = std::env::var("PATH").unwrap_or_default();
    if existing_path.is_empty() {
        existing_path = default_path_env_var();
    } else if !path_has_standard_dirs(&existing_path) {
        existing_path = join_path_env(&existing_path, &default_path_env_var(), PATH_SEPARATOR);
    }

    let updated_path_env_var = join_path_env(&path_element.to_string(), &existing_path, PATH_SEPARATOR);

    unsafe {
        std::env::set_var("PATH", updated_path_env_var);
    }

    Ok(temp_dir)
}

#[cfg(unix)]
fn default_path_env_var() -> String {
    // When PATH is missing (common in headless/non-interactive runners), many
    // libc implementations fall back to a built-in default search path (e.g.
    // /bin:/usr/bin). As soon as we set PATH, that fallback stops applying.
    // Ensure we always include standard system locations.
    if cfg!(target_os = "macos") {
        "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".to_string()
    } else {
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string()
    }
}

#[cfg(windows)]
fn default_path_env_var() -> String {
    // On Windows, rely on the system-provided default search behavior.
    // If PATH is missing, leave it empty to avoid guessing.
    String::new()
}

fn join_path_env(prefix: &str, suffix: &str, sep: &str) -> String {
    match (prefix.is_empty(), suffix.is_empty()) {
        (true, true) => String::new(),
        (false, true) => prefix.to_string(),
        (true, false) => suffix.to_string(),
        (false, false) => format!("{prefix}{sep}{suffix}"),
    }
}

#[cfg(unix)]
fn path_has_standard_dirs(path: &str) -> bool {
    use std::ffi::OsString;

    let os = OsString::from(path);
    std::env::split_paths(&os).any(|p| {
        p == std::path::Path::new("/usr/bin")
            || p == std::path::Path::new("/bin")
            || p == std::path::Path::new("/usr/sbin")
            || p == std::path::Path::new("/sbin")
    })
}

#[cfg(windows)]
fn path_has_standard_dirs(_path: &str) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static PATH_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn prepend_path_seeds_default_when_missing() {
        let _guard = PATH_LOCK.lock().unwrap();

        let before = std::env::var("PATH").ok();
        unsafe { std::env::remove_var("PATH") };

        let td = prepend_path_entry_for_apply_patch().unwrap();
        let path = std::env::var("PATH").unwrap();

        // The temp dir should be the first search entry so `apply_patch` resolves.
        assert!(path.starts_with(&format!("{}", td.path().display())));

        #[cfg(unix)]
        assert!(
            path.contains("/usr/bin") || path.contains(":/bin") || path.contains("/bin:"),
            "PATH should include standard system dirs, got: {path}"
        );

        // Restore.
        match before {
            Some(v) => unsafe { std::env::set_var("PATH", v) },
            None => unsafe { std::env::remove_var("PATH") },
        }
    }

    #[test]
    fn prepend_path_seeds_default_when_incomplete() {
        let _guard = PATH_LOCK.lock().unwrap();

        let before = std::env::var("PATH").ok();
        unsafe { std::env::set_var("PATH", "/usr/local/bin") };

        let td = prepend_path_entry_for_apply_patch().unwrap();
        let path = std::env::var("PATH").unwrap();

        assert!(path.starts_with(&format!("{}", td.path().display())));

        #[cfg(unix)]
        assert!(
            path.contains("/usr/bin") || path.contains(":/bin") || path.contains("/bin:"),
            "PATH should include standard system dirs, got: {path}"
        );

        match before {
            Some(v) => unsafe { std::env::set_var("PATH", v) },
            None => unsafe { std::env::remove_var("PATH") },
        }
    }
}
