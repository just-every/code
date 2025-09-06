fn main() {
    #[cfg(feature = "bindgen")]
    build();

    println!("cargo:rustc-check-cfg=cfg(io_uring_skip_arch_check)");
    println!("cargo:rustc-check-cfg=cfg(io_uring_use_own_sys)");
}

#[cfg(feature = "bindgen")]
fn build() {
    use std::env;
    use std::path::PathBuf;

    const INCLUDE: &str = r#"
#include <unistd.h>
#include <sys/syscall.h>
#include <linux/time_types.h>
#include <linux/stat.h>
#include <linux/openat2.h>
#include <linux/io_uring.h>
#include <linux/futex.h>
    "#;

    let mut builder = bindgen::Builder::default();

    if let Some(path) = env::var("BUILD_IO_URING_INCLUDE_FILE")
        .ok()
        .filter(|path| !path.is_empty())
    {
        builder = builder.header(path);
    } else {
        builder = builder.header_contents("include-file.h", INCLUDE);
    }

    #[cfg(feature = "overwrite")]
    fn output_file() -> PathBuf {
        let outdir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/sys");
        let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
        outdir.join(format!("sys_{}.rs", target_arch))
    }

    #[cfg(not(feature = "overwrite"))]
    fn output_file() -> PathBuf {
        let outdir = PathBuf::from(env::var("OUT_DIR").unwrap());
        outdir.join("sys.rs")
    }

    builder
        .ctypes_prefix("libc")
        .prepend_enum_name(false)
        .derive_default(true)
        .generate_comments(true)
        .use_core()
        .allowlist_type("io_uring_.*|io_.qring_.*|__kernel_timespec|open_how|futex_waitv")
        .allowlist_var("__NR_io_uring.*|IOSQE_.*|IORING_.*|IO_URING_.*|SPLICE_F_FD_IN_FIXED")
        .generate()
        .unwrap()
        .write_to_file(output_file())
        .unwrap();
}
