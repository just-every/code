//! Operation codes that can be used to construct [`squeue::Entry`](crate::squeue::Entry)s.

#![allow(clippy::new_without_default)]

use std::convert::TryInto;
use std::mem;
use std::os::unix::io::RawFd;

use crate::squeue::Entry;
use crate::squeue::Entry128;
use crate::sys;
use crate::types::{self, sealed};

macro_rules! assign_fd {
    ( $sqe:ident . fd = $opfd:expr ) => {
        match $opfd {
            sealed::Target::Fd(fd) => $sqe.fd = fd,
            sealed::Target::Fixed(idx) => {
                $sqe.fd = idx as _;
                $sqe.flags |= crate::squeue::Flags::FIXED_FILE.bits();
            }
        }
    };
}

macro_rules! opcode {
    (@type impl sealed::UseFixed ) => {
        sealed::Target
    };
    (@type impl sealed::UseFd ) => {
        RawFd
    };
    (@type $name:ty ) => {
        $name
    };
    (
        $( #[$outer:meta] )*
        pub struct $name:ident {
            $( #[$new_meta:meta] )*

            $( $field:ident : { $( $tnt:tt )+ } ),*

            $(,)?

            ;;

            $(
                $( #[$opt_meta:meta] )*
                $opt_field:ident : $opt_tname:ty = $default:expr
            ),*

            $(,)?
        }

        pub const CODE = $opcode:expr;

        $( #[$build_meta:meta] )*
        pub fn build($self:ident) -> $entry:ty $build_block:block
    ) => {
        $( #[$outer] )*
        pub struct $name {
            $( $field : opcode!(@type $( $tnt )*), )*
            $( $opt_field : $opt_tname, )*
        }

        impl $name {
            $( #[$new_meta] )*
            #[inline]
            pub fn new($( $field : $( $tnt )* ),*) -> Self {
                $name {
                    $( $field: $field.into(), )*
                    $( $opt_field: $default, )*
                }
            }

            /// The opcode of the operation. This can be passed to
            /// [`Probe::is_supported`](crate::Probe::is_supported) to check if this operation is
            /// supported with the current kernel.
            pub const CODE: u8 = $opcode as _;

            $(
                $( #[$opt_meta] )*
                #[inline]
                pub const fn $opt_field(mut self, $opt_field: $opt_tname) -> Self {
                    self.$opt_field = $opt_field;
                    self
                }
            )*

            $( #[$build_meta] )*
            #[inline]
            pub fn build($self) -> $entry $build_block
        }
    }
}

/// inline zeroed to improve codegen
#[inline(always)]
fn sqe_zeroed() -> sys::io_uring_sqe {
    unsafe { mem::zeroed() }
}

opcode! {
    /// Do not perform any I/O.
    ///
    /// This is useful for testing the performance of the io_uring implementation itself.
    #[derive(Debug)]
    pub struct Nop { ;; }

    pub const CODE = sys::IORING_OP_NOP;

    pub fn build(self) -> Entry {
        let Nop {} = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = -1;
        Entry(sqe)
    }
}

opcode! {
    /// Vectored read, equivalent to `preadv2(2)`.
    #[derive(Debug)]
    pub struct Readv {
        fd: { impl sealed::UseFixed },
        iovec: { *const libc::iovec },
        len: { u32 },
        ;;
        ioprio: u16 = 0,
        offset: u64 = 0,
        /// specified for read operations, contains a bitwise OR of per-I/O flags,
        /// as described in the `preadv2(2)` man page.
        rw_flags: types::RwFlags = 0,
        buf_group: u16 = 0
    }

    pub const CODE = sys::IORING_OP_READV;

    pub fn build(self) -> Entry {
        let Readv {
            fd,
            iovec, len, offset,
            ioprio, rw_flags,
            buf_group
        } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.ioprio = ioprio;
        sqe.__bindgen_anon_2.addr = iovec as _;
        sqe.len = len;
        sqe.__bindgen_anon_1.off = offset;
        sqe.__bindgen_anon_3.rw_flags = rw_flags;
        sqe.__bindgen_anon_4.buf_group = buf_group;
        Entry(sqe)
    }
}

opcode! {
    /// Vectored write, equivalent to `pwritev2(2)`.
    #[derive(Debug)]
    pub struct Writev {
        fd: { impl sealed::UseFixed },
        iovec: { *const libc::iovec },
        len: { u32 },
        ;;
        ioprio: u16 = 0,
        offset: u64 = 0,
        /// specified for write operations, contains a bitwise OR of per-I/O flags,
        /// as described in the `preadv2(2)` man page.
        rw_flags: types::RwFlags = 0
    }

    pub const CODE = sys::IORING_OP_WRITEV;

    pub fn build(self) -> Entry {
        let Writev {
            fd,
            iovec, len, offset,
            ioprio, rw_flags
        } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.ioprio = ioprio;
        sqe.__bindgen_anon_2.addr = iovec as _;
        sqe.len = len;
        sqe.__bindgen_anon_1.off = offset;
        sqe.__bindgen_anon_3.rw_flags = rw_flags;
        Entry(sqe)
    }
}

opcode! {
    /// File sync, equivalent to `fsync(2)`.
    ///
    /// Note that, while I/O is initiated in the order in which it appears in the submission queue,
    /// completions are unordered. For example, an application which places a write I/O followed by
    /// an fsync in the submission queue cannot expect the fsync to apply to the write. The two
    /// operations execute in parallel, so the fsync may complete before the write is issued to the
    /// storage. The same is also true for previously issued writes that have not completed prior to
    /// the fsync.
    #[derive(Debug)]
    pub struct Fsync {
        fd: { impl sealed::UseFixed },
        ;;
        /// The `flags` bit mask may contain either 0, for a normal file integrity sync,
        /// or [types::FsyncFlags::DATASYNC] to provide data sync only semantics.
        /// See the descriptions of `O_SYNC` and `O_DSYNC` in the `open(2)` manual page for more information.
        flags: types::FsyncFlags = types::FsyncFlags::empty()
    }

    pub const CODE = sys::IORING_OP_FSYNC;

    pub fn build(self) -> Entry {
        let Fsync { fd, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_3.fsync_flags = flags.bits();
        Entry(sqe)
    }
}

opcode! {
    /// Read from a file into a fixed buffer that has been previously registered with
    /// [`Submitter::register_buffers`](crate::Submitter::register_buffers).
    ///
    /// The return values match those documented in the `preadv2(2)` man pages.
    #[derive(Debug)]
    pub struct ReadFixed {
        fd: { impl sealed::UseFixed },
        buf: { *mut u8 },
        len: { u32 },
        buf_index: { u16 },
        ;;
        ioprio: u16 = 0,
        /// The offset of the file to read from.
        offset: u64 = 0,
        /// Specified for read operations, contains a bitwise OR of per-I/O flags, as described in
        /// the `preadv2(2)` man page.
        rw_flags: types::RwFlags = 0
    }

    pub const CODE = sys::IORING_OP_READ_FIXED;

    pub fn build(self) -> Entry {
        let ReadFixed {
            fd,
            buf, len, offset,
            buf_index,
            ioprio, rw_flags
        } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.ioprio = ioprio;
        sqe.__bindgen_anon_2.addr = buf as _;
        sqe.len = len;
        sqe.__bindgen_anon_1.off = offset;
        sqe.__bindgen_anon_3.rw_flags = rw_flags;
        sqe.__bindgen_anon_4.buf_index = buf_index;
        Entry(sqe)
    }
}

opcode! {
    /// Write to a file from a fixed buffer that have been previously registered with
    /// [`Submitter::register_buffers`](crate::Submitter::register_buffers).
    ///
    /// The return values match those documented in the `pwritev2(2)` man pages.
    #[derive(Debug)]
    pub struct WriteFixed {
        fd: { impl sealed::UseFixed },
        buf: { *const u8 },
        len: { u32 },
        buf_index: { u16 },
        ;;
        ioprio: u16 = 0,
        /// The offset of the file to write to.
        offset: u64 = 0,
        /// Specified for write operations, contains a bitwise OR of per-I/O flags, as described in
        /// the `pwritev2(2)` man page.
        rw_flags: types::RwFlags = 0
    }

    pub const CODE = sys::IORING_OP_WRITE_FIXED;

    pub fn build(self) -> Entry {
        let WriteFixed {
            fd,
            buf, len, offset,
            buf_index,
            ioprio, rw_flags
        } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.ioprio = ioprio;
        sqe.__bindgen_anon_2.addr = buf as _;
        sqe.len = len;
        sqe.__bindgen_anon_1.off = offset;
        sqe.__bindgen_anon_3.rw_flags = rw_flags;
        sqe.__bindgen_anon_4.buf_index = buf_index;
        Entry(sqe)
    }
}

opcode! {
    /// Poll the specified fd.
    ///
    /// Unlike poll or epoll without `EPOLLONESHOT`, this interface defaults to work in one shot mode.
    /// That is, once the poll operation is completed, it will have to be resubmitted.
    ///
    /// If multi is set, the poll will work in multi shot mode instead. That means it will
    /// repeatedly trigger when the requested event becomes true, and hence multiple CQEs can be
    /// generated from this single submission. The CQE flags field will have IORING_CQE_F_MORE set
    /// on completion if the application should expect further CQE entries from the original
    /// request. If this flag isn't set on completion, then the poll request has been terminated
    /// and no further events will be generated. This mode is available since 5.13.
    #[derive(Debug)]
    pub struct PollAdd {
        /// The bits that may be set in `flags` are defined in `<poll.h>`,
        /// and documented in `poll(2)`.
        fd: { impl sealed::UseFixed },
        flags: { u32 },
        ;;
        multi: bool = false
    }

    pub const CODE = sys::IORING_OP_POLL_ADD;

    pub fn build(self) -> Entry {
        let PollAdd { fd, flags, multi } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        if multi {
            sqe.len = sys::IORING_POLL_ADD_MULTI;
        }

        #[cfg(target_endian = "little")] {
            sqe.__bindgen_anon_3.poll32_events = flags;
        }

        #[cfg(target_endian = "big")] {
            let x = flags << 16;
            let y = flags >> 16;
            let flags = x | y;
            sqe.__bindgen_anon_3.poll32_events = flags;
        }

        Entry(sqe)
    }
}

opcode! {
    /// Remove an existing [poll](PollAdd) request.
    ///
    /// If found, the `result` method of the `cqueue::Entry` will return 0.
    /// If not found, `result` will return `-libc::ENOENT`.
    #[derive(Debug)]
    pub struct PollRemove {
        user_data: { u64 }
        ;;
    }

    pub const CODE = sys::IORING_OP_POLL_REMOVE;

    pub fn build(self) -> Entry {
        let PollRemove { user_data } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = -1;
        sqe.__bindgen_anon_2.addr = user_data;
        Entry(sqe)
    }
}

opcode! {
    /// Sync a file segment with disk, equivalent to `sync_file_range(2)`.
    #[derive(Debug)]
    pub struct SyncFileRange {
        fd: { impl sealed::UseFixed },
        len: { u32 },
        ;;
        /// the offset method holds the offset in bytes
        offset: u64 = 0,
        /// the flags method holds the flags for the command
        flags: u32 = 0
    }

    pub const CODE = sys::IORING_OP_SYNC_FILE_RANGE;

    pub fn build(self) -> Entry {
        let SyncFileRange {
            fd,
            len, offset,
            flags
        } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.len = len;
        sqe.__bindgen_anon_1.off = offset;
        sqe.__bindgen_anon_3.sync_range_flags = flags;
        Entry(sqe)
    }
}

opcode! {
    /// Send a message on a socket, equivalent to `send(2)`.
    ///
    /// fd must be set to the socket file descriptor, addr must contains a pointer to the msghdr
    /// structure, and flags holds the flags associated with the system call.
    #[derive(Debug)]
    pub struct SendMsg {
        fd: { impl sealed::UseFixed },
        msg: { *const libc::msghdr },
        ;;
        ioprio: u16 = 0,
        flags: u32 = 0
    }

    pub const CODE = sys::IORING_OP_SENDMSG;

    pub fn build(self) -> Entry {
        let SendMsg { fd, msg, ioprio, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.ioprio = ioprio;
        sqe.__bindgen_anon_2.addr = msg as _;
        sqe.len = 1;
        sqe.__bindgen_anon_3.msg_flags = flags;
        Entry(sqe)
    }
}

opcode! {
    /// Receive a message on a socket, equivalent to `recvmsg(2)`.
    ///
    /// See also the description of [`SendMsg`].
    #[derive(Debug)]
    pub struct RecvMsg {
        fd: { impl sealed::UseFixed },
        msg: { *mut libc::msghdr },
        ;;
        ioprio: u16 = 0,
        flags: u32 = 0,
        buf_group: u16 = 0
    }

    pub const CODE = sys::IORING_OP_RECVMSG;

    pub fn build(self) -> Entry {
        let RecvMsg { fd, msg, ioprio, flags, buf_group } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.ioprio = ioprio;
        sqe.__bindgen_anon_2.addr = msg as _;
        sqe.len = 1;
        sqe.__bindgen_anon_3.msg_flags = flags;
        sqe.__bindgen_anon_4.buf_group = buf_group;
        Entry(sqe)
    }
}

opcode! {
    /// Receive multiple messages on a socket, equivalent to `recvmsg(2)`.
    ///
    /// Parameters:
    ///     msg:       For this multishot variant of ResvMsg, only the msg_namelen and msg_controllen
    ///                fields are relevant.
    ///     buf_group: The id of the provided buffer pool to use for each received message.
    ///
    /// See also the description of [`SendMsg`] and [`types::RecvMsgOut`].
    ///
    /// The multishot version allows the application to issue a single receive request, which
    /// repeatedly posts a CQE when data is available. It requires the MSG_WAITALL flag is not set.
    /// Each CQE will take a buffer out of a provided buffer pool for receiving. The application
    /// should check the flags of each CQE, regardless of its result. If a posted CQE does not have
    /// the IORING_CQE_F_MORE flag set then the multishot receive will be done and the application
    /// should issue a new request.
    ///
    /// Unlike [`RecvMsg`], this multishot recvmsg will prepend a struct which describes the layout
    /// of the rest of the buffer in combination with the initial msghdr structure submitted with
    /// the request. Use [`types::RecvMsgOut`] to parse the data received and access its
    /// components.
    ///
    /// The recvmsg multishot variant is available since kernel 6.0.
    #[derive(Debug)]
    pub struct RecvMsgMulti {
        fd: { impl sealed::UseFixed },
        msg: { *const libc::msghdr },
        buf_group: { u16 },
        ;;
        ioprio: u16 = 0,
        flags: u32 = 0
    }

    pub const CODE = sys::IORING_OP_RECVMSG;

    pub fn build(self) -> Entry {
        let RecvMsgMulti { fd, msg, buf_group, ioprio, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_2.addr = msg as _;
        sqe.len = 1;
        sqe.__bindgen_anon_3.msg_flags = flags;
        sqe.__bindgen_anon_4.buf_group = buf_group;
        sqe.flags |= crate::squeue::Flags::BUFFER_SELECT.bits();
        sqe.ioprio = ioprio | (sys::IORING_RECV_MULTISHOT as u16);
        Entry(sqe)
    }
}

opcode! {
    /// Register a timeout operation.
    ///
    /// A timeout will trigger a wakeup event on the completion ring for anyone waiting for events.
    /// A timeout condition is met when either the specified timeout expires, or the specified number of events have completed.
    /// Either condition will trigger the event.
    /// The request will complete with `-ETIME` if the timeout got completed through expiration of the timer,
    /// or 0 if the timeout got completed through requests completing on their own.
    /// If the timeout was cancelled before it expired, the request will complete with `-ECANCELED`.
    #[derive(Debug)]
    pub struct Timeout {
        timespec: { *const types::Timespec },
        ;;
        /// `count` may contain a completion event count.
        /// If [`TimeoutFlags::MULTISHOT`](types::TimeoutFlags::MULTISHOT) is set in `flags`, this is the number of repeats.
        /// A value of 0 means the timeout is indefinite and can only be stopped by a removal request.
        count: u32 = 0,

        flags: types::TimeoutFlags = types::TimeoutFlags::empty()
    }

    pub const CODE = sys::IORING_OP_TIMEOUT;

    pub fn build(self) -> Entry {
        let Timeout { timespec, count, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = -1;
        sqe.__bindgen_anon_2.addr = timespec as _;
        sqe.len = 1;
        sqe.__bindgen_anon_1.off = count as _;
        sqe.__bindgen_anon_3.timeout_flags = flags.bits();
        Entry(sqe)
    }
}

// === 5.5 ===

opcode! {
    /// Attempt to remove an existing [timeout operation](Timeout).
    pub struct TimeoutRemove {
        user_data: { u64 },
        ;;
    }

    pub const CODE = sys::IORING_OP_TIMEOUT_REMOVE;

    pub fn build(self) -> Entry {
        let TimeoutRemove { user_data } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = -1;
        sqe.__bindgen_anon_2.addr = user_data;
        Entry(sqe)
    }
}

opcode! {
    /// Attempt to update an existing [timeout operation](Timeout) with a new timespec.
    /// The optional `count` value of the original timeout value cannot be updated.
    pub struct TimeoutUpdate {
        user_data: { u64 },
        timespec: { *const types::Timespec },
        ;;
        flags: types::TimeoutFlags = types::TimeoutFlags::empty()
    }

    pub const CODE = sys::IORING_OP_TIMEOUT_REMOVE;

    pub fn build(self) -> Entry {
        let TimeoutUpdate { user_data, timespec, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = -1;
        sqe.__bindgen_anon_1.off = timespec as _;
        sqe.__bindgen_anon_2.addr = user_data;
        sqe.__bindgen_anon_3.timeout_flags = flags.bits() | sys::IORING_TIMEOUT_UPDATE;
        Entry(sqe)
    }
}

opcode! {
    /// Accept a new connection on a socket, equivalent to `accept4(2)`.
    pub struct Accept {
        fd: { impl sealed::UseFixed },
        addr: { *mut libc::sockaddr },
        addrlen: { *mut libc::socklen_t },
        ;;
        file_index: Option<types::DestinationSlot> = None,
        flags: i32 = 0
    }

    pub const CODE = sys::IORING_OP_ACCEPT;

    pub fn build(self) -> Entry {
        let Accept { fd, addr, addrlen, file_index, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_2.addr = addr as _;
        sqe.__bindgen_anon_1.addr2 = addrlen as _;
        sqe.__bindgen_anon_3.accept_flags = flags as _;
        if let Some(dest) = file_index {
            sqe.__bindgen_anon_5.file_index = dest.kernel_index_arg();
        }
        Entry(sqe)
    }
}

opcode! {
    /// Set a socket option.
    pub struct SetSockOpt {
        fd: { impl sealed::UseFixed },
        level: { u32 },
        optname: { u32 },
        optval: { *const libc::c_void },
        optlen: { u32 },
        ;;
        flags: u32 = 0
    }

    pub const CODE = sys::IORING_OP_URING_CMD;

    pub fn build(self) -> Entry {
        let SetSockOpt { fd, level, optname, optval, optlen, flags } = self;
        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_1.__bindgen_anon_1.cmd_op = sys::SOCKET_URING_OP_SETSOCKOPT;

        sqe.__bindgen_anon_2.__bindgen_anon_1.level = level;
        sqe.__bindgen_anon_2.__bindgen_anon_1.optname = optname;
        sqe.__bindgen_anon_3.uring_cmd_flags = flags;
        sqe.__bindgen_anon_5.optlen = optlen;
        unsafe { *sqe.__bindgen_anon_6.optval.as_mut() = optval as u64 };
        Entry(sqe)
    }
}

opcode! {
    /// Attempt to cancel an already issued request.
    pub struct AsyncCancel {
        user_data: { u64 }
        ;;

        // TODO flags
    }

    pub const CODE = sys::IORING_OP_ASYNC_CANCEL;

    pub fn build(self) -> Entry {
        let AsyncCancel { user_data } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = -1;
        sqe.__bindgen_anon_2.addr = user_data;
        Entry(sqe)
    }
}

opcode! {
    /// This request must be linked with another request through
    /// [`Flags::IO_LINK`](crate::squeue::Flags::IO_LINK) which is described below.
    /// Unlike [`Timeout`], [`LinkTimeout`] acts on the linked request, not the completion queue.
    pub struct LinkTimeout {
        timespec: { *const types::Timespec },
        ;;
        flags: types::TimeoutFlags = types::TimeoutFlags::empty()
    }

    pub const CODE = sys::IORING_OP_LINK_TIMEOUT;

    pub fn build(self) -> Entry {
        let LinkTimeout { timespec, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = -1;
        sqe.__bindgen_anon_2.addr = timespec as _;
        sqe.len = 1;
        sqe.__bindgen_anon_3.timeout_flags = flags.bits();
        Entry(sqe)
    }
}

opcode! {
    /// Connect a socket, equivalent to `connect(2)`.
    pub struct Connect {
        fd: { impl sealed::UseFixed },
        addr: { *const libc::sockaddr },
        addrlen: { libc::socklen_t }
        ;;
    }

    pub const CODE = sys::IORING_OP_CONNECT;

    pub fn build(self) -> Entry {
        let Connect { fd, addr, addrlen } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_2.addr = addr as _;
        sqe.__bindgen_anon_1.off = addrlen as _;
        Entry(sqe)
    }
}

// === 5.6 ===

opcode! {
    /// Preallocate or deallocate space to a file, equivalent to `fallocate(2)`.
    pub struct Fallocate {
        fd: { impl sealed::UseFixed },
        len: { u64 },
        ;;
        offset: u64 = 0,
        mode: i32 = 0
    }

    pub const CODE = sys::IORING_OP_FALLOCATE;

    pub fn build(self) -> Entry {
        let Fallocate { fd, len, offset, mode } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_2.addr = len;
        sqe.len = mode as _;
        sqe.__bindgen_anon_1.off = offset;
        Entry(sqe)
    }
}

opcode! {
    /// Open a file, equivalent to `openat(2)`.
    pub struct OpenAt {
        dirfd: { impl sealed::UseFd },
        pathname: { *const libc::c_char },
        ;;
        file_index: Option<types::DestinationSlot> = None,
        flags: i32 = 0,
        mode: libc::mode_t = 0
    }

    pub const CODE = sys::IORING_OP_OPENAT;

    pub fn build(self) -> Entry {
        let OpenAt { dirfd, pathname, file_index, flags, mode } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = dirfd;
        sqe.__bindgen_anon_2.addr = pathname as _;
        sqe.len = mode;
        sqe.__bindgen_anon_3.open_flags = flags as _;
        if let Some(dest) = file_index {
            sqe.__bindgen_anon_5.file_index = dest.kernel_index_arg();
        }
        Entry(sqe)
    }
}

opcode! {
    /// Close a file descriptor, equivalent to `close(2)`.
    ///
    /// Use a types::Fixed(fd) argument to close an io_uring direct descriptor.
    pub struct Close {
        fd: { impl sealed::UseFixed },
        ;;
    }

    pub const CODE = sys::IORING_OP_CLOSE;

    pub fn build(self) -> Entry {
        let Close { fd } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        match fd {
            sealed::Target::Fd(fd) => sqe.fd = fd,
            sealed::Target::Fixed(idx) => {
                sqe.fd = 0;
                sqe.__bindgen_anon_5.file_index = idx + 1;
            }
        }
        Entry(sqe)
    }
}

opcode! {
    /// This command is an alternative to using
    /// [`Submitter::register_files_update`](crate::Submitter::register_files_update) which then
    /// works in an async fashion, like the rest of the io_uring commands.
    pub struct FilesUpdate {
        fds: { *const RawFd },
        len: { u32 },
        ;;
        offset: i32 = 0
    }

    pub const CODE = sys::IORING_OP_FILES_UPDATE;

    pub fn build(self) -> Entry {
        let FilesUpdate { fds, len, offset } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = -1;
        sqe.__bindgen_anon_2.addr = fds as _;
        sqe.len = len;
        sqe.__bindgen_anon_1.off = offset as _;
        Entry(sqe)
    }
}

opcode! {
    /// Get file status, equivalent to `statx(2)`.
    pub struct Statx {
        dirfd: { impl sealed::UseFd },
        pathname: { *const libc::c_char },
        statxbuf: { *mut types::statx },
        ;;
        flags: i32 = 0,
        mask: u32 = 0
    }

    pub const CODE = sys::IORING_OP_STATX;

    pub fn build(self) -> Entry {
        let Statx {
            dirfd, pathname, statxbuf,
            flags, mask
        } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = dirfd;
        sqe.__bindgen_anon_2.addr = pathname as _;
        sqe.len = mask;
        sqe.__bindgen_anon_1.off = statxbuf as _;
        sqe.__bindgen_anon_3.statx_flags = flags as _;
        Entry(sqe)
    }
}

opcode! {
    /// Issue the equivalent of a `pread(2)` or `pwrite(2)` system call
    ///
    /// * `fd` is the file descriptor to be operated on,
    /// * `addr` contains the buffer in question,
    /// * `len` contains the length of the IO operation,
    ///
    /// These are non-vectored versions of the `IORING_OP_READV` and `IORING_OP_WRITEV` opcodes.
    /// See also `read(2)` and `write(2)` for the general description of the related system call.
    ///
    /// Available since 5.6.
    pub struct Read {
        fd: { impl sealed::UseFixed },
        buf: { *mut u8 },
        len: { u32 },
        ;;
        /// `offset` contains the read or write offset.
        ///
        /// If `fd` does not refer to a seekable file, `offset` must be set to zero.
        /// If `offset` is set to `-1`, the offset will use (and advance) the file position,
        /// like the `read(2)` and `write(2)` system calls.
        offset: u64 = 0,
        ioprio: u16 = 0,
        rw_flags: types::RwFlags = 0,
        buf_group: u16 = 0
    }

    pub const CODE = sys::IORING_OP_READ;

    pub fn build(self) -> Entry {
        let Read {
            fd,
            buf, len, offset,
            ioprio, rw_flags,
            buf_group
        } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.ioprio = ioprio;
        sqe.__bindgen_anon_2.addr = buf as _;
        sqe.len = len;
        sqe.__bindgen_anon_1.off = offset;
        sqe.__bindgen_anon_3.rw_flags = rw_flags;
        sqe.__bindgen_anon_4.buf_group = buf_group;
        Entry(sqe)
    }
}

opcode! {
    /// Issue the equivalent of a `pread(2)` or `pwrite(2)` system call
    ///
    /// * `fd` is the file descriptor to be operated on,
    /// * `addr` contains the buffer in question,
    /// * `len` contains the length of the IO operation,
    ///
    /// These are non-vectored versions of the `IORING_OP_READV` and `IORING_OP_WRITEV` opcodes.
    /// See also `read(2)` and `write(2)` for the general description of the related system call.
    ///
    /// Available since 5.6.
    pub struct Write {
        fd: { impl sealed::UseFixed },
        buf: { *const u8 },
        len: { u32 },
        ;;
        /// `offset` contains the read or write offset.
        ///
        /// If `fd` does not refer to a seekable file, `offset` must be set to zero.
        /// If `offsett` is set to `-1`, the offset will use (and advance) the file position,
        /// like the `read(2)` and `write(2)` system calls.
        offset: u64 = 0,
        ioprio: u16 = 0,
        rw_flags: types::RwFlags = 0
    }

    pub const CODE = sys::IORING_OP_WRITE;

    pub fn build(self) -> Entry {
        let Write {
            fd,
            buf, len, offset,
            ioprio, rw_flags
        } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.ioprio = ioprio;
        sqe.__bindgen_anon_2.addr = buf as _;
        sqe.len = len;
        sqe.__bindgen_anon_1.off = offset;
        sqe.__bindgen_anon_3.rw_flags = rw_flags;
        Entry(sqe)
    }
}

opcode! {
    /// Predeclare an access pattern for file data, equivalent to `posix_fadvise(2)`.
    pub struct Fadvise {
        fd: { impl sealed::UseFixed },
        len: { libc::off_t },
        advice: { i32 },
        ;;
        offset: u64 = 0,
    }

    pub const CODE = sys::IORING_OP_FADVISE;

    pub fn build(self) -> Entry {
        let Fadvise { fd, len, advice, offset } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.len = len as _;
        sqe.__bindgen_anon_1.off = offset;
        sqe.__bindgen_anon_3.fadvise_advice = advice as _;
        Entry(sqe)
    }
}

opcode! {
    /// Give advice about use of memory, equivalent to `madvise(2)`.
    pub struct Madvise {
        addr: { *const libc::c_void },
        len: { libc::off_t },
        advice: { i32 },
        ;;
    }

    pub const CODE = sys::IORING_OP_MADVISE;

    pub fn build(self) -> Entry {
        let Madvise { addr, len, advice } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = -1;
        sqe.__bindgen_anon_2.addr = addr as _;
        sqe.len = len as _;
        sqe.__bindgen_anon_3.fadvise_advice = advice as _;
        Entry(sqe)
    }
}

opcode! {
    /// Send a message on a socket, equivalent to `send(2)`.
    pub struct Send {
        fd: { impl sealed::UseFixed },
        buf: { *const u8 },
        len: { u32 },
        ;;
        flags: i32 = 0,

        /// Set the destination address, for sending from an unconnected socket.
        ///
        /// When set, `dest_addr_len` must be set as well.
        /// See also `man 3 io_uring_prep_send_set_addr`.
        dest_addr: *const libc::sockaddr = core::ptr::null(),
        dest_addr_len: libc::socklen_t = 0,
    }

    pub const CODE = sys::IORING_OP_SEND;

    pub fn build(self) -> Entry {
        let Send { fd, buf, len, flags, dest_addr, dest_addr_len } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_2.addr = buf as _;
        sqe.__bindgen_anon_1.addr2 = dest_addr as _;
        sqe.__bindgen_anon_5.__bindgen_anon_1.addr_len = dest_addr_len as _;
        sqe.len = len;
        sqe.__bindgen_anon_3.msg_flags = flags as _;
        Entry(sqe)
    }
}

opcode! {
    /// Receive a message from a socket, equivalent to `recv(2)`.
    pub struct Recv {
        fd: { impl sealed::UseFixed },
        buf: { *mut u8 },
        len: { u32 },
        ;;
        flags: i32 = 0,
        buf_group: u16 = 0
    }

    pub const CODE = sys::IORING_OP_RECV;

    pub fn build(self) -> Entry {
        let Recv { fd, buf, len, flags, buf_group } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_2.addr = buf as _;
        sqe.len = len;
        sqe.__bindgen_anon_3.msg_flags = flags as _;
        sqe.__bindgen_anon_4.buf_group = buf_group;
        Entry(sqe)
    }
}

opcode! {
    /// Receive multiple messages from a socket, equivalent to `recv(2)`.
    ///
    /// Parameter:
    ///     buf_group: The id of the provided buffer pool to use for each received message.
    ///
    /// MSG_WAITALL should not be set in flags.
    ///
    /// The multishot version allows the application to issue a single receive request, which
    /// repeatedly posts a CQE when data is available. Each CQE will take a buffer out of a
    /// provided buffer pool for receiving. The application should check the flags of each CQE,
    /// regardless of its result. If a posted CQE does not have the IORING_CQE_F_MORE flag set then
    /// the multishot receive will be done and the application should issue a new request.
    ///
    /// Multishot variants are available since kernel 6.0.

    pub struct RecvMulti {
        fd: { impl sealed::UseFixed },
        buf_group: { u16 },
        ;;
        flags: i32 = 0,
    }

    pub const CODE = sys::IORING_OP_RECV;

    pub fn build(self) -> Entry {
        let RecvMulti { fd, buf_group, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_3.msg_flags = flags as _;
        sqe.__bindgen_anon_4.buf_group = buf_group;
        sqe.flags |= crate::squeue::Flags::BUFFER_SELECT.bits();
        sqe.ioprio = sys::IORING_RECV_MULTISHOT as _;
        Entry(sqe)
    }
}

opcode! {
    /// Open a file, equivalent to `openat2(2)`.
    pub struct OpenAt2 {
        dirfd: { impl sealed::UseFd },
        pathname: { *const libc::c_char },
        how: { *const types::OpenHow }
        ;;
        file_index: Option<types::DestinationSlot> = None,
    }

    pub const CODE = sys::IORING_OP_OPENAT2;

    pub fn build(self) -> Entry {
        let OpenAt2 { dirfd, pathname, how, file_index } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = dirfd;
        sqe.__bindgen_anon_2.addr = pathname as _;
        sqe.len = mem::size_of::<sys::open_how>() as _;
        sqe.__bindgen_anon_1.off = how as _;
        if let Some(dest) = file_index {
            sqe.__bindgen_anon_5.file_index = dest.kernel_index_arg();
        }
        Entry(sqe)
    }
}

opcode! {
    /// Modify an epoll file descriptor, equivalent to `epoll_ctl(2)`.
    pub struct EpollCtl {
        epfd: { impl sealed::UseFixed },
        fd: { impl sealed::UseFd },
        op: { i32 },
        ev: { *const types::epoll_event },
        ;;
    }

    pub const CODE = sys::IORING_OP_EPOLL_CTL;

    pub fn build(self) -> Entry {
        let EpollCtl { epfd, fd, op, ev } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = epfd);
        sqe.__bindgen_anon_2.addr = ev as _;
        sqe.len = op as _;
        sqe.__bindgen_anon_1.off = fd as _;
        Entry(sqe)
    }
}

// === 5.7 ===

opcode! {
    /// Splice data to/from a pipe, equivalent to `splice(2)`.
    ///
    /// if `fd_in` refers to a pipe, `off_in` must be `-1`;
    /// The description of `off_in` also applied to `off_out`.
    pub struct Splice {
        fd_in: { impl sealed::UseFixed },
        off_in: { i64 },
        fd_out: { impl sealed::UseFixed },
        off_out: { i64 },
        len: { u32 },
        ;;
        /// see man `splice(2)` for description of flags.
        flags: u32 = 0
    }

    pub const CODE = sys::IORING_OP_SPLICE;

    pub fn build(self) -> Entry {
        let Splice { fd_in, off_in, fd_out, off_out, len, mut flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd_out);
        sqe.len = len;
        sqe.__bindgen_anon_1.off = off_out as _;

        sqe.__bindgen_anon_5.splice_fd_in = match fd_in {
            sealed::Target::Fd(fd) => fd,
            sealed::Target::Fixed(idx) => {
                flags |= sys::SPLICE_F_FD_IN_FIXED;
                idx as _
            }
        };

        sqe.__bindgen_anon_2.splice_off_in = off_in as _;
        sqe.__bindgen_anon_3.splice_flags = flags;
        Entry(sqe)
    }
}

opcode! {
    /// Register `nbufs` buffers that each have the length `len` with ids starting from `bid` in the
    /// group `bgid` that can be used for any request. See
    /// [`BUFFER_SELECT`](crate::squeue::Flags::BUFFER_SELECT) for more info.
    pub struct ProvideBuffers {
        addr: { *mut u8 },
        len: { i32 },
        nbufs: { u16 },
        bgid: { u16 },
        bid: { u16 }
        ;;
    }

    pub const CODE = sys::IORING_OP_PROVIDE_BUFFERS;

    pub fn build(self) -> Entry {
        let ProvideBuffers { addr, len, nbufs, bgid, bid } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = nbufs as _;
        sqe.__bindgen_anon_2.addr = addr as _;
        sqe.len = len as _;
        sqe.__bindgen_anon_1.off = bid as _;
        sqe.__bindgen_anon_4.buf_group = bgid;
        Entry(sqe)
    }
}

opcode! {
    /// Remove some number of buffers from a buffer group. See
    /// [`BUFFER_SELECT`](crate::squeue::Flags::BUFFER_SELECT) for more info.
    pub struct RemoveBuffers {
        nbufs: { u16 },
        bgid: { u16 }
        ;;
    }

    pub const CODE = sys::IORING_OP_REMOVE_BUFFERS;

    pub fn build(self) -> Entry {
        let RemoveBuffers { nbufs, bgid } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = nbufs as _;
        sqe.__bindgen_anon_4.buf_group = bgid;
        Entry(sqe)
    }
}

// === 5.8 ===

opcode! {
    /// Duplicate pipe content, equivalent to `tee(2)`.
    pub struct Tee {
        fd_in: { impl sealed::UseFixed },
        fd_out: { impl sealed::UseFixed },
        len: { u32 }
        ;;
        flags: u32 = 0
    }

    pub const CODE = sys::IORING_OP_TEE;

    pub fn build(self) -> Entry {
        let Tee { fd_in, fd_out, len, mut flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;

        assign_fd!(sqe.fd = fd_out);
        sqe.len = len;

        sqe.__bindgen_anon_5.splice_fd_in = match fd_in {
            sealed::Target::Fd(fd) => fd,
            sealed::Target::Fixed(idx) => {
                flags |= sys::SPLICE_F_FD_IN_FIXED;
                idx as _
            }
        };

        sqe.__bindgen_anon_3.splice_flags = flags;

        Entry(sqe)
    }
}

// === 5.11 ===

opcode! {
    /// Shut down all or part of a full duplex connection on a socket, equivalent to `shutdown(2)`.
    /// Available since kernel 5.11.
    pub struct Shutdown {
        fd: { impl sealed::UseFixed },
        how: { i32 },
        ;;
    }

    pub const CODE = sys::IORING_OP_SHUTDOWN;

    pub fn build(self) -> Entry {
        let Shutdown { fd, how } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.len = how as _;
        Entry(sqe)
    }
}

opcode! {
    // Change the name or location of a file, equivalent to `renameat2(2)`.
    // Available since kernel 5.11.
    pub struct RenameAt {
        olddirfd: { impl sealed::UseFd },
        oldpath: { *const libc::c_char },
        newdirfd: { impl sealed::UseFd },
        newpath: { *const libc::c_char },
        ;;
        flags: u32 = 0
    }

    pub const CODE = sys::IORING_OP_RENAMEAT;

    pub fn build(self) -> Entry {
        let RenameAt {
            olddirfd, oldpath,
            newdirfd, newpath,
            flags
        } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = olddirfd;
        sqe.__bindgen_anon_2.addr = oldpath as _;
        sqe.len = newdirfd as _;
        sqe.__bindgen_anon_1.off = newpath as _;
        sqe.__bindgen_anon_3.rename_flags = flags;
        Entry(sqe)
    }
}

opcode! {
    // Delete a name and possible the file it refers to, equivalent to `unlinkat(2)`.
    // Available since kernel 5.11.
    pub struct UnlinkAt {
        dirfd: { impl sealed::UseFd },
        pathname: { *const libc::c_char },
        ;;
        flags: i32 = 0
    }

    pub const CODE = sys::IORING_OP_UNLINKAT;

    pub fn build(self) -> Entry {
        let UnlinkAt { dirfd, pathname, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = dirfd;
        sqe.__bindgen_anon_2.addr = pathname as _;
        sqe.__bindgen_anon_3.unlink_flags = flags as _;
        Entry(sqe)
    }
}

// === 5.15 ===

opcode! {
    /// Make a directory, equivalent to `mkdirat(2)`.
    pub struct MkDirAt {
        dirfd: { impl sealed::UseFd },
        pathname: { *const libc::c_char },
        ;;
        mode: libc::mode_t = 0
    }

    pub const CODE = sys::IORING_OP_MKDIRAT;

    pub fn build(self) -> Entry {
        let MkDirAt { dirfd, pathname, mode } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = dirfd;
        sqe.__bindgen_anon_2.addr = pathname as _;
        sqe.len = mode;
        Entry(sqe)
    }
}

opcode! {
    /// Create a symlink, equivalent to `symlinkat(2)`.
    pub struct SymlinkAt {
        newdirfd: { impl sealed::UseFd },
        target: { *const libc::c_char },
        linkpath: { *const libc::c_char },
        ;;
    }

    pub const CODE = sys::IORING_OP_SYMLINKAT;

    pub fn build(self) -> Entry {
        let SymlinkAt { newdirfd, target, linkpath } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = newdirfd;
        sqe.__bindgen_anon_2.addr = target as _;
        sqe.__bindgen_anon_1.addr2 = linkpath as _;
        Entry(sqe)
    }
}

opcode! {
    /// Create a hard link, equivalent to `linkat(2)`.
    pub struct LinkAt {
        olddirfd: { impl sealed::UseFd },
        oldpath: { *const libc::c_char },
        newdirfd: { impl sealed::UseFd },
        newpath: { *const libc::c_char },
        ;;
        flags: i32 = 0
    }

    pub const CODE = sys::IORING_OP_LINKAT;

    pub fn build(self) -> Entry {
        let LinkAt { olddirfd, oldpath, newdirfd, newpath, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = olddirfd as _;
        sqe.__bindgen_anon_2.addr = oldpath as _;
        sqe.len = newdirfd as _;
        sqe.__bindgen_anon_1.addr2 = newpath as _;
        sqe.__bindgen_anon_3.hardlink_flags = flags as _;
        Entry(sqe)
    }
}

// === 5.17 ===

opcode! {
    /// Get extended attribute, equivalent to `getxattr(2)`.
    pub struct GetXattr {
        name: { *const libc::c_char },
        value: { *mut libc::c_void },
        path: { *const libc::c_char },
        len: { u32 },
        ;;
    }

    pub const CODE = sys::IORING_OP_GETXATTR;

    pub fn build(self) -> Entry {
        let GetXattr { name, value, path, len } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.__bindgen_anon_2.addr = name as _;
        sqe.len = len;
        sqe.__bindgen_anon_1.off = value as _;
        unsafe { sqe.__bindgen_anon_6.__bindgen_anon_1.as_mut().addr3 = path as _ };
        sqe.__bindgen_anon_3.xattr_flags = 0;
        Entry(sqe)
    }
}

opcode! {
    /// Set extended attribute, equivalent to `setxattr(2)`.
    pub struct SetXattr {
        name: { *const libc::c_char },
        value: { *const libc::c_void },
        path: { *const libc::c_char },
        len: { u32 },
        ;;
        flags: i32 = 0
    }

    pub const CODE = sys::IORING_OP_SETXATTR;

    pub fn build(self) -> Entry {
        let SetXattr { name, value, path, flags, len } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.__bindgen_anon_2.addr = name as _;
        sqe.len = len;
        sqe.__bindgen_anon_1.off = value as _;
        unsafe { sqe.__bindgen_anon_6.__bindgen_anon_1.as_mut().addr3 = path as _ };
        sqe.__bindgen_anon_3.xattr_flags = flags as _;
        Entry(sqe)
    }
}

opcode! {
    /// Get extended attribute from a file descriptor, equivalent to `fgetxattr(2)`.
    pub struct FGetXattr {
        fd: { impl sealed::UseFixed },
        name: { *const libc::c_char },
        value: { *mut libc::c_void },
        len: { u32 },
        ;;
    }

    pub const CODE = sys::IORING_OP_FGETXATTR;

    pub fn build(self) -> Entry {
        let FGetXattr { fd, name, value, len } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_2.addr = name as _;
        sqe.len = len;
        sqe.__bindgen_anon_1.off = value as _;
        sqe.__bindgen_anon_3.xattr_flags = 0;
        Entry(sqe)
    }
}

opcode! {
    /// Set extended attribute on a file descriptor, equivalent to `fsetxattr(2)`.
    pub struct FSetXattr {
        fd: { impl sealed::UseFixed },
        name: { *const libc::c_char },
        value: { *const libc::c_void },
        len: { u32 },
        ;;
        flags: i32 = 0
    }

    pub const CODE = sys::IORING_OP_FSETXATTR;

    pub fn build(self) -> Entry {
        let FSetXattr { fd, name, value, flags, len } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_2.addr = name as _;
        sqe.len = len;
        sqe.__bindgen_anon_1.off = value as _;
        sqe.__bindgen_anon_3.xattr_flags = flags as _;
        Entry(sqe)
    }
}

// === 5.18 ===

opcode! {
    /// Send a message (with data) to a target ring.
    pub struct MsgRingData {
        ring_fd: { impl sealed::UseFd },
        result: { i32 },
        user_data: { u64 },
        user_flags: { Option<u32> },
        ;;
        opcode_flags: u32 = 0
    }

    pub const CODE = sys::IORING_OP_MSG_RING;

    pub fn build(self) -> Entry {
        let MsgRingData { ring_fd, result, user_data, user_flags, opcode_flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.__bindgen_anon_2.addr = sys::IORING_MSG_DATA.into();
        sqe.fd = ring_fd;
        sqe.len = result as u32;
        sqe.__bindgen_anon_1.off = user_data;
        sqe.__bindgen_anon_3.msg_ring_flags = opcode_flags;
        if let Some(flags) = user_flags {
            sqe.__bindgen_anon_5.file_index = flags;
            unsafe {sqe.__bindgen_anon_3.msg_ring_flags |= sys::IORING_MSG_RING_FLAGS_PASS};
        }
        Entry(sqe)
    }
}

// === 5.19 ===

opcode! {
    /// Attempt to cancel an already issued request, receiving a cancellation
    /// builder, which allows for the new cancel criterias introduced since
    /// 5.19.
    pub struct AsyncCancel2 {
        builder: { types::CancelBuilder }
        ;;
    }

    pub const CODE = sys::IORING_OP_ASYNC_CANCEL;

    pub fn build(self) -> Entry {
        let AsyncCancel2 { builder } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = builder.to_fd();
        sqe.__bindgen_anon_2.addr = builder.user_data.unwrap_or(0);
        sqe.__bindgen_anon_3.cancel_flags = builder.flags.bits();
        Entry(sqe)
    }
}

opcode! {
    /// A file/device-specific 16-byte command, akin (but not equivalent) to `ioctl(2)`.
    pub struct UringCmd16 {
        fd: { impl sealed::UseFixed },
        cmd_op: { u32 },
        ;;
        /// The `buf_index` is an index into an array of fixed buffers,
        /// and is only valid if fixed buffers were registered.
        buf_index: Option<u16> = None,
        /// Arbitrary command data.
        cmd: [u8; 16] = [0u8; 16]
    }

    pub const CODE = sys::IORING_OP_URING_CMD;

    pub fn build(self) -> Entry {
        let UringCmd16 { fd, cmd_op, cmd, buf_index } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_1.__bindgen_anon_1.cmd_op = cmd_op;
        unsafe { *sqe.__bindgen_anon_6.cmd.as_mut().as_mut_ptr().cast::<[u8; 16]>() = cmd };
        if let Some(buf_index) = buf_index {
            sqe.__bindgen_anon_4.buf_index = buf_index;
            unsafe {
                sqe.__bindgen_anon_3.uring_cmd_flags |= sys::IORING_URING_CMD_FIXED;
            }
        }
        Entry(sqe)
    }
}

opcode! {
    /// A file/device-specific 80-byte command, akin (but not equivalent) to `ioctl(2)`.
    pub struct UringCmd80 {
        fd: { impl sealed::UseFixed },
        cmd_op: { u32 },
        ;;
        /// The `buf_index` is an index into an array of fixed buffers,
        /// and is only valid if fixed buffers were registered.
        buf_index: Option<u16> = None,
        /// Arbitrary command data.
        cmd: [u8; 80] = [0u8; 80]
    }

    pub const CODE = sys::IORING_OP_URING_CMD;

    pub fn build(self) -> Entry128 {
        let UringCmd80 { fd, cmd_op, cmd, buf_index } = self;

        let cmd1 = cmd[..16].try_into().unwrap();
        let cmd2 = cmd[16..].try_into().unwrap();

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_1.__bindgen_anon_1.cmd_op = cmd_op;
        unsafe { *sqe.__bindgen_anon_6.cmd.as_mut().as_mut_ptr().cast::<[u8; 16]>() = cmd1 };
        if let Some(buf_index) = buf_index {
            sqe.__bindgen_anon_4.buf_index = buf_index;
            unsafe {
                sqe.__bindgen_anon_3.uring_cmd_flags |= sys::IORING_URING_CMD_FIXED;
            }
        }
        Entry128(Entry(sqe), cmd2)
    }
}

opcode! {
    /// Create an endpoint for communication, equivalent to `socket(2)`.
    ///
    /// If the `file_index` argument is set, the resulting socket is
    /// directly mapped to the given fixed-file slot instead of being
    /// returned as a normal file descriptor. The application must first
    /// have registered a file table, and the target slot should fit into
    /// it.
    ///
    /// Available since 5.19.
    pub struct Socket {
        domain: { i32 },
        socket_type: { i32 },
        protocol: { i32 },
        ;;
        file_index: Option<types::DestinationSlot> = None,
        flags: types::RwFlags = 0,
    }

    pub const CODE = sys::IORING_OP_SOCKET;

    pub fn build(self) -> Entry {
        let Socket { domain, socket_type, protocol, file_index, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = domain as _;
        sqe.__bindgen_anon_1.off = socket_type as _;
        sqe.len = protocol as _;
        sqe.__bindgen_anon_3.rw_flags = flags;
        if let Some(dest) = file_index {
            sqe.__bindgen_anon_5.file_index = dest.kernel_index_arg();
        }
        Entry(sqe)
    }
}

opcode! {
    /// Accept multiple new connections on a socket.
    ///
    /// Set the `allocate_file_index` property if fixed file table entries should be used.
    ///
    /// Available since 5.19.
    pub struct AcceptMulti {
        fd: { impl sealed::UseFixed },
        ;;
        allocate_file_index: bool = false,
        flags: i32 = 0
    }

    pub const CODE = sys::IORING_OP_ACCEPT;

    pub fn build(self) -> Entry {
        let AcceptMulti { fd, allocate_file_index, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.ioprio = sys::IORING_ACCEPT_MULTISHOT as u16;
        // No out SockAddr is passed for the multishot accept case.
        // The user should perform a syscall to get any resulting connection's remote address.
        sqe.__bindgen_anon_3.accept_flags = flags as _;
        if allocate_file_index {
            sqe.__bindgen_anon_5.file_index = sys::IORING_FILE_INDEX_ALLOC as u32;
        }
        Entry(sqe)
    }
}

// === 6.0 ===

opcode! {
    /// Send a message (with fixed FD) to a target ring.
    pub struct MsgRingSendFd {
        ring_fd: { impl sealed::UseFd },
        fixed_slot_src: { types::Fixed },
        dest_slot_index: { types::DestinationSlot },
        user_data: { u64 },
        ;;
        opcode_flags: u32 = 0
    }

    pub const CODE = sys::IORING_OP_MSG_RING;

    pub fn build(self) -> Entry {
        let MsgRingSendFd { ring_fd, fixed_slot_src, dest_slot_index, user_data, opcode_flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.__bindgen_anon_2.addr = sys::IORING_MSG_SEND_FD.into();
        sqe.fd = ring_fd;
        sqe.__bindgen_anon_1.off = user_data;
        unsafe { sqe.__bindgen_anon_6.__bindgen_anon_1.as_mut().addr3 = fixed_slot_src.0 as u64 };
        sqe.__bindgen_anon_5.file_index = dest_slot_index.kernel_index_arg();
        sqe.__bindgen_anon_3.msg_ring_flags = opcode_flags;
        Entry(sqe)
    }
}

// === 6.0 ===

opcode! {
    /// Send a zerocopy message on a socket, equivalent to `send(2)`.
    ///
    /// When `dest_addr` is non-zero it points to the address of the target with `dest_addr_len`
    /// specifying its size, turning the request into a `sendto(2)`
    ///
    /// A fixed (pre-mapped) buffer can optionally be used from pre-mapped buffers that have been
    /// previously registered with [`Submitter::register_buffers`](crate::Submitter::register_buffers).
    ///
    /// This operation might result in two completion queue entries.
    /// See the `IORING_OP_SEND_ZC` section at [io_uring_enter][] for the exact semantics.
    /// Notifications posted by this operation can be checked with [notif](crate::cqueue::notif).
    ///
    /// [io_uring_enter]: https://man7.org/linux/man-pages/man2/io_uring_enter.2.html
    pub struct SendZc {
        fd: { impl sealed::UseFixed },
        buf: { *const u8 },
        len: { u32 },
        ;;
        /// The `buf_index` is an index into an array of fixed buffers, and is only valid if fixed
        /// buffers were registered.
        ///
        /// The buf and len arguments must fall within a region specified by buf_index in the
        /// previously registered buffer. The buffer need not be aligned with the start of the
        /// registered buffer.
        buf_index: Option<u16> = None,
        dest_addr: *const libc::sockaddr = core::ptr::null(),
        dest_addr_len: libc::socklen_t = 0,
        flags: i32 = 0,
        zc_flags: u16 = 0,
    }

    pub const CODE = sys::IORING_OP_SEND_ZC;

    pub fn build(self) -> Entry {
        let SendZc { fd, buf, len, buf_index, dest_addr, dest_addr_len, flags, zc_flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_2.addr = buf as _;
        sqe.len = len;
        sqe.__bindgen_anon_3.msg_flags = flags as _;
        sqe.ioprio = zc_flags;
        if let Some(buf_index) = buf_index {
            sqe.__bindgen_anon_4.buf_index = buf_index;
            sqe.ioprio |= sys::IORING_RECVSEND_FIXED_BUF as u16;
        }
        sqe.__bindgen_anon_1.addr2 = dest_addr as _;
        sqe.__bindgen_anon_5.__bindgen_anon_1.addr_len = dest_addr_len as _;
        Entry(sqe)
    }
}

// === 6.1 ===

opcode! {
    /// Send a zerocopy message on a socket, equivalent to `send(2)`.
    ///
    /// fd must be set to the socket file descriptor, addr must contains a pointer to the msghdr
    /// structure, and flags holds the flags associated with the system call.
    #[derive(Debug)]
    pub struct SendMsgZc {
        fd: { impl sealed::UseFixed },
        msg: { *const libc::msghdr },
        ;;
        ioprio: u16 = 0,
        flags: u32 = 0
    }

    pub const CODE = sys::IORING_OP_SENDMSG_ZC;

    pub fn build(self) -> Entry {
        let SendMsgZc { fd, msg, ioprio, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.ioprio = ioprio;
        sqe.__bindgen_anon_2.addr = msg as _;
        sqe.len = 1;
        sqe.__bindgen_anon_3.msg_flags = flags;
        Entry(sqe)
    }
}

// === 6.7 ===

opcode! {
    /// Issue the equivalent of `pread(2)` with multi-shot semantics.
    pub struct ReadMulti {
        fd: { impl sealed::UseFixed },
        len: { u32 },
        buf_group: { u16 },
        ;;
        offset: u64 = 0,
    }

    pub const CODE = sys::IORING_OP_READ_MULTISHOT;

    pub fn build(self) -> Entry {
        let Self { fd, len, buf_group, offset } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_1.off = offset;
        sqe.len = len;
        sqe.__bindgen_anon_4.buf_group = buf_group;
        sqe.flags = crate::squeue::Flags::BUFFER_SELECT.bits();
        Entry(sqe)
    }
}

opcode! {
    /// Wait on a futex, like but not equivalant to `futex(2)`'s `FUTEX_WAIT_BITSET`.
    ///
    /// Wait on a futex at address `futex` and which still has the value `val` and with `futex2(2)`
    /// flags of `futex_flags`. `musk` can be set to a specific bitset mask, which will be matched
    /// by the waking side to decide who to wake up. To always get woken, an application may use
    /// `FUTEX_BITSET_MATCH_ANY` (truncated to futex bits). `futex_flags` follows the `futex2(2)`
    /// flags, not the `futex(2)` v1 interface flags. `flags` are currently unused and hence `0`
    /// must be passed.
    #[derive(Debug)]
    pub struct FutexWait {
        futex: { *const u32 },
        val: { u64 },
        mask: { u64 },
        futex_flags: { u32 },
        ;;
        flags: u32 = 0
    }

    pub const CODE = sys::IORING_OP_FUTEX_WAIT;

    pub fn build(self) -> Entry {
        let FutexWait { futex, val, mask, futex_flags, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = futex_flags as _;
        sqe.__bindgen_anon_2.addr = futex as usize as _;
        sqe.__bindgen_anon_1.off = val;
        unsafe { sqe.__bindgen_anon_6.__bindgen_anon_1.as_mut().addr3 = mask };
        sqe.__bindgen_anon_3.futex_flags = flags;
        Entry(sqe)
    }
}

opcode! {
    /// Wake up waiters on a futex, like but not equivalant to `futex(2)`'s `FUTEX_WAKE_BITSET`.
    ///
    /// Wake any waiters on the futex indicated by `futex` and at most `val` futexes. `futex_flags`
    /// indicates the `futex2(2)` modifier flags. If a given bitset for who to wake is desired,
    /// then that must be set in `mask`. Use `FUTEX_BITSET_MATCH_ANY` (truncated to futex bits) to
    /// match any waiter on the given futex. `flags` are currently unused and hence `0` must be
    /// passed.
    #[derive(Debug)]
    pub struct FutexWake {
        futex: { *const u32 },
        val: { u64 },
        mask: { u64 },
        futex_flags: { u32 },
        ;;
        flags: u32 = 0
    }

    pub const CODE = sys::IORING_OP_FUTEX_WAKE;

    pub fn build(self) -> Entry {
        let FutexWake { futex, val, mask, futex_flags, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = futex_flags as _;
        sqe.__bindgen_anon_2.addr = futex as usize as _;
        sqe.__bindgen_anon_1.off = val;
        unsafe { sqe.__bindgen_anon_6.__bindgen_anon_1.as_mut().addr3 = mask };
        sqe.__bindgen_anon_3.futex_flags = flags;
        Entry(sqe)
    }
}

opcode! {
    /// Wait on multiple futexes.
    ///
    /// Wait on multiple futexes at the same time. Futexes are given by `futexv` and `nr_futex` is
    /// the number of futexes in that array. Unlike `FutexWait`, the desired bitset mask and values
    /// are passed in `futexv`. `flags` are currently unused and hence `0` must be passed.
    #[derive(Debug)]
    pub struct FutexWaitV {
        futexv: { *const types::FutexWaitV },
        nr_futex: { u32 },
        ;;
        flags: u32 = 0
    }

    pub const CODE = sys::IORING_OP_FUTEX_WAITV;

    pub fn build(self) -> Entry {
        let FutexWaitV { futexv, nr_futex, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.__bindgen_anon_2.addr = futexv as usize as _;
        sqe.len = nr_futex;
        sqe.__bindgen_anon_3.futex_flags = flags;
        Entry(sqe)
    }
}

opcode! {
    /// Issue the equivalent of a `waitid(2)` system call.
    ///
    /// Available since kernel 6.7.
    #[derive(Debug)]
    pub struct WaitId {
        idtype: { libc::idtype_t },
        id: { libc::id_t },
        options: { libc::c_int },
        ;;
        infop: *const libc::siginfo_t = std::ptr::null(),
        flags: libc::c_uint = 0,
    }

    pub const CODE = sys::IORING_OP_WAITID;

    pub fn build(self) -> Entry {
        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = self.id as _;
        sqe.len = self.idtype as _;
        sqe.__bindgen_anon_3.waitid_flags = self.flags;
        sqe.__bindgen_anon_5.file_index = self.options as _;
        sqe.__bindgen_anon_1.addr2 = self.infop as _;
        Entry(sqe)
    }
}

// === 6.8 ===

opcode! {
    /// Install a fixed file descriptor
    ///
    /// Turns a direct descriptor into a regular file descriptor that can be later used by regular
    /// system calls that take a normal raw file descriptor
    #[derive(Debug)]
    pub struct FixedFdInstall {
        fd: { types::Fixed },
        file_flags: { u32 },
        ;;
    }

    pub const CODE = sys::IORING_OP_FIXED_FD_INSTALL;

    pub fn build(self) -> Entry {
        let FixedFdInstall { fd, file_flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = fd.0 as _;
        sqe.flags = crate::squeue::Flags::FIXED_FILE.bits();
        sqe.__bindgen_anon_3.install_fd_flags = file_flags;
        Entry(sqe)
    }
}

// === 6.9 ===

opcode! {
    /// Perform file truncation, equivalent to `ftruncate(2)`.
    #[derive(Debug)]
    pub struct Ftruncate {
        fd: { impl sealed::UseFixed },
        len: { u64 },
        ;;
    }

    pub const CODE = sys::IORING_OP_FTRUNCATE;

    pub fn build(self) -> Entry {
        let Ftruncate { fd, len } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_1.off = len;
        Entry(sqe)
    }
}

// === 6.10 ===

opcode! {
    /// Send a bundle of messages on a socket in a single request.
    pub struct SendBundle {
        fd: { impl sealed::UseFixed },
        buf_group: { u16 },
        ;;
        flags: i32 = 0,
        len: u32 = 0
    }

    pub const CODE = sys::IORING_OP_SEND;

    pub fn build(self) -> Entry {
        let SendBundle { fd, len, flags, buf_group } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.len = len;
        sqe.__bindgen_anon_3.msg_flags = flags as _;
        sqe.ioprio |= sys::IORING_RECVSEND_BUNDLE as u16;
        sqe.flags |= crate::squeue::Flags::BUFFER_SELECT.bits();
        sqe.__bindgen_anon_4.buf_group = buf_group;
        Entry(sqe)
    }
}

opcode! {
    /// Receive a bundle of buffers from a socket.
    ///
    /// Parameter
    ///     buf_group: The id of the provided buffer pool to use for the bundle.
    ///
    /// Note that as of kernel 6.10 first recv always gets a single buffer, while second
    /// obtains the bundle of remaining buffers. This behavior may change in the future.
    ///
    /// Bundle variant is available since kernel 6.10
    pub struct RecvBundle {
        fd: { impl sealed::UseFixed },
        buf_group: { u16 },
        ;;
        flags: i32 = 0
    }

    pub const CODE = sys::IORING_OP_RECV;

    pub fn build(self) -> Entry {
        let RecvBundle { fd, buf_group, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_3.msg_flags = flags as _;
        sqe.__bindgen_anon_4.buf_group = buf_group;
        sqe.flags |= crate::squeue::Flags::BUFFER_SELECT.bits();
        sqe.ioprio |= sys::IORING_RECVSEND_BUNDLE as u16;
        Entry(sqe)
    }
}

opcode! {
    /// Receive multiple messages from a socket as a bundle.
    ///
    /// Parameter:
    ///     buf_group: The id of the provided buffer pool to use for each received message.
    ///
    /// MSG_WAITALL should not be set in flags.
    ///
    /// The multishot version allows the application to issue a single receive request, which
    /// repeatedly posts a CQE when data is available. Each CQE will take a bundle of buffers
    /// out of a provided buffer pool for receiving. The application should check the flags of each CQE,
    /// regardless of its result. If a posted CQE does not have the IORING_CQE_F_MORE flag set then
    /// the multishot receive will be done and the application should issue a new request.
    ///
    /// Note that as of kernel 6.10 first CQE always gets a single buffer, while second
    /// obtains the bundle of remaining buffers. This behavior may change in the future.
    ///
    /// Multishot bundle variant is available since kernel 6.10.
    pub struct RecvMultiBundle {
        fd: { impl sealed::UseFixed },
        buf_group: { u16 },
        ;;
        flags: i32 = 0
    }

    pub const CODE = sys::IORING_OP_RECV;

    pub fn build(self) -> Entry {
        let RecvMultiBundle { fd, buf_group, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_3.msg_flags = flags as _;
        sqe.__bindgen_anon_4.buf_group = buf_group;
        sqe.flags |= crate::squeue::Flags::BUFFER_SELECT.bits();
        sqe.ioprio = sys::IORING_RECV_MULTISHOT as _;
        sqe.ioprio |= sys::IORING_RECVSEND_BUNDLE as u16;
        Entry(sqe)
    }
}

// === 6.11 ===

opcode! {
    /// Bind a socket, equivalent to `bind(2)`.
    pub struct Bind {
        fd: { impl sealed::UseFixed },
        addr: { *const libc::sockaddr },
        addrlen: { libc::socklen_t }
        ;;
    }

    pub const CODE = sys::IORING_OP_BIND;

    pub fn build(self) -> Entry {
        let Bind { fd, addr, addrlen } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_2.addr = addr as _;
        sqe.__bindgen_anon_1.off = addrlen as _;
        Entry(sqe)
    }
}

opcode! {
    /// Listen on a socket, equivalent to `listen(2)`.
    pub struct Listen {
        fd: { impl sealed::UseFixed },
        backlog: { i32 },
        ;;
    }

    pub const CODE = sys::IORING_OP_LISTEN;

    pub fn build(self) -> Entry {
        let Listen { fd, backlog } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.len = backlog as _;
        Entry(sqe)
    }
}

// === 6.15 ===

opcode! {
    /// Issue the zerocopy equivalent of a `recv(2)` system call.
    pub struct RecvZc {
        fd: { impl sealed::UseFixed },
        len: { u32 },
        ;;
        ifq: u32 = 0,
        ioprio: u16 = 0,
    }

    pub const CODE = sys::IORING_OP_RECV_ZC;

    pub fn build(self) -> Entry {
        let Self { fd, len, ifq, ioprio } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.len = len;
        sqe.ioprio = ioprio | sys::IORING_RECV_MULTISHOT as u16;
        sqe.__bindgen_anon_5.zcrx_ifq_idx = ifq;
        Entry(sqe)
    }
}

opcode! {
    /// Issue the equivalent of a `epoll_wait(2)` system call.
    pub struct EpollWait {
        fd: { impl sealed::UseFixed },
        events: { *mut types::epoll_event },
        max_events: { u32 },
        ;;
        flags: u32 = 0,
    }

    pub const CODE = sys::IORING_OP_EPOLL_WAIT;

    pub fn build(self) -> Entry {
        let Self { fd, events, max_events, flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_2.addr = events as u64;
        sqe.len = max_events;
        sqe.__bindgen_anon_3.poll32_events = flags;
        Entry(sqe)
    }
}

opcode! {
    /// Vectored read into a fixed buffer, equivalent to `preadv2(2)`.
    pub struct ReadvFixed {
        fd: { impl sealed::UseFixed },
        iovec: { *const ::libc::iovec },
        len: { u32 },
        buf_index: { u16 },
        ;;
        ioprio: u16 = 0,
        offset: u64 = 0,
        rw_flags: i32 = 0,
    }

    pub const CODE = sys::IORING_OP_READV_FIXED;

    pub fn build(self) -> Entry {
        let Self { fd, iovec, len, buf_index, offset, ioprio, rw_flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_1.off = offset as _;
        sqe.__bindgen_anon_2.addr = iovec as _;
        sqe.len = len;
        sqe.__bindgen_anon_4.buf_index = buf_index;
        sqe.ioprio = ioprio;
        sqe.__bindgen_anon_3.rw_flags = rw_flags;
        Entry(sqe)
    }
}

opcode! {
    /// Vectored write from a fixed buffer, equivalent to `pwritev2(2)`.
    pub struct WritevFixed {
        fd: { impl sealed::UseFixed },
        iovec: { *const ::libc::iovec },
        len: { u32 },
        buf_index: { u16 },
        ;;
        ioprio: u16 = 0,
        offset: u64 = 0,
        rw_flags: i32 = 0,
    }

    pub const CODE = sys::IORING_OP_WRITEV_FIXED;

    pub fn build(self) -> Entry {
        let Self { fd, iovec, len, buf_index, offset, ioprio, rw_flags } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        assign_fd!(sqe.fd = fd);
        sqe.__bindgen_anon_1.off = offset as _;
        sqe.__bindgen_anon_2.addr = iovec as _;
        sqe.len = len;
        sqe.__bindgen_anon_4.buf_index = buf_index;
        sqe.ioprio = ioprio;
        sqe.__bindgen_anon_3.rw_flags = rw_flags;
        Entry(sqe)
    }
}

// === 6.16 ===

opcode! {
    // Create a pipe, equivalent to `pipe(2)`.
    pub struct Pipe {
        fds: { *mut RawFd },
        ;;
        flags: u32 = 0,
        file_index: Option<types::DestinationSlot> = None,
    }

    pub const CODE = sys::IORING_OP_PIPE;

    pub fn build(self) -> Entry {
        let Self { fds, flags, file_index } = self;

        let mut sqe = sqe_zeroed();
        sqe.opcode = Self::CODE;
        sqe.fd = 0;
        sqe.__bindgen_anon_2.addr = fds as _;
        sqe.__bindgen_anon_3.pipe_flags = flags;
        if let Some(dest) = file_index {
            sqe.__bindgen_anon_5.file_index = dest.kernel_index_arg();
        }
        Entry(sqe)
    }
}
