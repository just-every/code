#![allow(
    non_camel_case_types,
    non_upper_case_globals,
    dead_code,
    non_snake_case,
    unused_qualifications
)]
#![allow(
    clippy::unreadable_literal,
    clippy::missing_safety_doc,
    clippy::non_canonical_clone_impl
)]

use std::io;

use libc::*;

#[cfg(all(
    not(feature = "bindgen"),
    not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "loongarch64",
        target_arch = "powerpc64"
    )),
    not(io_uring_skip_arch_check)
))]
compile_error!(
    "The prebuilt `sys.rs` may not be compatible with your target,
please use bindgen feature to generate new `sys.rs` of your arch
or use `--cfg=io_uring_skip_arch_check` to skip the check."
);

cfg_if::cfg_if! {
    if #[cfg(io_uring_use_own_sys)] {
        include!(env!("IO_URING_OWN_SYS_BINDING"));
    } else if #[cfg(all(feature = "bindgen", not(feature = "overwrite")))] {
        include!(concat!(env!("OUT_DIR"), "/sys.rs"));
    } else {
        include!("sys.rs");
    }
}

#[cfg(feature = "bindgen")]
const SYSCALL_REGISTER: c_long = __NR_io_uring_register as _;

#[cfg(not(feature = "bindgen"))]
const SYSCALL_REGISTER: c_long = libc::SYS_io_uring_register;

#[cfg(feature = "bindgen")]
const SYSCALL_SETUP: c_long = __NR_io_uring_setup as _;

#[cfg(not(feature = "bindgen"))]
const SYSCALL_SETUP: c_long = libc::SYS_io_uring_setup;

#[cfg(feature = "bindgen")]
const SYSCALL_ENTER: c_long = __NR_io_uring_enter as _;

#[cfg(not(feature = "bindgen"))]
const SYSCALL_ENTER: c_long = libc::SYS_io_uring_enter;

#[cfg(feature = "direct-syscall")]
fn to_result(ret: c_int) -> io::Result<c_int> {
    if ret >= 0 {
        Ok(ret)
    } else {
        Err(io::Error::from_raw_os_error(-ret))
    }
}

#[cfg(not(feature = "direct-syscall"))]
fn to_result(ret: c_int) -> io::Result<c_int> {
    if ret >= 0 {
        Ok(ret)
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(feature = "direct-syscall"))]
pub unsafe fn io_uring_register(
    fd: c_int,
    opcode: c_uint,
    arg: *const c_void,
    nr_args: c_uint,
) -> io::Result<c_int> {
    to_result(syscall(
        SYSCALL_REGISTER,
        fd as c_long,
        opcode as c_long,
        arg as c_long,
        nr_args as c_long,
    ) as _)
}

#[cfg(feature = "direct-syscall")]
pub unsafe fn io_uring_register(
    fd: c_int,
    opcode: c_uint,
    arg: *const c_void,
    nr_args: c_uint,
) -> io::Result<c_int> {
    to_result(sc::syscall4(
        SYSCALL_REGISTER as usize,
        fd as usize,
        opcode as usize,
        arg as usize,
        nr_args as usize,
    ) as _)
}

#[cfg(not(feature = "direct-syscall"))]
pub unsafe fn io_uring_setup(entries: c_uint, p: *mut io_uring_params) -> io::Result<c_int> {
    to_result(syscall(SYSCALL_SETUP, entries as c_long, p as c_long) as _)
}

#[cfg(feature = "direct-syscall")]
pub unsafe fn io_uring_setup(entries: c_uint, p: *mut io_uring_params) -> io::Result<c_int> {
    to_result(sc::syscall2(SYSCALL_SETUP as usize, entries as usize, p as usize) as _)
}

#[cfg(not(feature = "direct-syscall"))]
pub unsafe fn io_uring_enter(
    fd: c_int,
    to_submit: c_uint,
    min_complete: c_uint,
    flags: c_uint,
    arg: *const libc::c_void,
    size: usize,
) -> io::Result<c_int> {
    to_result(syscall(
        SYSCALL_ENTER,
        fd as c_long,
        to_submit as c_long,
        min_complete as c_long,
        flags as c_long,
        arg as c_long,
        size as c_long,
    ) as _)
}

#[cfg(feature = "direct-syscall")]
pub unsafe fn io_uring_enter(
    fd: c_int,
    to_submit: c_uint,
    min_complete: c_uint,
    flags: c_uint,
    arg: *const libc::c_void,
    size: usize,
) -> io::Result<c_int> {
    to_result(sc::syscall6(
        SYSCALL_ENTER as usize,
        fd as usize,
        to_submit as usize,
        min_complete as usize,
        flags as usize,
        arg as usize,
        size,
    ) as _)
}
