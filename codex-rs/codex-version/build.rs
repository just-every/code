fn main() {
    // Prefer an explicit CODE_VERSION provided by CI; fall back to the
    // crate's package version to keep local builds sane.
    let version =
        std::env::var("CODE_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());

    // Inject the version as a rustc env so it participates in the compiler
    // invocation hash (sccache-friendly) and guarantees a cache miss when
    // the version changes.
    println!("cargo:rustc-env=CODE_VERSION={}", version);

    // Ensure dependent crates rebuild when CODE_VERSION changes even if the
    // source graph stays the same.
    println!("cargo:rerun-if-env-changed=CODE_VERSION");
}
