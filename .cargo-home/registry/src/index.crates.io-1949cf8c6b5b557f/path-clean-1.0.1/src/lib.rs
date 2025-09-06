//! `path-clean` is a Rust port of the the `cleanname` procedure from the Plan 9 C library, and is similar to
//! [`path.Clean`](https://golang.org/pkg/path/#Clean) from the Go standard library. It works as follows:
//!
//! 1. Reduce multiple slashes to a single slash.
//! 2. Eliminate `.` path name elements (the current directory).
//! 3. Eliminate `..` path name elements (the parent directory) and the non-`.` non-`..`, element that precedes them.
//! 4. Eliminate `..` elements that begin a rooted path, that is, replace `/..` by `/` at the beginning of a path.
//! 5. Leave intact `..` elements that begin a non-rooted path.
//!
//! If the result of this process is an empty string, return the string `"."`, representing the current directory.
//!
//! It performs this transform lexically, without touching the filesystem. Therefore it doesn't do
//! any symlink resolution or absolute path resolution. For more information you can see ["Getting Dot-Dot
//! Right"](https://9p.io/sys/doc/lexnames.html).
//!
//! For convenience, the [`PathClean`] trait is exposed and comes implemented for [`std::path::{Path, PathBuf}`].
//!
//! ```rust
//! use std::path::PathBuf;
//! use path_clean::{clean, PathClean};
//! assert_eq!(clean("hello/world/.."), PathBuf::from("hello"));
//! assert_eq!(
//!     PathBuf::from("/test/../path/").clean(),
//!     PathBuf::from("/path")
//! );
//! ```
#![forbid(unsafe_code)]

use std::path::{Component, Path, PathBuf};

/// The Clean trait implements a `clean` method.
pub trait PathClean {
    fn clean(&self) -> PathBuf;
}

/// PathClean implemented for `Path`
impl PathClean for Path {
    fn clean(&self) -> PathBuf {
        clean(self)
    }
}

/// PathClean implemented for `PathBuf`
impl PathClean for PathBuf {
    fn clean(&self) -> PathBuf {
        clean(self)
    }
}

/// The core implementation. It performs the following, lexically:
/// 1. Reduce multiple slashes to a single slash.
/// 2. Eliminate `.` path name elements (the current directory).
/// 3. Eliminate `..` path name elements (the parent directory) and the non-`.` non-`..`, element that precedes them.
/// 4. Eliminate `..` elements that begin a rooted path, that is, replace `/..` by `/` at the beginning of a path.
/// 5. Leave intact `..` elements that begin a non-rooted path.
///
/// If the result of this process is an empty string, return the string `"."`, representing the current directory.
pub fn clean<P>(path: P) -> PathBuf
where
    P: AsRef<Path>,
{
    let mut out = Vec::new();

    for comp in path.as_ref().components() {
        match comp {
            Component::CurDir => (),
            Component::ParentDir => match out.last() {
                Some(Component::RootDir) => (),
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                None
                | Some(Component::CurDir)
                | Some(Component::ParentDir)
                | Some(Component::Prefix(_)) => out.push(comp),
            },
            comp => out.push(comp),
        }
    }

    if !out.is_empty() {
        out.iter().collect()
    } else {
        PathBuf::from(".")
    }
}

#[cfg(test)]
mod tests {
    use super::{clean, PathClean};
    use std::path::{Path, PathBuf};

    #[test]
    fn test_empty_path_is_current_dir() {
        assert_eq!(clean(""), PathBuf::from("."));
    }

    #[test]
    fn test_clean_paths_dont_change() {
        let tests = vec![(".", "."), ("..", ".."), ("/", "/")];

        for test in tests {
            assert_eq!(clean(test.0), PathBuf::from(test.1));
        }
    }

    #[test]
    fn test_replace_multiple_slashes() {
        let tests = vec![
            ("/", "/"),
            ("//", "/"),
            ("///", "/"),
            (".//", "."),
            ("//..", "/"),
            ("..//", ".."),
            ("/..//", "/"),
            ("/.//./", "/"),
            ("././/./", "."),
            ("path//to///thing", "path/to/thing"),
            ("/path//to///thing", "/path/to/thing"),
        ];

        for test in tests {
            assert_eq!(clean(test.0), PathBuf::from(test.1));
        }
    }

    #[test]
    fn test_eliminate_current_dir() {
        let tests = vec![
            ("./", "."),
            ("/./", "/"),
            ("./test", "test"),
            ("./test/./path", "test/path"),
            ("/test/./path/", "/test/path"),
            ("test/path/.", "test/path"),
        ];

        for test in tests {
            assert_eq!(clean(test.0), PathBuf::from(test.1));
        }
    }

    #[test]
    fn test_eliminate_parent_dir() {
        let tests = vec![
            ("/..", "/"),
            ("/../test", "/test"),
            ("test/..", "."),
            ("test/path/..", "test"),
            ("test/../path", "path"),
            ("/test/../path", "/path"),
            ("test/path/../../", "."),
            ("test/path/../../..", ".."),
            ("/test/path/../../..", "/"),
            ("/test/path/../../../..", "/"),
            ("test/path/../../../..", "../.."),
            ("test/path/../../another/path", "another/path"),
            ("test/path/../../another/path/..", "another"),
            ("../test", "../test"),
            ("../test/", "../test"),
            ("../test/path", "../test/path"),
            ("../test/..", ".."),
        ];

        for test in tests {
            assert_eq!(clean(test.0), PathBuf::from(test.1));
        }
    }

    #[test]
    fn test_pathbuf_trait() {
        assert_eq!(
            PathBuf::from("/test/../path/").clean(),
            PathBuf::from("/path")
        );
    }

    #[test]
    fn test_path_trait() {
        assert_eq!(Path::new("/test/../path/").clean(), PathBuf::from("/path"));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_windows_paths() {
        let tests = vec![
            ("\\..", "\\"),
            ("\\..\\test", "\\test"),
            ("test\\..", "."),
            ("test\\path\\..\\..\\..", ".."),
            ("test\\path/..\\../another\\path", "another\\path"), // Mixed
            ("test\\path\\my/path", "test\\path\\my\\path"),      // Mixed 2
            ("/dir\\../otherDir/test.json", "/otherDir/test.json"), // User example
            ("c:\\test\\..", "c:\\"),                             // issue #12
            ("c:/test/..", "c:/"),                                // issue #12
        ];

        for test in tests {
            assert_eq!(clean(test.0), PathBuf::from(test.1));
        }
    }
}
