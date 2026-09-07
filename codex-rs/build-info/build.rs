//! Embed the compilation target, including its architecture and ABI.

fn main() -> Result<(), std::env::VarError> {
    let target = std::env::var("TARGET")?;
    println!("cargo:rustc-env=CODEX_BUILD_TARGET={target}");
    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}
