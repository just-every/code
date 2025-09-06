# path-clean

[![crates.io version][1]][2]
[![build status][3]][4]
[![docs.rs docs][5]][6]
[![license][7]][8]

## Installation

```sh
cargo add path-clean
```

## Usage

```rust
use std::path::PathBuf;
use path_clean::{clean, PathClean};
assert_eq!(clean("hello/world/.."), PathBuf::from("hello"));
assert_eq!(
  PathBuf::from("/test/../path/").clean(),
  PathBuf::from("/path")
);
```

## About

`path-clean` is a Rust port of the the `cleanname` procedure from the Plan 9 C library, and is similar to [`path.Clean`](https://golang.org/pkg/path/#Clean) from the Go standard library. It works as follows:

  1. Reduce multiple slashes to a single slash.
  2. Eliminate `.` path name elements (the current directory).
  3. Eliminate `..` path name elements (the parent directory) and the non-`.` non-`..`, element that precedes them.
  4. Eliminate `..` elements that begin a rooted path, that is, replace `/..` by `/` at the beginning of a path.
  5. Leave intact `..` elements that begin a non-rooted path.

If the result of this process is an empty string, return the string `"."`, representing the current directory.

It performs this transform lexically, without touching the filesystem. Therefore it doesn't do any symlink resolution or absolute path resolution. For more information you can see ["Getting Dot-Dot Right"](https://9p.io/sys/doc/lexnames.html).

For convenience, the [`PathClean`] trait is exposed and comes implemented for [`std::path::PathBuf`].

## License
[MIT](./LICENSE-MIT) OR [Apache-2.0](./LICENSE-APACHE)


[1]: https://img.shields.io/crates/v/path-clean.svg?style=flat-square
[2]: https://crates.io/crates/path-clean
[3]: https://img.shields.io/github/actions/workflow/status/danreeves/path-clean/ci.yml?style=flat-square
[4]: https://github.com/danreeves/path-clean/actions
[5]: https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square
[6]: https://docs.rs/path-clean
[7]: https://img.shields.io/crates/l/path-clean.svg?style=flat-square
[8]: #license

