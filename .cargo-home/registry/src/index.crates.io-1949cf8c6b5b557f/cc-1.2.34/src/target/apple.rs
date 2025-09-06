use super::TargetInfo;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum AppleEnv {
    Simulator,
    MacCatalyst,
}
pub(crate) use AppleEnv::*;

impl TargetInfo<'_> {
    pub(crate) fn get_apple_env(&self) -> Option<AppleEnv> {
        match (self.env, self.abi) {
            ("sim", _) | (_, "sim") => Some(Simulator),
            ("macabi", _) | (_, "macabi") => Some(MacCatalyst),
            _ => None,
        }
    }

    pub(crate) fn apple_sdk_name(&self) -> &'static str {
        match (self.os, self.get_apple_env()) {
            ("macos", None) => "macosx",
            ("ios", None) => "iphoneos",
            ("ios", Some(Simulator)) => "iphonesimulator",
            ("ios", Some(MacCatalyst)) => "macosx",
            ("tvos", None) => "appletvos",
            ("tvos", Some(Simulator)) => "appletvsimulator",
            ("watchos", None) => "watchos",
            ("watchos", Some(Simulator)) => "watchsimulator",
            ("visionos", None) => "xros",
            ("visionos", Some(Simulator)) => "xrsimulator",
            (os, _) => panic!("invalid Apple target OS {}", os),
        }
    }

    pub(crate) fn apple_version_flag(&self, min_version: &str) -> String {
        // There are many aliases for these, and `-mtargetos=` is preferred on Clang nowadays, but
        // for compatibility with older Clang, we use the earliest supported name here.
        //
        // NOTE: GCC does not support `-miphoneos-version-min=` etc. (because it does not support
        // iOS in general), but we specify them anyhow in case we actually have a Clang-like
        // compiler disguised as a GNU-like compiler, or in case GCC adds support for these in the
        // future.
        //
        // See also:
        // https://clang.llvm.org/docs/ClangCommandLineReference.html#cmdoption-clang-mmacos-version-min
        // https://clang.llvm.org/docs/AttributeReference.html#availability
        // https://gcc.gnu.org/onlinedocs/gcc/Darwin-Options.html#index-mmacosx-version-min
        match (self.os, self.get_apple_env()) {
            ("macos", None) => format!("-mmacosx-version-min={min_version}"),
            ("ios", None) => format!("-miphoneos-version-min={min_version}"),
            ("ios", Some(Simulator)) => format!("-mios-simulator-version-min={min_version}"),
            ("ios", Some(MacCatalyst)) => format!("-mtargetos=ios{min_version}-macabi"),
            ("tvos", None) => format!("-mappletvos-version-min={min_version}"),
            ("tvos", Some(Simulator)) => format!("-mappletvsimulator-version-min={min_version}"),
            ("watchos", None) => format!("-mwatchos-version-min={min_version}"),
            ("watchos", Some(Simulator)) => format!("-mwatchsimulator-version-min={min_version}"),
            // `-mxros-version-min` does not exist
            // https://github.com/llvm/llvm-project/issues/88271
            ("visionos", None) => format!("-mtargetos=xros{min_version}"),
            ("visionos", Some(Simulator)) => format!("-mtargetos=xros{min_version}-simulator"),
            (os, _) => panic!("invalid Apple target OS {}", os),
        }
    }
}
