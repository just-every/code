<div align="center">
  <h1><code>wasi</code></h1>

<strong>A <a href="https://bytecodealliance.org/">Bytecode Alliance</a> project</strong>

  <p>
    <strong>WASI API Bindings for Rust</strong>
  </p>

  <p>
    <a href="https://crates.io/crates/wasi"><img src="https://img.shields.io/crates/v/wasi.svg?style=flat-square" alt="Crates.io version" /></a>
    <a href="https://crates.io/crates/wasi"><img src="https://img.shields.io/crates/d/wasi.svg?style=flat-square" alt="Download" /></a>
    <a href="https://docs.rs/wasi/"><img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square" alt="docs.rs docs" /></a>
  </p>
</div>

This crate contains bindings for [WASI](https://github.com/WebAssembly/WASI)
APIs for the worlds:

* [`wasi:cli/command`]
* [`wasi:http/proxy`]

This crate is procedurally generated from [WIT] files using [`wit-bindgen`].

[`wasi:cli/command`]: https://github.com/WebAssembly/wasi-cli
[`wasi:http/proxy`]: https://github.com/WebAssembly/wasi-http
[WIT]: https://component-model.bytecodealliance.org/design/wit.html
[`wit-bindgen`]: https://github.com/bytecodealliance/wit-bindgen
[components]: https://component-model.bytecodealliance.org/
[`wasm-tools`]: https://github.com/bytecodealliance/wasm-tools

# Usage

Depending on this crate can be done by adding it to your dependencies:

```sh
$ cargo add wasi
```

Next you can use the APIs in the root of the module like so:

```rust
fn main() {
    let stdout = wasi::cli::stdout::get_stdout();
    stdout.blocking_write_and_flush(b"Hello, world!\n").unwrap();
}
```

This crate can currently be used in three main ways.

- One is to use it and compile for the [`wasm32-wasip2` target] in Rust 1.82 and later.
  This is the simplest approach, as all the tools needed are included in the
  Rust tooling, however it doesn't yet support some of the features of the
  other approaches.

- Another is to use it and compile using [`cargo component`]. This is essentially
  the same as the next option, except that `cargo component` handles most of the
  steps for you. `cargo component` also has a number of additional features for
  working with dependencies and custom WIT interfaces.

- And the third is to compile for the `wasm32-wasip1` target, and then adapt
  the resulting modules into component using `wasm-tools component new`; see
  the next section here for details.

[`wasm32-wasip2` target]: https://blog.rust-lang.org/2024/11/26/wasip2-tier-2.html
[`cargo component`]: https://github.com/bytecodealliance/cargo-component

## Building with wasm32-wasip1 and `cargo component new`.

The `wasm32-wasip2` target works with a simple `cargo build --target=wasm32-wasip2`
and doesn't need a lot of documentation here, and `cargo component` has its own
documentation, so here we have some documentation for the `wasm32-wasip1` way.

```
$ cargo build --target wasm32-wasip1
```

Next you'll want an "adapter" to convert the Rust standard library's usage of
`wasi_snapshot_preview1` to the component model. An example adapter can be found
from [Wasmtime's release page](https://github.com/bytecodealliance/wasmtime/releases/download/v17.0.0/wasi_snapshot_preview1.command.wasm).

```
$ curl -LO https://github.com/bytecodealliance/wasmtime/releases/download/v17.0.0/wasi_snapshot_preview1.command.wasm
```

Next to create a component you'll use the [`wasm-tools`] CLI to create a
component:

```
$ cargo install wasm-tools
$ wasm-tools component new target/wasm32-wasip1/debug/foo.wasm \
    --adapt ./wasi_snapshot_preview1.command.wasm \
    -o component.wasm
```

And finally the component can be run by a runtime that has Component Model
support, such as [Wasmtime]:

```
$ wasmtime run component.wasm
Hello, world!
```

[Wasmtime]: https://github.com/bytecodealliance/wasmtime

# WASIp2 vs WASIp1

In January 2024 the WASI subgroup published WASI 0.2.0, colloquially known as
"WASIp2". Around the same time the subgroup additionally decided to name the
previous iteration of WASI as "WASIp1", historically known as "WASI preview1".
This now-historical snapshot of WASI was defined with an entirely different set
of primitives and worked very differently. This crate now targets WASIp2 and no
longer targets WASIp1.

## Support for WASIp1

The last version of the `wasi` crate to support WASIp1 was the
[0.11.0+wasi-snapshot-preview1
version](https://crates.io/crates/wasi/0.11.0+wasi-snapshot-preview1). This
version of the crate supported all WASIp1 APIs. WASIp1 was historically defined
with `*.witx` files and used a bindings generator called `witx-bindgen`.

## Should I use WASIp1 or WASIp2?

This is a bit of a nuanced question/answer but the short answer is to probably
use the 0.11.0 release of `wasi` for now if you're unsure.

The longer-form answer of this is that it depends on the Rust targets that you
want to support. Rust WebAssembly targets include:

* `wasm32-unknown-unknown` - do not use this crate because this target indicates
  that WASI is not desired.
* `wasm32-wasip1` - this target has been present in Rust for quite some time and
  was previously known as `wasm32-wasi`. For this target you probably want the
  0.11.0 track of this crate.
* `wasm32-wasip2` - this target is a recent addition to rustc (as of the time of
  this writing it's not merged yet into rustc). This is what the 0.12.0 version
  of the crate is intended for.

Note that if you use `wasm32-wasip1` it's not necessarily guaranteed you want
0.11.0 of this crate. If your users are producing components then you probably
want 0.12.0 instead. If you don't know what your users are producing then you
should probably stick with 0.11.0.

Long story short, it's a bit complicated. We're in a transition period from
WASIp1 to WASIp2 and things aren't going to be perfect every step of the way, so
understanding is appreciated!

# Development

The bulk of the `wasi` crate is generated by the [`wit-bindgen`] tool. The
`src/bindings.rs` file can be regenerated with:

```
$ ./ci/regenerate.sh
```

WASI definitions are located in the `wit` directory of this repository.
Currently they're copied from upstream repositories but are hoped to be better
managed in the future.

# License

This project is triple licenced under the Apache 2/ Apache 2 with LLVM exceptions/ MIT licences. The reasoning for this is:
- Apache 2/ MIT is common in the rust ecosystem.
- Apache 2/ MIT is used in the rust standard library, and some of this code may be migrated there.
- Some of this code may be used in compiler output, and the Apache 2 with LLVM exceptions licence is useful for this.

For more details see
- [Apache 2 Licence](LICENSE-APACHE)
- [Apache 2 Licence with LLVM exceptions](LICENSE-Apache-2.0_WITH_LLVM-exception)
- [MIT Licence](LICENSE-MIT)

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache 2/ Apache 2 with LLVM exceptions/ MIT licenses,
shall be licensed as above, without any additional terms or conditions.
