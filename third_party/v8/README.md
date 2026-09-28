# `rusty_v8` Consumer Artifacts

This directory wires the `v8` crate to exact-version Bazel inputs.
Bazel consumer builds use:

- Codex-published sandbox archive/binding pairs on Darwin and GNU Linux (x64
  and arm64), with checksums pinned from the trusted release manifests
- the existing Codex-published Windows MSVC archives
- source-built V8 archives on musl Linux and Windows GNU

Local Cargo builds still use upstream prebuilt `rusty_v8` archives by default.
Selected Cargo CI, release, and package builds override
`RUSTY_V8_ARCHIVE`/`RUSTY_V8_SRC_BINDING_PATH` with Codex release assets. Bazel
sets those variables independently in `MODULE.bazel`, selecting the pair above
for its consumers. All Bazel compilation modes use the same published V8
release archive on supported platforms.

The Bazel `v8` crate feature selection enables V8's in-process sandbox.
Darwin/GNU consumers select prebuilts only when both the V8 sandbox and pointer
compression settings match the published artifact. For source instrumentation
or custom V8 C++ settings, use `--//:rusty_v8_from_source=true`; the archive
and binding then both come from the source path. The published release archive
cannot incorporate local V8 C++ flags or native debug/sanitizer settings.

Current pinned versions:

- Rust crate: `v8 = =146.4.0`
- Embedded upstream V8 source for musl release builds: `14.6.202.9`

When bumping the Rust crate version, keep the checked-in checksum manifest and
`MODULE.bazel` in sync:

```bash
python3 .github/scripts/rusty_v8_bazel.py update-module-bazel
python3 .github/scripts/rusty_v8_bazel.py check-module-bazel
```

The commands read `third_party/v8/rusty_v8_<crate_version>.sha256` by default
and validate every matching `rusty_v8_<crate_version>` `http_file` entry.
CI runs the check command to block checksum drift.

The consumer-facing selectors are:

- `//third_party/v8:rusty_v8_archive_for_target`
- `//third_party/v8:rusty_v8_binding_for_target`

Musl release assets are expected at the tag:

- `rusty-v8-v<crate_version>`

with these raw asset names:

- `librusty_v8_release_<target>.a.gz`
- `src_binding_release_<target>.rs`

The dedicated publishing workflow is `.github/workflows/rusty-v8-release.yml`.
It builds musl release pairs from source and keeps the release artifacts as the
statically linked form:

- `//third_party/v8:rusty_v8_release_pair_x86_64_unknown_linux_musl`
- `//third_party/v8:rusty_v8_release_pair_aarch64_unknown_linux_musl`

Cargo musl builds use `RUSTY_V8_ARCHIVE` plus a downloaded
`RUSTY_V8_SRC_BINDING_PATH` to point at those `openai/codex` release assets
directly. We do not use `RUSTY_V8_MIRROR` for musl because the upstream `v8`
crate hardcodes a `v<crate_version>` tag layout, while our musl artifacts are
published under `rusty-v8-v<crate_version>`.

Do not mix artifacts across crate versions. The archive and binding must match
the exact resolved `v8` crate version in `codex-rs/Cargo.lock`.
