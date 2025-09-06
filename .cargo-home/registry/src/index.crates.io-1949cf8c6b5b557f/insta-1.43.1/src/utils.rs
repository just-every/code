use std::{
    borrow::Cow,
    env,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

/// Are we running in in a CI environment?
pub fn is_ci() -> bool {
    match env::var("CI").ok().as_deref() {
        Some("false") | Some("0") | Some("") => false,
        None => env::var("TF_BUILD").is_ok(),
        Some(_) => true,
    }
}

#[cfg(feature = "colors")]
pub use console::style;

#[cfg(not(feature = "colors"))]
mod fake_colors {
    pub struct FakeStyledObject<D>(D);

    macro_rules! style_attr {
        ($($name:ident)*) => {
            $(
                #[inline]
                pub fn $name(self) -> FakeStyledObject<D> { self }
            )*
        }
    }

    impl<D> FakeStyledObject<D> {
        style_attr!(red green yellow cyan bold dim underlined);
    }

    impl<D: std::fmt::Display> std::fmt::Display for FakeStyledObject<D> {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            std::fmt::Display::fmt(&self.0, f)
        }
    }

    pub fn style<D>(val: D) -> FakeStyledObject<D> {
        FakeStyledObject(val)
    }
}

#[cfg(not(feature = "colors"))]
pub use self::fake_colors::*;

/// Returns the term width that insta should use.
pub fn term_width() -> usize {
    #[cfg(feature = "colors")]
    {
        console::Term::stdout().size().1 as usize
    }
    #[cfg(not(feature = "colors"))]
    {
        74
    }
}

/// Converts a path into a string that can be persisted.
pub fn path_to_storage(path: &Path) -> String {
    #[cfg(windows)]
    {
        path.to_str().unwrap().replace('\\', "/")
    }

    #[cfg(not(windows))]
    {
        path.to_string_lossy().into()
    }
}

/// Tries to format a given rust expression with rustfmt
pub fn format_rust_expression(value: &str) -> Cow<'_, str> {
    const PREFIX: &str = "const x:() = ";
    const SUFFIX: &str = ";\n";
    if let Ok(mut proc) = Command::new("rustfmt")
        .arg("--emit=stdout")
        .arg("--edition=2018")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        {
            let stdin = proc.stdin.as_mut().unwrap();
            stdin.write_all(PREFIX.as_bytes()).unwrap();
            stdin.write_all(value.as_bytes()).unwrap();
            stdin.write_all(SUFFIX.as_bytes()).unwrap();
        }
        if let Ok(output) = proc.wait_with_output() {
            if output.status.success() {
                // slice between after the prefix and before the suffix
                // (currently 14 from the start and 2 before the end, respectively)
                let start = PREFIX.len() + 1;
                let end = output.stdout.len() - SUFFIX.len();
                return std::str::from_utf8(&output.stdout[start..end])
                    .unwrap()
                    .replace("\r\n", "\n")
                    .into();
            }
        }
    }
    Cow::Borrowed(value)
}

#[cfg(feature = "_cargo_insta_internal")]
pub fn get_cargo() -> std::ffi::OsString {
    let cargo = env::var_os("CARGO");
    let cargo = cargo
        .as_deref()
        .unwrap_or_else(|| std::ffi::OsStr::new("cargo"));
    cargo.to_os_string()
}

#[test]
fn test_format_rust_expression() {
    use crate::assert_snapshot;
    assert_snapshot!(format_rust_expression("vec![1,2,3]"), @"vec![1, 2, 3]");
    assert_snapshot!(format_rust_expression("vec![1,2,3].iter()"), @"vec![1, 2, 3].iter()");
    assert_snapshot!(format_rust_expression(r#"    "aoeu""#), @r#""aoeu""#);
    assert_snapshot!(format_rust_expression(r#"  "aoe😄""#), @r#""aoe😄""#);
    assert_snapshot!(format_rust_expression("😄😄😄😄😄"), @"😄😄😄😄😄")
}
