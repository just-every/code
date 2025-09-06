//! Common Linux types not provided by libc.

pub(crate) mod sealed {
    use super::{Fd, Fixed};
    use std::os::unix::io::RawFd;

    #[derive(Debug)]
    pub enum Target {
        Fd(RawFd),
        Fixed(u32),
    }

    pub trait UseFd: Sized {
        fn into(self) -> RawFd;
    }

    pub trait UseFixed: Sized {
        fn into(self) -> Target;
    }

    impl UseFd for Fd {
        #[inline]
        fn into(self) -> RawFd {
            self.0
        }
    }

    impl UseFixed for Fd {
        #[inline]
        fn into(self) -> Target {
            Target::Fd(self.0)
        }
    }

    impl UseFixed for Fixed {
        #[inline]
        fn into(self) -> Target {
            Target::Fixed(self.0)
        }
    }
}

use crate::sys;
use crate::util::{cast_ptr, unwrap_nonzero, unwrap_u32};
use bitflags::bitflags;
use std::convert::TryFrom;
use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::os::unix::io::RawFd;

pub use sys::__kernel_rwf_t as RwFlags;
pub use sys::{
    io_uring_region_desc, io_uring_zcrx_area_reg, io_uring_zcrx_cqe, io_uring_zcrx_ifq_reg,
    io_uring_zcrx_rqe, IORING_MEM_REGION_TYPE_USER, IORING_ZCRX_AREA_SHIFT, IOU_PBUF_RING_INC,
    IOU_PBUF_RING_MMAP,
};

// From linux/io_uring.h
//
// NOTE: bindgen skips this due to the expression so we define it manually.
pub const IORING_ZCRX_AREA_MASK: u64 = !((1u64 << IORING_ZCRX_AREA_SHIFT) - 1);

/// Opaque types, you should use [`statx`](struct@libc::statx) instead.
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct statx {
    _priv: (),
}

/// Opaque types, you should use [`epoll_event`](libc::epoll_event) instead.
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct epoll_event {
    _priv: (),
}

/// A file descriptor that has not been registered with io_uring.
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Fd(pub RawFd);

/// A file descriptor that has been registered with io_uring using
/// [`Submitter::register_files`](crate::Submitter::register_files) or [`Submitter::register_files_sparse`](crate::Submitter::register_files_sparse).
/// This can reduce overhead compared to using [`Fd`] in some cases.
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Fixed(pub u32);

bitflags! {
    /// Options for [`Timeout`](super::Timeout).
    ///
    /// The default behavior is to treat the timespec as a relative time interval. `flags` may
    /// contain [`TimeoutFlags::ABS`] to indicate the timespec represents an absolute
    /// time. When an absolute time is being specified, the kernel will use its monotonic clock
    /// unless one of the following flags is set (they may not both be set):
    /// [`TimeoutFlags::BOOTTIME`] or [`TimeoutFlags::REALTIME`].
    ///
    /// The default behavior when the timeout expires is to sever dependent links, as a failed
    /// request normally would. To keep the links untouched include [`TimeoutFlags::ETIME_SUCCESS`].
    /// CQE will still contain -libc::ETIME in the res field
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub struct TimeoutFlags: u32 {
        const ABS = sys::IORING_TIMEOUT_ABS;

        const BOOTTIME = sys::IORING_TIMEOUT_BOOTTIME;

        const REALTIME = sys::IORING_TIMEOUT_REALTIME;

        const LINK_TIMEOUT_UPDATE = sys::IORING_LINK_TIMEOUT_UPDATE;

        const ETIME_SUCCESS = sys::IORING_TIMEOUT_ETIME_SUCCESS;

        const MULTISHOT = sys::IORING_TIMEOUT_MULTISHOT;
    }
}

bitflags! {
    /// Options for [`Fsync`](super::Fsync).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub struct FsyncFlags: u32 {
        const DATASYNC = sys::IORING_FSYNC_DATASYNC;
    }
}

bitflags! {
    /// Options for [`AsyncCancel`](super::AsyncCancel) and
    /// [`Submitter::register_sync_cancel`](super::Submitter::register_sync_cancel).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub(crate) struct AsyncCancelFlags: u32 {
        /// Cancel all requests that match the given criteria, rather
        /// than just canceling the first one found.
        ///
        /// Available since 5.19.
        const ALL = sys::IORING_ASYNC_CANCEL_ALL;

        /// Match based on the file descriptor used in the original
        /// request rather than the user_data.
        ///
        /// Available since 5.19.
        const FD = sys::IORING_ASYNC_CANCEL_FD;

        /// Match any request in the ring, regardless of user_data or
        /// file descriptor.  Can be used to cancel any pending
        /// request in the ring.
        ///
        /// Available since 5.19.
        const ANY = sys::IORING_ASYNC_CANCEL_ANY;

        /// Match based on the fixed file descriptor used in the original
        /// request rather than the user_data.
        ///
        /// Available since 6.0
        const FD_FIXED = sys::IORING_ASYNC_CANCEL_FD_FIXED;
    }
}

/// Wrapper around `open_how` as used in [the `openat2(2)` system
/// call](https://man7.org/linux/man-pages/man2/openat2.2.html).
#[derive(Default, Debug, Clone, Copy)]
#[repr(transparent)]
pub struct OpenHow(sys::open_how);

impl OpenHow {
    pub const fn new() -> Self {
        OpenHow(sys::open_how {
            flags: 0,
            mode: 0,
            resolve: 0,
        })
    }

    pub const fn flags(mut self, flags: u64) -> Self {
        self.0.flags = flags;
        self
    }

    pub const fn mode(mut self, mode: u64) -> Self {
        self.0.mode = mode;
        self
    }

    pub const fn resolve(mut self, resolve: u64) -> Self {
        self.0.resolve = resolve;
        self
    }
}

#[derive(Default, Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Timespec(pub(crate) sys::__kernel_timespec);

impl Timespec {
    #[inline]
    pub const fn new() -> Self {
        Timespec(sys::__kernel_timespec {
            tv_sec: 0,
            tv_nsec: 0,
        })
    }

    #[inline]
    pub const fn sec(mut self, sec: u64) -> Self {
        self.0.tv_sec = sec as _;
        self
    }

    #[inline]
    pub const fn nsec(mut self, nsec: u32) -> Self {
        self.0.tv_nsec = nsec as _;
        self
    }
}

impl From<std::time::Duration> for Timespec {
    fn from(value: std::time::Duration) -> Self {
        Timespec::new()
            .sec(value.as_secs())
            .nsec(value.subsec_nanos())
    }
}

/// Submit arguments
///
/// Note that arguments that exceed their lifetime will fail to compile.
///
/// ```compile_fail
/// use io_uring::types::{ SubmitArgs, Timespec };
///
/// let sigmask: libc::sigset_t = unsafe { std::mem::zeroed() };
///
/// let mut args = SubmitArgs::new();
///
/// {
///     let ts = Timespec::new();
///     args = args.timespec(&ts);
///     args = args.sigmask(&sigmask);
/// }
///
/// drop(args);
/// ```
#[derive(Default, Debug, Clone, Copy)]
pub struct SubmitArgs<'prev: 'now, 'now> {
    pub(crate) args: sys::io_uring_getevents_arg,
    prev: PhantomData<&'prev ()>,
    now: PhantomData<&'now ()>,
}

impl<'prev, 'now> SubmitArgs<'prev, 'now> {
    #[inline]
    pub const fn new() -> SubmitArgs<'static, 'static> {
        let args = sys::io_uring_getevents_arg {
            sigmask: 0,
            sigmask_sz: 0,
            min_wait_usec: 0,
            ts: 0,
        };

        SubmitArgs {
            args,
            prev: PhantomData,
            now: PhantomData,
        }
    }

    #[inline]
    /// Signals to mask during waiting for the result
    ///
    /// Masked signals will be restored after submit operation returns
    pub fn sigmask<'new>(mut self, sigmask: &'new libc::sigset_t) -> SubmitArgs<'now, 'new> {
        self.args.sigmask = cast_ptr(sigmask) as _;
        self.args.sigmask_sz = std::mem::size_of::<libc::sigset_t>() as _;

        SubmitArgs {
            args: self.args,
            prev: self.now,
            now: PhantomData,
        }
    }

    #[inline]
    /// Timeout for submit operation
    pub fn timespec<'new>(mut self, timespec: &'new Timespec) -> SubmitArgs<'now, 'new> {
        self.args.ts = cast_ptr(timespec) as _;

        SubmitArgs {
            args: self.args,
            prev: self.now,
            now: PhantomData,
        }
    }
}

#[repr(transparent)]
pub struct BufRingEntry(sys::io_uring_buf);

/// An entry in a buf_ring that allows setting the address, length and buffer id.
#[allow(clippy::len_without_is_empty)]
impl BufRingEntry {
    /// Sets the entry addr.
    pub fn set_addr(&mut self, addr: u64) {
        self.0.addr = addr;
    }

    /// Returns the entry addr.
    pub fn addr(&self) -> u64 {
        self.0.addr
    }

    /// Sets the entry len.
    pub fn set_len(&mut self, len: u32) {
        self.0.len = len;
    }

    /// Returns the entry len.
    pub fn len(&self) -> u32 {
        self.0.len
    }

    /// Sets the entry bid.
    pub fn set_bid(&mut self, bid: u16) {
        self.0.bid = bid;
    }

    /// Returns the entry bid.
    pub fn bid(&self) -> u16 {
        self.0.bid
    }

    /// The offset to the ring's tail field given the ring's base address.
    ///
    /// The caller should ensure the ring's base address is aligned with the system's page size,
    /// per the uring interface requirements.
    ///
    /// # Safety
    ///
    /// The ptr will be dereferenced in order to determine the address of the resv field,
    /// so the caller is responsible for passing in a valid pointer. And not just
    /// a valid pointer type, but also the argument must be the address to the first entry
    /// of the buf_ring for the resv field to even be considered the tail field of the ring.
    /// The entry must also be properly initialized.
    pub unsafe fn tail(ring_base: *const BufRingEntry) -> *const u16 {
        std::ptr::addr_of!((*ring_base).0.resv)
    }
}

/// A destination slot for sending fixed resources
/// (e.g. [`opcode::MsgRingSendFd`](crate::opcode::MsgRingSendFd)).
#[derive(Debug, Clone, Copy)]
pub struct DestinationSlot {
    /// Fixed slot as indexed by the kernel (target+1).
    dest: NonZeroU32,
}

impl DestinationSlot {
    // SAFETY: kernel constant, `IORING_FILE_INDEX_ALLOC` is always > 0.
    const AUTO_ALLOC: NonZeroU32 =
        unwrap_nonzero(NonZeroU32::new(sys::IORING_FILE_INDEX_ALLOC as u32));

    /// Use an automatically allocated target slot.
    pub const fn auto_target() -> Self {
        Self {
            dest: DestinationSlot::AUTO_ALLOC,
        }
    }

    /// Try to use a given target slot.
    ///
    /// Valid slots are in the range from `0` to `u32::MAX - 2` inclusive.
    pub fn try_from_slot_target(target: u32) -> Result<Self, u32> {
        // SAFETY: kernel constant, `IORING_FILE_INDEX_ALLOC` is always >= 2.
        const MAX_INDEX: u32 = unwrap_u32(DestinationSlot::AUTO_ALLOC.get().checked_sub(2));

        if target > MAX_INDEX {
            return Err(target);
        }

        let kernel_index = target.saturating_add(1);
        // SAFETY: by construction, always clamped between 1 and IORING_FILE_INDEX_ALLOC-1.
        debug_assert!(0 < kernel_index && kernel_index < DestinationSlot::AUTO_ALLOC.get());
        let dest = NonZeroU32::new(kernel_index).unwrap();

        Ok(Self { dest })
    }

    pub(crate) fn kernel_index_arg(&self) -> u32 {
        self.dest.get()
    }
}

/// Helper structure for parsing the result of a multishot [`opcode::RecvMsg`](crate::opcode::RecvMsg).
#[derive(Debug)]
pub struct RecvMsgOut<'buf> {
    header: sys::io_uring_recvmsg_out,
    /// The fixed length of the name field, in bytes.
    ///
    /// If the incoming name data is larger than this, it gets truncated to this.
    /// If it is smaller, it gets 0-padded to fill the whole field. In either case,
    /// this fixed amount of space is reserved in the result buffer.
    msghdr_name_len: usize,

    name_data: &'buf [u8],
    control_data: &'buf [u8],
    payload_data: &'buf [u8],
}

impl<'buf> RecvMsgOut<'buf> {
    const DATA_START: usize = std::mem::size_of::<sys::io_uring_recvmsg_out>();

    /// Parse the data buffered upon completion of a `RecvMsg` multishot operation.
    ///
    /// `buffer` is the whole buffer previously provided to the ring, while `msghdr`
    /// is the same content provided as input to the corresponding SQE
    /// (only `msg_namelen` and `msg_controllen` fields are relevant).
    #[allow(clippy::result_unit_err)]
    #[allow(clippy::useless_conversion)]
    pub fn parse(buffer: &'buf [u8], msghdr: &libc::msghdr) -> Result<Self, ()> {
        let msghdr_name_len = usize::try_from(msghdr.msg_namelen).unwrap();
        let msghdr_control_len = usize::try_from(msghdr.msg_controllen).unwrap();

        if Self::DATA_START
            .checked_add(msghdr_name_len)
            .and_then(|acc| acc.checked_add(msghdr_control_len))
            .map(|header_len| buffer.len() < header_len)
            .unwrap_or(true)
        {
            return Err(());
        }
        // SAFETY: buffer (minimum) length is checked here above.
        let header = unsafe {
            buffer
                .as_ptr()
                .cast::<sys::io_uring_recvmsg_out>()
                .read_unaligned()
        };

        // min is used because the header may indicate the true size of the data
        // while what we received was truncated.
        let (name_data, control_start) = {
            let name_start = Self::DATA_START;
            let name_data_end =
                name_start + usize::min(usize::try_from(header.namelen).unwrap(), msghdr_name_len);
            let name_field_end = name_start + msghdr_name_len;
            (&buffer[name_start..name_data_end], name_field_end)
        };
        let (control_data, payload_start) = {
            let control_data_end = control_start
                + usize::min(
                    usize::try_from(header.controllen).unwrap(),
                    msghdr_control_len,
                );
            let control_field_end = control_start + msghdr_control_len;
            (&buffer[control_start..control_data_end], control_field_end)
        };
        let payload_data = {
            let payload_data_end = payload_start
                + usize::min(
                    usize::try_from(header.payloadlen).unwrap(),
                    buffer.len() - payload_start,
                );
            &buffer[payload_start..payload_data_end]
        };

        Ok(Self {
            header,
            msghdr_name_len,
            name_data,
            control_data,
            payload_data,
        })
    }

    /// Return the length of the incoming `name` data.
    ///
    /// This may be larger than the size of the content returned by
    /// `name_data()`, if the kernel could not fit all the incoming
    /// data in the provided buffer size. In that case, name data in
    /// the result buffer gets truncated.
    pub fn incoming_name_len(&self) -> u32 {
        self.header.namelen
    }

    /// Return whether the incoming name data was larger than the provided limit/buffer.
    ///
    /// When `true`, data returned by `name_data()` is truncated and
    /// incomplete.
    pub fn is_name_data_truncated(&self) -> bool {
        self.header.namelen as usize > self.msghdr_name_len
    }

    /// Message control data, with the same semantics as `msghdr.msg_control`.
    pub fn name_data(&self) -> &[u8] {
        self.name_data
    }

    /// Return the length of the incoming `control` data.
    ///
    /// This may be larger than the size of the content returned by
    /// `control_data()`, if the kernel could not fit all the incoming
    /// data in the provided buffer size. In that case, control data in
    /// the result buffer gets truncated.
    pub fn incoming_control_len(&self) -> u32 {
        self.header.controllen
    }

    /// Return whether the incoming control data was larger than the provided limit/buffer.
    ///
    /// When `true`, data returned by `control_data()` is truncated and
    /// incomplete.
    pub fn is_control_data_truncated(&self) -> bool {
        (self.header.flags & u32::try_from(libc::MSG_CTRUNC).unwrap()) != 0
    }

    /// Message control data, with the same semantics as `msghdr.msg_control`.
    pub fn control_data(&self) -> &[u8] {
        self.control_data
    }

    /// Return whether the incoming payload was larger than the provided limit/buffer.
    ///
    /// When `true`, data returned by `payload_data()` is truncated and
    /// incomplete.
    pub fn is_payload_truncated(&self) -> bool {
        (self.header.flags & u32::try_from(libc::MSG_TRUNC).unwrap()) != 0
    }

    /// Message payload, as buffered by the kernel.
    pub fn payload_data(&self) -> &[u8] {
        self.payload_data
    }

    /// Return the length of the incoming `payload` data.
    ///
    /// This may be larger than the size of the content returned by
    /// `payload_data()`, if the kernel could not fit all the incoming
    /// data in the provided buffer size. In that case, payload data in
    /// the result buffer gets truncated.
    pub fn incoming_payload_len(&self) -> u32 {
        self.header.payloadlen
    }

    /// Message flags, with the same semantics as `msghdr.msg_flags`.
    pub fn flags(&self) -> u32 {
        self.header.flags
    }
}

/// [CancelBuilder] constructs match criteria for request cancellation.
///
/// The [CancelBuilder] can be used to selectively cancel one or more requests
/// by user_data, fd, fixed fd, or unconditionally.
///
/// ### Examples
///
/// ```
/// use io_uring::types::{CancelBuilder, Fd, Fixed};
///
/// // Match all in-flight requests.
/// CancelBuilder::any();
///
/// // Match a single request with user_data = 42.
/// CancelBuilder::user_data(42);
///
/// // Match a single request with fd = 42.
/// CancelBuilder::fd(Fd(42));
///
/// // Match a single request with fixed fd = 42.
/// CancelBuilder::fd(Fixed(42));
///
/// // Match all in-flight requests with user_data = 42.
/// CancelBuilder::user_data(42).all();
/// ```
#[derive(Debug)]
pub struct CancelBuilder {
    pub(crate) flags: AsyncCancelFlags,
    pub(crate) user_data: Option<u64>,
    pub(crate) fd: Option<sealed::Target>,
}

impl CancelBuilder {
    /// Create a new [CancelBuilder] which will match any in-flight request.
    ///
    /// This will cancel every in-flight request in the ring.
    ///
    /// Async cancellation matching any requests is only available since 5.19.
    pub const fn any() -> Self {
        Self {
            flags: AsyncCancelFlags::ANY,
            user_data: None,
            fd: None,
        }
    }

    /// Create a new [CancelBuilder] which will match in-flight requests
    /// with the given `user_data` value.
    ///
    /// The first request with the given `user_data` value will be canceled.
    /// [CancelBuilder::all](#method.all) can be called to instead match every
    /// request with the provided `user_data` value.
    pub const fn user_data(user_data: u64) -> Self {
        Self {
            flags: AsyncCancelFlags::empty(),
            user_data: Some(user_data),
            fd: None,
        }
    }

    /// Create a new [CancelBuilder] which will match in-flight requests with
    /// the given `fd` value.
    ///
    /// The first request with the given `fd` value will be canceled. [CancelBuilder::all](#method.all)
    /// can be called to instead match every request with the provided `fd` value.
    ///
    /// FD async cancellation is only available since 5.19.
    pub fn fd(fd: impl sealed::UseFixed) -> Self {
        let mut flags = AsyncCancelFlags::FD;
        let target = fd.into();
        if matches!(target, sealed::Target::Fixed(_)) {
            flags.insert(AsyncCancelFlags::FD_FIXED);
        }
        Self {
            flags,
            user_data: None,
            fd: Some(target),
        }
    }

    /// Modify the [CancelBuilder] match criteria to match all in-flight requests
    /// rather than just the first one.
    ///
    /// This has no effect when combined with [CancelBuilder::any](#method.any).
    ///
    /// Async cancellation matching all requests is only available since 5.19.
    pub fn all(mut self) -> Self {
        self.flags.insert(AsyncCancelFlags::ALL);
        self
    }

    pub(crate) fn to_fd(&self) -> i32 {
        self.fd
            .as_ref()
            .map(|target| match *target {
                sealed::Target::Fd(fd) => fd,
                sealed::Target::Fixed(idx) => idx as i32,
            })
            .unwrap_or(-1)
    }
}

/// Wrapper around `futex_waitv` as used in [`futex_waitv` system
/// call](https://www.kernel.org/doc/html/latest/userspace-api/futex2.html).
#[derive(Default, Debug, Clone, Copy)]
#[repr(transparent)]
pub struct FutexWaitV(sys::futex_waitv);

impl FutexWaitV {
    pub const fn new() -> Self {
        Self(sys::futex_waitv {
            val: 0,
            uaddr: 0,
            flags: 0,
            __reserved: 0,
        })
    }

    pub const fn val(mut self, val: u64) -> Self {
        self.0.val = val;
        self
    }

    pub const fn uaddr(mut self, uaddr: u64) -> Self {
        self.0.uaddr = uaddr;
        self
    }

    pub const fn flags(mut self, flags: u32) -> Self {
        self.0.flags = flags;
        self
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::types::sealed::Target;

    use super::*;

    #[test]
    fn timespec_from_duration_converts_correctly() {
        let duration = Duration::new(2, 500);
        let timespec = Timespec::from(duration);

        assert_eq!(timespec.0.tv_sec as u64, duration.as_secs());
        assert_eq!(timespec.0.tv_nsec as u32, duration.subsec_nanos());
    }

    #[test]
    fn test_cancel_builder_flags() {
        let cb = CancelBuilder::any();
        assert_eq!(cb.flags, AsyncCancelFlags::ANY);

        let mut cb = CancelBuilder::user_data(42);
        assert_eq!(cb.flags, AsyncCancelFlags::empty());
        assert_eq!(cb.user_data, Some(42));
        assert!(cb.fd.is_none());
        cb = cb.all();
        assert_eq!(cb.flags, AsyncCancelFlags::ALL);

        let mut cb = CancelBuilder::fd(Fd(42));
        assert_eq!(cb.flags, AsyncCancelFlags::FD);
        assert!(matches!(cb.fd, Some(Target::Fd(42))));
        assert!(cb.user_data.is_none());
        cb = cb.all();
        assert_eq!(cb.flags, AsyncCancelFlags::FD | AsyncCancelFlags::ALL);

        let mut cb = CancelBuilder::fd(Fixed(42));
        assert_eq!(cb.flags, AsyncCancelFlags::FD | AsyncCancelFlags::FD_FIXED);
        assert!(matches!(cb.fd, Some(Target::Fixed(42))));
        assert!(cb.user_data.is_none());
        cb = cb.all();
        assert_eq!(
            cb.flags,
            AsyncCancelFlags::FD | AsyncCancelFlags::FD_FIXED | AsyncCancelFlags::ALL
        );
    }
}
