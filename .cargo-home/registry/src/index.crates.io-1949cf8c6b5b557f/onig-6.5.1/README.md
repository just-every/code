``# Rust Onig

[![Cargo](https://img.shields.io/crates/v/onig.svg)](
https://crates.io/crates/onig)
[![Documentation](https://docs.rs/onig/badge.svg)](
https://docs.rs/onig)
![CI](https://github.com/rust-onig/rust-onig/workflows/CI/badge.svg)
[![Build status](https://ci.appveyor.com/api/projects/status/7qxdb44xpw4bkjfi/branch/main?svg=true)](https://ci.appveyor.com/project/iwillspeak/rust-onig/branch/main)
[![dependency status](https://deps.rs/crate/onig/6.4.0/status.svg)](https://deps.rs/crate/onig/6.4.0)

Rust bindings for the [Oniguruma regex library][Onig_wiki], a powerful and mature regular expression library with support for a wide range of character sets and language syntaxes. Oniguruma is written in C. This repository provides two crates: `onig-sys` which provides the raw Rust FFI bindings, and `onig`, which provides a safe Rust wrapper around them.

## Documentation

Check out the [module documentation][onig_crate_doc] to find out all the features that are available. To see some example usage of this crate take a look a the [examples folder][examples_folder]. The examples can be run from the command line with `cargo run --example <examplename>`.

## Getting Started

Add the following to your `Cargo.toml` file:

```toml
[dependencies]
onig = "6"
```

Add the following extern to your crate root if you are not using edition 2018:

```rust
extern crate onig;
```

You can compile simple regular expressions with [`Regex::new`][regex_new], check if the pattern matches an entire `&str` with [`Regex::is_match`][regex_is_match] and find matches within a `&str` with [`Regex::find`][regex_find]. The `onig` crate also supplies more powerful versions of these methods which expose the wide range of options Oniguruma provides.

```rust
use onig::*;

let regex = Regex::new("e(l+)").unwrap();
for (i, pos) in regex.captures("hello").unwrap().iter_pos().enumerate() {
    match pos {
         Some((beg, end)) =>
             println!("Group {} captured in position {}:{}", i, beg, end),
         None =>
             println!("Group {} is not captured", i)
    }
}
```

## Linking

If a version of Oniguruma can be found by `pkg-config` then that will be used. If not then Oniguruma will be compiled from source and linked to the `onig-sys` crate.

By default `rust-onig` will be statically linked to `libonig`. If you would rather that dynamic linking is used then the environment variables `RUSTONIG_STATIC_LIBONIG` and `RUSTONIG_DYNAMIC_LIBONIG` can be set. On *nix:

    $ RUSTONIG_DYNAMIC_LIBONIG=1 cargo build

Or Windows:

    > set RUSTONIG_DYNAMIC_LIBONIG=1
    > cargo build

## Build errors caused by libclang/llvm

By default `onig` uses `bindgen` to generate bindings for libonig. If you plan to only use the bundled version of libonig, you can make compilation faster and more reliable by disabling the default `generate` feature:

```toml
[dependencies]
onig = { version = "6", default-features = false }
```

## Debugging

Sometimes it's useful to debug how Oniguruma parses, compiles, optimizes or
executes a particular pattern.

When activating the `print-debug` feature for this crate, Oniguruma is compiled
with debugging. Note that it's a compile-time setting, so you also need to make
`rust-onig` not use the system Oniguruma by using `RUSTONIG_SYSTEM_LIBONIG`.

With all that combined, here's an example command to debug the pattern `a|b`:

    RUSTONIG_SYSTEM_LIBONIG=0 cargo run --features print-debug --example capturedump 'a|b'

## Supported Rust Versions

Rust Onig supports Rust 1.70.0 or later (2021 edition) for Windows, Linux, and
macOS. If the minimum supported rust version (MSRV) is changed then the minor
version number will be increased. That is v6.5.x should always compile
with the same version of the compiler.

## Rust-Onig is Open Source

The contents of this repository are distributed under the MIT license. See
[LICENSE](LICENSE.md) for more details. If you'd like to contribute take a look
at our open [easy issues][easy_issues].

 [Onig_wiki]: https://en.wikipedia.org/wiki/Oniguruma
 [onig_crate_doc]: https://docs.rs/onig/
 [examples_folder]: https://github.com/rust-onig/rust-onig/tree/main/onig/examples
 [regex_new]: https://docs.rs/onig/6.4.0/onig/struct.Regex.html#method.new
 [regex_is_match]: https://docs.rs/onig/6.4.0/onig/struct.Regex.html#method.is_match
 [regex_find]: https://docs.rs/onig/6.4.0/onig/struct.Regex.html#method.find
 [easy_issues]: https://github.com/rust-onig/rust-onig/issues?q=is%3Aopen+is%3Aissue+label%3AE-Easy
