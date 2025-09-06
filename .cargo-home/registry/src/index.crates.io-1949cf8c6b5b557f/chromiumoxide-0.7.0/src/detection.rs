use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DetectionOptions {
    /// Detect Microsoft Edge
    pub msedge: bool,

    /// Detect unstable installations (beta, dev, unstable)
    pub unstable: bool,
}

impl Default for DetectionOptions {
    fn default() -> Self {
        Self {
            msedge: true,
            unstable: false,
        }
    }
}

/// Returns the path to Chrome's executable.
///
/// The following elements will be checked:
///   - `CHROME` environment variable
///   - Usual filenames in the user path
///   - (Windows) Registry
///   - (Windows & MacOS) Usual installations paths
///     If all of the above fail, an error is returned.
pub fn default_executable(options: DetectionOptions) -> Result<std::path::PathBuf, String> {
    if let Some(path) = get_by_env_var() {
        return Ok(path);
    }

    if let Some(path) = get_by_name(&options) {
        return Ok(path);
    }

    #[cfg(windows)]
    if let Some(path) = get_by_registry() {
        return Ok(path);
    }

    if let Some(path) = get_by_path(&options) {
        return Ok(path);
    }

    Err("Could not auto detect a chrome executable".to_string())
}

fn get_by_env_var() -> Option<PathBuf> {
    if let Ok(path) = env::var("CHROME") {
        if Path::new(&path).exists() {
            return Some(path.into());
        }
    }

    None
}

fn get_by_name(options: &DetectionOptions) -> Option<PathBuf> {
    let default_apps = [
        ("chrome", true),
        ("chrome-browser", true),
        ("google-chrome-stable", true),
        ("google-chrome-beta", options.unstable),
        ("google-chrome-dev", options.unstable),
        ("google-chrome-unstable", options.unstable),
        ("chromium", true),
        ("chromium-browser", true),
        ("msedge", options.msedge),
        ("microsoft-edge", options.msedge),
        ("microsoft-edge-stable", options.msedge),
        ("microsoft-edge-beta", options.msedge && options.unstable),
        ("microsoft-edge-dev", options.msedge && options.unstable),
    ];
    for (app, allowed) in default_apps {
        if !allowed {
            continue;
        }
        if let Ok(path) = which::which(app) {
            return Some(path);
        }
    }

    None
}

#[allow(unused_variables)]
fn get_by_path(options: &DetectionOptions) -> Option<PathBuf> {
    #[cfg(all(unix, not(target_os = "macos")))]
    let default_paths: [(&str, bool); 3] = [
        ("/opt/chromium.org/chromium", true),
        ("/opt/google/chrome", true),
        // test for lambda
        ("/tmp/aws/lib", true),
    ];
    #[cfg(windows)]
    let default_paths = [(
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        options.msedge,
    )];
    #[cfg(target_os = "macos")]
    let default_paths = [
        (
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            true,
        ),
        (
            "/Applications/Google Chrome Beta.app/Contents/MacOS/Google Chrome Beta",
            options.unstable,
        ),
        (
            "/Applications/Google Chrome Dev.app/Contents/MacOS/Google Chrome Dev",
            options.unstable,
        ),
        (
            "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
            options.unstable,
        ),
        ("/Applications/Chromium.app/Contents/MacOS/Chromium", true),
        (
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            options.msedge,
        ),
        (
            "/Applications/Microsoft Edge Beta.app/Contents/MacOS/Microsoft Edge Beta",
            options.msedge && options.unstable,
        ),
        (
            "/Applications/Microsoft Edge Dev.app/Contents/MacOS/Microsoft Edge Dev",
            options.msedge && options.unstable,
        ),
        (
            "/Applications/Microsoft Edge Canary.app/Contents/MacOS/Microsoft Edge Canary",
            options.msedge && options.unstable,
        ),
    ];

    for (path, allowed) in default_paths {
        if !allowed {
            continue;
        }
        if Path::new(path).exists() {
            return Some(path.into());
        }
    }

    None
}

#[cfg(windows)]
fn get_by_registry() -> Option<PathBuf> {
    winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\chrome.exe")
        .or_else(|_| {
            winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
                .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\chrome.exe")
        })
        .and_then(|key| key.get_value::<String, _>(""))
        .map(PathBuf::from)
        .ok()
}
