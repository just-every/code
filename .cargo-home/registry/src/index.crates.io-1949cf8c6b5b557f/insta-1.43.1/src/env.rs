use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::{env, fmt, fs};

use crate::utils::is_ci;
use crate::{
    content::{yaml, Content},
    elog,
};

use once_cell::sync::Lazy;

static WORKSPACES: Lazy<Mutex<BTreeMap<String, Arc<PathBuf>>>> =
    Lazy::new(|| Mutex::new(BTreeMap::new()));
static TOOL_CONFIGS: Lazy<Mutex<BTreeMap<PathBuf, Arc<ToolConfig>>>> =
    Lazy::new(|| Mutex::new(BTreeMap::new()));

pub fn get_tool_config(workspace_dir: &Path) -> Arc<ToolConfig> {
    TOOL_CONFIGS
        .lock()
        .unwrap()
        .entry(workspace_dir.to_path_buf())
        .or_insert_with(|| {
            ToolConfig::from_workspace(workspace_dir)
                .unwrap_or_else(|e| panic!("Error building config from {:?}: {}", workspace_dir, e))
                .into()
        })
        .clone()
}

/// The test runner to use.
#[cfg(feature = "_cargo_insta_internal")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum TestRunner {
    Auto,
    CargoTest,
    Nextest,
}

#[cfg(feature = "_cargo_insta_internal")]
impl TestRunner {
    /// Fall back to `cargo test` if `cargo nextest` isn't installed and
    /// `test_runner_fallback` is true
    pub fn resolve_fallback(&self, test_runner_fallback: bool) -> &TestRunner {
        use crate::utils::get_cargo;
        if self == &TestRunner::Nextest
            && test_runner_fallback
            && std::process::Command::new(get_cargo())
                .arg("nextest")
                .arg("--version")
                .output()
                .map(|output| !output.status.success())
                .unwrap_or(true)
        {
            &TestRunner::Auto
        } else {
            self
        }
    }
}

/// Controls how information is supposed to be displayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputBehavior {
    /// Diff only
    Diff,
    /// Short summary
    Summary,
    /// The most minimal output
    Minimal,
    /// No output at all
    Nothing,
}

/// Unreferenced snapshots flag
#[cfg(feature = "_cargo_insta_internal")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum UnreferencedSnapshots {
    Auto,
    Reject,
    Delete,
    Warn,
    Ignore,
}

/// Snapshot update flag
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotUpdate {
    Always,
    Auto,
    Unseen,
    New,
    No,
    Force,
}

#[derive(Debug)]
pub enum Error {
    Deserialize(crate::content::Error),
    Env(&'static str),
    #[allow(unused)]
    Config(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Deserialize(_) => write!(f, "failed to deserialize tool config"),
            Error::Env(var) => write!(f, "invalid value for env var '{}'", var),
            Error::Config(var) => write!(f, "invalid value for config '{}'", var),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Deserialize(ref err) => Some(err),
            _ => None,
        }
    }
}

/// Represents a tool configuration.
#[derive(Debug, Clone)]
pub struct ToolConfig {
    force_pass: bool,
    require_full_match: bool,
    output: OutputBehavior,
    snapshot_update: SnapshotUpdate,
    #[cfg(feature = "glob")]
    glob_fail_fast: bool,
    #[cfg(feature = "_cargo_insta_internal")]
    test_runner_fallback: bool,
    #[cfg(feature = "_cargo_insta_internal")]
    test_runner: TestRunner,
    #[cfg(feature = "_cargo_insta_internal")]
    test_unreferenced: UnreferencedSnapshots,
    #[cfg(feature = "_cargo_insta_internal")]
    auto_review: bool,
    #[cfg(feature = "_cargo_insta_internal")]
    auto_accept_unseen: bool,
    #[cfg(feature = "_cargo_insta_internal")]
    review_include_ignored: bool,
    #[cfg(feature = "_cargo_insta_internal")]
    review_include_hidden: bool,
    #[cfg(feature = "_cargo_insta_internal")]
    review_warn_undiscovered: bool,
}

impl ToolConfig {
    /// Loads the tool config from a cargo workspace.
    pub fn from_workspace(workspace_dir: &Path) -> Result<ToolConfig, Error> {
        let mut cfg = None;
        for choice in &[".config/insta.yaml", "insta.yaml", ".insta.yaml"] {
            let path = workspace_dir.join(choice);
            match fs::read_to_string(&path) {
                Ok(s) => {
                    cfg = Some(yaml::parse_str(&s, &path).map_err(Error::Deserialize)?);
                    break;
                }
                // ideally we would not swallow all errors here but unfortunately there are
                // some cases where we cannot detect the error properly.
                // Eg we can see NotADirectory here as kind, but on stable rust it cannot
                // be matched on.
                Err(_) => continue,
            }
        }
        let cfg = cfg.unwrap_or_else(|| Content::Map(Default::default()));

        // Support for the deprecated environment variables.  This is
        // implemented in a way that cargo-insta can support older and newer
        // insta versions alike. Versions of `cargo-insta` <= 1.39 will set
        // `INSTA_FORCE_UPDATE_SNAPSHOTS` & `INSTA_FORCE_UPDATE`.
        //
        // If `INSTA_FORCE_UPDATE_SNAPSHOTS` is the only env var present we emit
        // a deprecation warning, later to be expanded to `INSTA_FORCE_UPDATE`.
        //
        // Another approach would be to pass the version of `cargo-insta` in a
        // `INSTA_CARGO_INSTA_VERSION` env var, and then raise a warning unless
        // running under cargo-insta <= 1.39. Though it would require adding a
        // `semver` dependency to this crate or doing the version comparison
        // ourselves (a tractable task...).
        let force_update_old_env_vars = if let Ok("1") = env::var("INSTA_FORCE_UPDATE").as_deref() {
            // Don't raise a warning yet, because recent versions of
            // `cargo-insta` use this, so that it's compatible with older
            // versions of `insta`.
            //
            //   elog!("INSTA_FORCE_UPDATE is deprecated, use
            //   INSTA_UPDATE=force");
            true
        } else if let Ok("1") = env::var("INSTA_FORCE_UPDATE_SNAPSHOTS").as_deref() {
            // Warn on an old envvar.
            //
            // There's some possibility that we're running from within an fairly
            // old version of `cargo-insta` (before we added an
            // `INSTA_CARGO_INSTA` env var, so we can't pick that up.) So offer
            // a caveat in that case.
            elog!("INSTA_FORCE_UPDATE_SNAPSHOTS is deprecated, use INSTA_UPDATE=force. (If running from `cargo insta`, no action is required; upgrading `cargo-insta` will silence this warning.)");
            true
        } else {
            false
        };
        if force_update_old_env_vars {
            env::set_var("INSTA_UPDATE", "force");
        }

        Ok(ToolConfig {
            require_full_match: match env::var("INSTA_REQUIRE_FULL_MATCH").as_deref() {
                Err(_) | Ok("") => resolve(&cfg, &["behavior", "require_full_match"])
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
                Ok("0") => false,
                Ok("1") => true,
                _ => return Err(Error::Env("INSTA_REQUIRE_FULL_MATCH")),
            },
            force_pass: match env::var("INSTA_FORCE_PASS").as_deref() {
                Err(_) | Ok("") => resolve(&cfg, &["behavior", "force_pass"])
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
                Ok("0") => false,
                Ok("1") => true,
                _ => return Err(Error::Env("INSTA_FORCE_PASS")),
            },
            output: {
                let env_var = env::var("INSTA_OUTPUT");
                let val = match env_var.as_deref() {
                    Err(_) | Ok("") => resolve(&cfg, &["behavior", "output"])
                        .and_then(|x| x.as_str())
                        .unwrap_or("diff"),
                    Ok(val) => val,
                };
                match val {
                    "diff" => OutputBehavior::Diff,
                    "summary" => OutputBehavior::Summary,
                    "minimal" => OutputBehavior::Minimal,
                    "none" => OutputBehavior::Nothing,
                    _ => return Err(Error::Env("INSTA_OUTPUT")),
                }
            },
            snapshot_update: {
                let env_var = env::var("INSTA_UPDATE");
                let val = match env_var.as_deref() {
                    Err(_) | Ok("") => resolve(&cfg, &["behavior", "update"])
                        .and_then(|x| x.as_str())
                        // Legacy support for the old force update config
                        .or(resolve(&cfg, &["behavior", "force_update"]).and_then(|x| {
                            elog!("`force_update: true` is deprecated in insta config files, use `update: force`");
                            match x.as_bool() {
                                Some(true) => Some("force"),
                                _ => None,
                            }
                        }))
                        .unwrap_or("auto"),
                    Ok(val) => val,
                };
                match val {
                    "auto" => SnapshotUpdate::Auto,
                    "always" | "1" => SnapshotUpdate::Always,
                    "new" => SnapshotUpdate::New,
                    "unseen" => SnapshotUpdate::Unseen,
                    "no" => SnapshotUpdate::No,
                    "force" => SnapshotUpdate::Force,
                    _ => return Err(Error::Env("INSTA_UPDATE")),
                }
            },
            #[cfg(feature = "glob")]
            glob_fail_fast: match env::var("INSTA_GLOB_FAIL_FAST").as_deref() {
                Err(_) | Ok("") => resolve(&cfg, &["behavior", "glob_fail_fast"])
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
                Ok("1") => true,
                Ok("0") => false,
                _ => return Err(Error::Env("INSTA_GLOB_FAIL_FAST")),
            },
            #[cfg(feature = "_cargo_insta_internal")]
            test_runner: {
                let env_var = env::var("INSTA_TEST_RUNNER");
                match env_var.as_deref() {
                    Err(_) | Ok("") => resolve(&cfg, &["test", "runner"])
                        .and_then(|x| x.as_str())
                        .unwrap_or("auto"),
                    Ok(val) => val,
                }
                .parse::<TestRunner>()
                .map_err(|_| Error::Env("INSTA_TEST_RUNNER"))?
            },
            #[cfg(feature = "_cargo_insta_internal")]
            test_runner_fallback: match env::var("INSTA_TEST_RUNNER_FALLBACK").as_deref() {
                Err(_) | Ok("") => resolve(&cfg, &["test", "runner_fallback"])
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
                Ok("1") => true,
                Ok("0") => false,
                _ => return Err(Error::Env("INSTA_RUNNER_FALLBACK")),
            },
            #[cfg(feature = "_cargo_insta_internal")]
            test_unreferenced: {
                resolve(&cfg, &["test", "unreferenced"])
                    .and_then(|x| x.as_str())
                    .unwrap_or("ignore")
                    .parse::<UnreferencedSnapshots>()
                    .map_err(|_| Error::Config("unreferenced"))?
            },
            #[cfg(feature = "_cargo_insta_internal")]
            auto_review: resolve(&cfg, &["test", "auto_review"])
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
            #[cfg(feature = "_cargo_insta_internal")]
            auto_accept_unseen: resolve(&cfg, &["test", "auto_accept_unseen"])
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
            #[cfg(feature = "_cargo_insta_internal")]
            review_include_hidden: resolve(&cfg, &["review", "include_hidden"])
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
            #[cfg(feature = "_cargo_insta_internal")]
            review_include_ignored: resolve(&cfg, &["review", "include_ignored"])
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
            #[cfg(feature = "_cargo_insta_internal")]
            review_warn_undiscovered: resolve(&cfg, &["review", "warn_undiscovered"])
                .and_then(|x| x.as_bool())
                .unwrap_or(true),
        })
    }

    // TODO: Do we want all these methods, vs. just allowing access to the fields?

    /// Should we fail if metadata doesn't match?
    pub fn require_full_match(&self) -> bool {
        self.require_full_match
    }

    /// Is insta instructed to fail in tests?
    pub fn force_pass(&self) -> bool {
        self.force_pass
    }

    /// Returns the intended output behavior for insta.
    pub fn output_behavior(&self) -> OutputBehavior {
        self.output
    }

    /// Returns the intended snapshot update behavior.
    pub fn snapshot_update(&self) -> SnapshotUpdate {
        self.snapshot_update
    }

    /// Returns whether the glob should fail fast, as snapshot failures within the glob macro will appear only at the end of execution unless `glob_fail_fast` is set.
    #[cfg(feature = "glob")]
    pub fn glob_fail_fast(&self) -> bool {
        self.glob_fail_fast
    }
}

#[cfg(feature = "_cargo_insta_internal")]
impl ToolConfig {
    /// Returns the intended test runner
    pub fn test_runner(&self) -> TestRunner {
        self.test_runner
    }

    /// Whether to fallback to `cargo test` if the test runner isn't available
    pub fn test_runner_fallback(&self) -> bool {
        self.test_runner_fallback
    }

    pub fn test_unreferenced(&self) -> UnreferencedSnapshots {
        self.test_unreferenced
    }

    /// Returns the auto review flag.
    pub fn auto_review(&self) -> bool {
        self.auto_review
    }

    /// Returns the auto accept unseen flag.
    pub fn auto_accept_unseen(&self) -> bool {
        self.auto_accept_unseen
    }

    pub fn review_include_hidden(&self) -> bool {
        self.review_include_hidden
    }

    pub fn review_include_ignored(&self) -> bool {
        self.review_include_ignored
    }

    pub fn review_warn_undiscovered(&self) -> bool {
        self.review_warn_undiscovered
    }
}

/// How snapshots are supposed to be updated
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotUpdateBehavior {
    /// Snapshots are updated in-place
    InPlace,
    /// Snapshots are placed in a new file with a .new suffix
    NewFile,
    /// Snapshots are not updated at all.
    NoUpdate,
}

/// Returns the intended snapshot update behavior.
pub fn snapshot_update_behavior(tool_config: &ToolConfig, unseen: bool) -> SnapshotUpdateBehavior {
    match tool_config.snapshot_update() {
        SnapshotUpdate::Always => SnapshotUpdateBehavior::InPlace,
        SnapshotUpdate::Auto => {
            if is_ci() {
                SnapshotUpdateBehavior::NoUpdate
            } else {
                SnapshotUpdateBehavior::NewFile
            }
        }
        SnapshotUpdate::Unseen => {
            if unseen {
                SnapshotUpdateBehavior::NewFile
            } else {
                SnapshotUpdateBehavior::InPlace
            }
        }
        SnapshotUpdate::New => SnapshotUpdateBehavior::NewFile,
        SnapshotUpdate::No => SnapshotUpdateBehavior::NoUpdate,
        SnapshotUpdate::Force => SnapshotUpdateBehavior::InPlace,
    }
}

pub enum Workspace {
    DetectWithCargo(&'static str),
    UseAsIs(&'static str),
}

/// Returns the cargo workspace path for a crate manifest, like
/// `/Users/janedoe/projects/insta` when passed
/// `/Users/janedoe/projects/insta/insta/Cargo.toml`.
///
/// If `INSTA_WORKSPACE_ROOT` environment variable is set at runtime, use the value as-is.
/// If `INSTA_WORKSPACE_ROOT` environment variable is set at compile time, use the value as-is.
/// If `INSTA_WORKSPACE_ROOT` environment variable is not set, use `cargo metadata` to find the workspace root.
pub fn get_cargo_workspace(workspace: Workspace) -> Arc<PathBuf> {
    // This is useful where CARGO_MANIFEST_DIR at compilation points to some
    // transient location. This can easily happen when building the test in one
    // directory but running it in another.
    if let Ok(workspace_root) = env::var("INSTA_WORKSPACE_ROOT") {
        return PathBuf::from(workspace_root).into();
    }

    // Distinguish if we need to run `cargo metadata`` or if we can return the workspace
    // as is.
    // This is useful if INSTA_WORKSPACE_ROOT was set at compile time, not pointing to
    // the cargo manifest directory
    let manifest_dir = match workspace {
        Workspace::UseAsIs(workspace_root) => return PathBuf::from(workspace_root).into(),
        Workspace::DetectWithCargo(manifest_dir) => manifest_dir,
    };

    let error_message = || {
        format!(
            "`cargo metadata --format-version=1 --no-deps` in path `{}`",
            manifest_dir
        )
    };

    WORKSPACES
        .lock()
        // we really do not care about poisoning here.
        .unwrap()
        .entry(manifest_dir.to_string())
        .or_insert_with(|| {
            let output = std::process::Command::new(
                env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()),
            )
            .args(["metadata", "--format-version=1", "--no-deps"])
            .current_dir(manifest_dir)
            .output()
            .unwrap_or_else(|e| panic!("failed to run {}\n\n{}", error_message(), e));

            crate::content::yaml::vendored::yaml::YamlLoader::load_from_str(
                std::str::from_utf8(&output.stdout).unwrap(),
            )
            .map_err(|e| e.to_string())
            .and_then(|docs| {
                docs.into_iter()
                    .next()
                    .ok_or_else(|| "No content found in yaml".to_string())
            })
            .and_then(|metadata| {
                metadata["workspace_root"]
                    .clone()
                    .into_string()
                    .ok_or_else(|| "Couldn't find `workspace_root`".to_string())
            })
            .map(|path| PathBuf::from(path).into())
            .unwrap_or_else(|e| {
                panic!(
                    "failed to parse cargo metadata output from {}: {}\n\n{:?}",
                    error_message(),
                    e,
                    output.stdout
                )
            })
        })
        .clone()
}

#[test]
fn test_get_cargo_workspace_manifest_dir() {
    let workspace = get_cargo_workspace(Workspace::DetectWithCargo(env!("CARGO_MANIFEST_DIR")));
    // The absolute path of the workspace should be a valid directory
    // In worktrees or other setups, the path might not end with "insta"
    // but should still be a parent of the manifest directory
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert!(manifest_dir.starts_with(&*workspace));
}

#[test]
fn test_get_cargo_workspace_insta_workspace() {
    let workspace = get_cargo_workspace(Workspace::UseAsIs("/tmp/insta_workspace_root"));
    // The absolute path of the workspace, like `/tmp/insta_workspace_root`
    assert!(workspace.ends_with("insta_workspace_root"));
}

#[cfg(feature = "_cargo_insta_internal")]
impl std::str::FromStr for TestRunner {
    type Err = ();

    fn from_str(value: &str) -> Result<TestRunner, ()> {
        match value {
            "auto" => Ok(TestRunner::Auto),
            "cargo-test" => Ok(TestRunner::CargoTest),
            "nextest" => Ok(TestRunner::Nextest),
            _ => Err(()),
        }
    }
}

#[cfg(feature = "_cargo_insta_internal")]
impl std::str::FromStr for UnreferencedSnapshots {
    type Err = ();

    fn from_str(value: &str) -> Result<UnreferencedSnapshots, ()> {
        match value {
            "auto" => Ok(UnreferencedSnapshots::Auto),
            "reject" | "error" => Ok(UnreferencedSnapshots::Reject),
            "delete" => Ok(UnreferencedSnapshots::Delete),
            "warn" => Ok(UnreferencedSnapshots::Warn),
            "ignore" => Ok(UnreferencedSnapshots::Ignore),
            _ => Err(()),
        }
    }
}

/// Memoizes a snapshot file in the reference file, as part of removing unreferenced snapshots.
pub fn memoize_snapshot_file(snapshot_file: &Path) {
    if let Ok(path) = env::var("INSTA_SNAPSHOT_REFERENCES_FILE") {
        let mut f = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .unwrap();
        f.write_all(format!("{}\n", snapshot_file.display()).as_bytes())
            .unwrap();
    }
}

fn resolve<'a>(value: &'a Content, path: &[&str]) -> Option<&'a Content> {
    path.iter()
        .try_fold(value, |node, segment| match node.resolve_inner() {
            Content::Map(fields) => fields
                .iter()
                .find(|x| x.0.as_str() == Some(segment))
                .map(|x| &x.1),
            Content::Struct(_, fields) | Content::StructVariant(_, _, _, fields) => {
                fields.iter().find(|x| x.0 == *segment).map(|x| &x.1)
            }
            _ => None,
        })
}
