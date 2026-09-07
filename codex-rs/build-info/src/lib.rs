//! Resolve the packaged release version, compiled commit, and target of the current runtime.

use std::fmt;
use std::sync::OnceLock;

use codex_install_context::InstallContext;
use semver::Version;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

static BUILD_INFO: OnceLock<BuildInfo> = OnceLock::new();

/// Initialize build information from the commit stamped into the calling executable.
///
/// The environment lookup intentionally expands at the macro call site so Git
/// changes invalidate only final binary actions, not this shared library.
/// Cargo builds must supply STABLE_GIT_COMMIT in the build environment.
#[macro_export]
macro_rules! initialize {
    () => {
        $crate::BuildInfo::initialize(option_env!("STABLE_GIT_COMMIT").unwrap_or("dev"));
    };
}

/// The packaged release version and build provenance for the current runtime.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildInfo {
    version: Version,
    build_commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target: Option<String>,
}

impl BuildInfo {
    /// Return build information for the current Codex runtime.
    pub fn get() -> Self {
        BUILD_INFO
            .get_or_init(|| Self::resolve(InstallContext::current(), "dev"))
            .clone()
    }

    /// Initialize build information using the final executable's stamped commit.
    ///
    /// Keeping the stamp in the executable prevents Git operations from
    /// invalidating the shared Rust library graph.
    #[doc(hidden)]
    pub fn initialize(build_commit: &'static str) {
        let _ = BUILD_INFO.get_or_init(|| Self::resolve(InstallContext::current(), build_commit));
    }

    /// Recover structured release information from a persisted version string.
    pub fn from_version(version: impl Into<String>) -> Self {
        let version = version.into();
        match Version::parse(&version) {
            Ok(parsed_version) => Self {
                build_commit: if parsed_version.major == 0
                    && parsed_version.minor == 0
                    && parsed_version.patch == 0
                {
                    version
                } else {
                    "unknown".to_string()
                },
                version: parsed_version,
                target: None,
            },
            Err(_) => Self {
                version: Version::new(0, 0, 0),
                build_commit: version,
                target: None,
            },
        }
    }

    /// Return the parsed package version, or `0.0.0` for a source build.
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Format the version for a user-facing Codex header.
    pub fn display_version(&self) -> String {
        if self.build_commit == "dev" {
            "dev".to_string()
        } else if self.is_source_build() {
            format!("v{}", self.build_commit)
        } else {
            format!("v{}", self.version)
        }
    }

    /// Identify source builds without parsing their displayed Git commit.
    pub fn is_source_build(&self) -> bool {
        self.version.major == 0 && self.version.minor == 0 && self.version.patch == 0
    }

    /// Return the Git commit stamped into the final executable.
    pub fn build_commit(&self) -> &str {
        &self.build_commit
    }

    /// Return the compiler target triple, or `None` for historical metadata without a target.
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    fn resolve(install_context: &InstallContext, build_commit: &'static str) -> Self {
        if let Some(manifest) = install_context.package_manifest() {
            return Self {
                version: manifest.version,
                build_commit: build_commit.to_owned(),
                target: Some(env!("CODEX_BUILD_TARGET").to_owned()),
            };
        }

        Self {
            version: Version::new(0, 0, 0),
            build_commit: build_commit.to_owned(),
            target: Some(env!("CODEX_BUILD_TARGET").to_owned()),
        }
    }
}

/// Derive a standard build's opaque ID from its stamped commit and compiler target.
///
/// Explicit inputs let CI identify cross-compiled builds without running them.
/// Runtime callers must use the running executable's stamp and target. The stable
/// encoding is SHA-256 of `git:<full lowercase commit>:<target>`, excluding version.
/// This identifies a standard build configuration, not exact executable bytes.
pub fn build_id(build_commit: &str, target: &str) -> Option<String> {
    if build_commit.len() != 40
        || !build_commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        || target.is_empty()
    {
        return None;
    }
    let identity = format!("git:{}:{target}", build_commit.to_ascii_lowercase());
    let digest = Sha256::digest(identity.as_bytes());
    Some(format!("sha256:{digest:x}"))
}

impl fmt::Display for BuildInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_source_build() {
            formatter.write_str(&self.build_commit)
        } else {
            self.version.fmt(formatter)
        }
    }
}

#[cfg(test)]
#[path = "build_info_tests.rs"]
mod tests;
