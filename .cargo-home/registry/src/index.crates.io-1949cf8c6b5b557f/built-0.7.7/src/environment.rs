use crate::util::ArrayDisplay;
use crate::{fmt_option_str, write_str_variable, write_variable};
use std::{borrow, collections, env, ffi, fmt, fs, io, process};

pub struct EnvironmentMap(collections::HashMap<String, String>);

fn get_version_from_cmd(executable: &ffi::OsStr) -> io::Result<String> {
    let output = process::Command::new(executable).arg("-V").output()?;
    let mut v = String::from_utf8(output.stdout).unwrap();
    v.pop(); // remove newline
    Ok(v)
}

impl EnvironmentMap {
    pub fn new() -> Self {
        let mut envmap = collections::HashMap::new();
        for (k, v) in env::vars_os() {
            let k = k.into_string();
            let v = v.into_string();
            if let (Ok(k), Ok(v)) = (k, v) {
                envmap.insert(k, v);
            }
        }
        Self(envmap)
    }

    pub fn write_ci(&self, mut w: &fs::File) -> io::Result<()> {
        use io::Write;

        write_variable!(
            w,
            "CI_PLATFORM",
            "Option<&str>",
            fmt_option_str(self.detect_ci()),
            "The Continuous Integration platform detected during compilation."
        );
        Ok(())
    }

    pub fn write_env(&self, mut w: &fs::File) -> io::Result<()> {
        use io::Write;
        macro_rules! write_env_str {
            ($(($name:ident, $env_name:expr,$doc:expr)),*) => {$(
                write_str_variable!(
                    w,
                    stringify!($name),
                    self.0.get($env_name)
                        .expect(stringify!(Missing expected environment variable $env_name)),
                        $doc
                );
            )*}
        }

        write_env_str!(
            (PKG_VERSION, "CARGO_PKG_VERSION", "The full version."),
            (
                PKG_VERSION_MAJOR,
                "CARGO_PKG_VERSION_MAJOR",
                "The major version."
            ),
            (
                PKG_VERSION_MINOR,
                "CARGO_PKG_VERSION_MINOR",
                "The minor version."
            ),
            (
                PKG_VERSION_PATCH,
                "CARGO_PKG_VERSION_PATCH",
                "The patch version."
            ),
            (
                PKG_VERSION_PRE,
                "CARGO_PKG_VERSION_PRE",
                "The pre-release version."
            ),
            (
                PKG_AUTHORS,
                "CARGO_PKG_AUTHORS",
                "A colon-separated list of authors."
            ),
            (PKG_NAME, "CARGO_PKG_NAME", "The name of the package."),
            (PKG_DESCRIPTION, "CARGO_PKG_DESCRIPTION", "The description."),
            (PKG_HOMEPAGE, "CARGO_PKG_HOMEPAGE", "The homepage."),
            (PKG_LICENSE, "CARGO_PKG_LICENSE", "The license."),
            (
                PKG_REPOSITORY,
                "CARGO_PKG_REPOSITORY",
                "The source repository as advertised in Cargo.toml."
            ),
            (
                TARGET,
                "TARGET",
                "The target triple that was being compiled for."
            ),
            (HOST, "HOST", "The host triple of the rust compiler."),
            (
                PROFILE,
                "PROFILE",
                "`release` for release builds, `debug` for other builds."
            ),
            (RUSTC, "RUSTC", "The compiler that cargo resolved to use."),
            (
                RUSTDOC,
                "RUSTDOC",
                "The documentation generator that cargo resolved to use."
            )
        );
        write_str_variable!(
            w,
            "OPT_LEVEL",
            env::var("OPT_LEVEL").unwrap(),
            "Value of OPT_LEVEL for the profile used during compilation."
        );
        write_variable!(
            w,
            "NUM_JOBS",
            "u32",
            if env::var(crate::SOURCE_DATE_EPOCH).is_ok() {
                borrow::Cow::Borrowed("1")
            } else {
                borrow::Cow::Owned(env::var("NUM_JOBS").unwrap())
            },
            "The parallelism that was specified during compilation."
        );
        write_variable!(
            w,
            "DEBUG",
            "bool",
            env::var("DEBUG").unwrap() == "true",
            "Value of DEBUG for the profile used during compilation."
        );
        Ok(())
    }

    pub fn write_features(&self, mut w: &fs::File) -> io::Result<()> {
        use io::Write;

        let mut features = Vec::new();
        for name in self.0.keys() {
            if let Some(feat) = name.strip_prefix("CARGO_FEATURE_") {
                features.push(feat.to_owned());
            }
        }
        features.sort_unstable();

        write_variable!(
            w,
            "FEATURES",
            format_args!("[&str; {}]", features.len()),
            ArrayDisplay(&features, |t, f| write!(f, "\"{}\"", t.escape_default())),
            "The features that were enabled during compilation."
        );
        let features_str = features.join(", ");
        write_str_variable!(
            w,
            "FEATURES_STR",
            features_str,
            "The features as a comma-separated string."
        );

        let mut lowercase_features = features
            .iter()
            .map(|name| name.to_lowercase())
            .collect::<Vec<_>>();
        lowercase_features.sort_unstable();

        write_variable!(
            w,
            "FEATURES_LOWERCASE",
            format_args!("[&str; {}]", lowercase_features.len()),
            ArrayDisplay(&lowercase_features, |val, fmt| write!(
                fmt,
                "\"{}\"",
                val.escape_default()
            )),
            "The features as above, as lowercase strings."
        );
        let lowercase_features_str = lowercase_features.join(", ");
        write_str_variable!(
            w,
            "FEATURES_LOWERCASE_STR",
            lowercase_features_str,
            "The feature-string as above, from lowercase strings."
        );

        Ok(())
    }

    pub fn write_cfg(&self, mut w: &fs::File) -> io::Result<()> {
        use io::Write;

        write_str_variable!(
            w,
            "CFG_TARGET_ARCH",
            self.0["CARGO_CFG_TARGET_ARCH"],
            "The target architecture, given by `CARGO_CFG_TARGET_ARCH`."
        );

        write_str_variable!(
            w,
            "CFG_ENDIAN",
            self.0["CARGO_CFG_TARGET_ENDIAN"],
            "The endianness, given by `CARGO_CFG_TARGET_ENDIAN`."
        );

        write_str_variable!(
            w,
            "CFG_ENV",
            self.0["CARGO_CFG_TARGET_ENV"],
            "The toolchain-environment, given by `CARGO_CFG_TARGET_ENV`."
        );

        write_str_variable!(
            w,
            "CFG_FAMILY",
            self.0
                .get("CARGO_CFG_TARGET_FAMILY")
                .map(|s| s.as_str())
                .unwrap_or_default(),
            "The OS-family, given by `CARGO_CFG_TARGET_FAMILY`."
        );

        write_str_variable!(
            w,
            "CFG_OS",
            self.0["CARGO_CFG_TARGET_OS"],
            "The operating system, given by `CARGO_CFG_TARGET_OS`."
        );

        write_str_variable!(
            w,
            "CFG_POINTER_WIDTH",
            self.0["CARGO_CFG_TARGET_POINTER_WIDTH"],
            "The pointer width, given by `CARGO_CFG_TARGET_POINTER_WIDTH`."
        );

        Ok(())
    }

    pub fn write_compiler_version(&self, mut w: &fs::File) -> io::Result<()> {
        use std::io::Write;

        let rustc = &self.0["RUSTC"];
        let rustdoc = &self.0["RUSTDOC"];

        let rustc_version = get_version_from_cmd(rustc.as_ref())?;
        let rustdoc_version = get_version_from_cmd(rustdoc.as_ref()).unwrap_or_default();

        write_str_variable!(
            w,
            "RUSTC_VERSION",
            rustc_version,
            format_args!("The output of `{rustc} -V`")
        );

        write_str_variable!(
            w,
            "RUSTDOC_VERSION",
            rustdoc_version,
            format_args!(
                "The output of `{rustdoc} -V`; empty string if `{rustdoc} -V` failed to execute"
            )
        );
        Ok(())
    }

    pub fn detect_ci(&self) -> Option<CIPlatform> {
        macro_rules! detect {
            ($(($k:expr, $v:expr, $i:ident)),*) => {$(
                    if self.0.get($k).map_or(false, |v| v == $v) {
                        return Some(CIPlatform::$i);
                    }
                    )*};
            ($(($k:expr, $i:ident)),*) => {$(
                    if self.0.contains_key($k) {
                        return Some(CIPlatform::$i);
                    }
                    )*};
            ($($k:expr),*) => {$(
                if self.0.contains_key($k) {
                    return Some(CIPlatform::Generic);
                }
            )*};
        }
        // Variable names collected by watson/ci-info
        detect!(
            ("TRAVIS", Travis),
            ("CIRCLECI", Circle),
            ("GITLAB_CI", GitLab),
            ("APPVEYOR", AppVeyor),
            ("DRONE", Drone),
            ("MAGNUM", Magnum),
            ("SEMAPHORE", Semaphore),
            ("JENKINS_URL", Jenkins),
            ("bamboo_planKey", Bamboo),
            ("TF_BUILD", TFS),
            ("TEAMCITY_VERSION", TeamCity),
            ("BUILDKITE", Buildkite),
            ("HUDSON_URL", Hudson),
            ("GO_PIPELINE_LABEL", GoCD),
            ("BITBUCKET_COMMIT", BitBucket),
            ("GITHUB_ACTIONS", GitHubActions)
        );

        if self.0.contains_key("TASK_ID") && self.0.contains_key("RUN_ID") {
            return Some(CIPlatform::TaskCluster);
        }

        detect!(("CI_NAME", "codeship", Codeship));

        detect!(
            "CI",                     // Could be Travis, Circle, GitLab, AppVeyor or CodeShip
            "CONTINUOUS_INTEGRATION", // Probably Travis
            "BUILD_NUMBER"            // Jenkins, TeamCity
        );
        None
    }
}

/// Various Continuous Integration platforms whose presence can be detected.
pub enum CIPlatform {
    /// <https://travis-ci.org>
    Travis,
    /// <https://circleci.com>
    Circle,
    /// <https://about.gitlab.com/gitlab-ci>
    GitLab,
    /// <https://www.appveyor.com>
    AppVeyor,
    /// <https://codeship.com>
    Codeship,
    /// <https://github.com/drone/drone>
    Drone,
    /// <https://magnum-ci.com>
    Magnum,
    /// <https://semaphoreci.com>
    Semaphore,
    /// <https://jenkins.io>
    Jenkins,
    /// <https://www.atlassian.com/software/bamboo>
    Bamboo,
    /// <https://www.visualstudio.com/de/tfs>
    TFS,
    /// <https://www.jetbrains.com/teamcity>
    TeamCity,
    /// <https://buildkite.com>
    Buildkite,
    /// <http://hudson-ci.org>
    Hudson,
    /// <https://github.com/taskcluster>
    TaskCluster,
    /// <https://www.gocd.io>
    GoCD,
    /// <https://bitbucket.org>
    BitBucket,
    /// <https://github.com/features/actions>
    GitHubActions,
    /// Unspecific
    Generic,
}

impl fmt::Display for CIPlatform {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match *self {
            CIPlatform::Travis => "Travis CI",
            CIPlatform::Circle => "CircleCI",
            CIPlatform::GitLab => "GitLab",
            CIPlatform::AppVeyor => "AppVeyor",
            CIPlatform::Codeship => "CodeShip",
            CIPlatform::Drone => "Drone",
            CIPlatform::Magnum => "Magnum",
            CIPlatform::Semaphore => "Semaphore",
            CIPlatform::Jenkins => "Jenkins",
            CIPlatform::Bamboo => "Bamboo",
            CIPlatform::TFS => "Team Foundation Server",
            CIPlatform::TeamCity => "TeamCity",
            CIPlatform::Buildkite => "Buildkite",
            CIPlatform::Hudson => "Hudson",
            CIPlatform::TaskCluster => "TaskCluster",
            CIPlatform::GoCD => "GoCD",
            CIPlatform::BitBucket => "BitBucket",
            CIPlatform::GitHubActions => "GitHub Actions",
            CIPlatform::Generic => "Generic CI",
        })
    }
}
