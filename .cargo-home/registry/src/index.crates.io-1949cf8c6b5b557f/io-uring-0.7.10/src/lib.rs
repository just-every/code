//! The `io_uring` library for Rust.
//!
//! The crate only provides a summary of the parameters.
//! For more detailed documentation, see manpage.

#[macro_use]
mod util;
pub mod cqueue;
pub mod opcode;
pub mod register;
pub mod squeue;
mod submit;
mod sys;
pub mod types;

use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::{cmp, io, mem};

#[cfg(feature = "io_safety")]
use std::os::unix::io::{AsFd, BorrowedFd};

pub use cqueue::CompletionQueue;
pub use register::Probe;
pub use squeue::SubmissionQueue;
pub use submit::Submitter;
use util::{Mmap, OwnedFd};

/// IoUring instance
///
/// - `S`: The ring's submission queue entry (SQE) type, either [`squeue::Entry`] or
///   [`squeue::Entry128`];
/// - `C`: The ring's completion queue entry (CQE) type, either [`cqueue::Entry`] or
///   [`cqueue::Entry32`].
pub struct IoUring<S = squeue::Entry, C = cqueue::Entry>
where
    S: squeue::EntryMarker,
    C: cqueue::EntryMarker,
{
    sq: squeue::Inner<S>,
    cq: cqueue::Inner<C>,
    fd: OwnedFd,
    params: Parameters,
    memory: ManuallyDrop<MemoryMap>,
}

#[allow(dead_code)]
struct MemoryMap {
    sq_mmap: Mmap,
    sqe_mmap: Mmap,
    cq_mmap: Option<Mmap>,
}

/// IoUring build params
#[derive(Clone, Default)]
pub struct Builder<S = squeue::Entry, C = cqueue::Entry>
where
    S: squeue::EntryMarker,
    C: cqueue::EntryMarker,
{
    dontfork: bool,
    params: sys::io_uring_params,
    phantom: PhantomData<(S, C)>,
}

/// The parameters that were used to construct an [`IoUring`].
///
/// This type is a transparent wrapper over the system structure `io_uring_params`. A value can be
/// (unsafely) created from any properly laid-out and initialized memory representation.
#[derive(Clone)]
#[repr(transparent)]
pub struct Parameters(sys::io_uring_params);

unsafe impl<S: squeue::EntryMarker, C: cqueue::EntryMarker> Send for IoUring<S, C> {}
unsafe impl<S: squeue::EntryMarker, C: cqueue::EntryMarker> Sync for IoUring<S, C> {}

impl IoUring<squeue::Entry, cqueue::Entry> {
    /// Create a new `IoUring` instance with default configuration parameters. See [`Builder`] to
    /// customize it further.
    ///
    /// The `entries` sets the size of queue,
    /// and its value should be the power of two.
    pub fn new(entries: u32) -> io::Result<Self> {
        Self::builder().build(entries)
    }

    /// Create an `IoUring` instance from a pre-opened file descriptor.
    ///
    /// # Safety
    ///
    /// The caller must uphold that the file descriptor is owned and refers to a uring. The
    /// `params` argument must be equivalent to the those previously filled in by the kernel when
    /// the provided ring was created.
    pub unsafe fn from_fd(fd: RawFd, params: Parameters) -> io::Result<Self> {
        Self::with_fd_and_params(OwnedFd::from_raw_fd(fd), params.0)
    }
}

impl<S: squeue::EntryMarker, C: cqueue::EntryMarker> IoUring<S, C> {
    /// Create a [`Builder`] for an `IoUring` instance.
    ///
    /// This allows for further customization than [`new`](Self::new).
    ///
    /// Unlike [`IoUring::new`], this function is available for any combination of submission
    /// queue entry (SQE) and completion queue entry (CQE) types.
    #[must_use]
    pub fn builder() -> Builder<S, C> {
        Builder {
            dontfork: false,
            params: sys::io_uring_params {
                flags: S::BUILD_FLAGS | C::BUILD_FLAGS,
                ..Default::default()
            },
            phantom: PhantomData,
        }
    }

    fn with_params(entries: u32, mut p: sys::io_uring_params) -> io::Result<Self> {
        let fd: OwnedFd = unsafe { OwnedFd::from_raw_fd(sys::io_uring_setup(entries, &mut p)?) };
        unsafe { Self::with_fd_and_params(fd, p) }
    }

    unsafe fn with_fd_and_params(fd: OwnedFd, p: sys::io_uring_params) -> io::Result<Self> {
        // NOTE: The `SubmissionQueue` and `CompletionQueue` are references,
        // and their lifetime can never exceed `MemoryMap`.
        //
        // The memory mapped regions of `MemoryMap` never move,
        // so `SubmissionQueue` and `CompletionQueue` are `Unpin`.
        //
        // I really hope that Rust can safely use self-reference types.
        #[inline]
        unsafe fn setup_queue<S: squeue::EntryMarker, C: cqueue::EntryMarker>(
            fd: &OwnedFd,
            p: &sys::io_uring_params,
        ) -> io::Result<(MemoryMap, squeue::Inner<S>, cqueue::Inner<C>)> {
            let sq_len = p.sq_off.array as usize + p.sq_entries as usize * mem::size_of::<u32>();
            let cq_len = p.cq_off.cqes as usize + p.cq_entries as usize * mem::size_of::<C>();
            let sqe_len = p.sq_entries as usize * mem::size_of::<S>();
            let sqe_mmap = Mmap::new(fd, sys::IORING_OFF_SQES as _, sqe_len)?;

            if p.features & sys::IORING_FEAT_SINGLE_MMAP != 0 {
                let scq_mmap =
                    Mmap::new(fd, sys::IORING_OFF_SQ_RING as _, cmp::max(sq_len, cq_len))?;

                let sq = squeue::Inner::new(&scq_mmap, &sqe_mmap, p);
                let cq = cqueue::Inner::new(&scq_mmap, p);
                let mm = MemoryMap {
                    sq_mmap: scq_mmap,
                    cq_mmap: None,
                    sqe_mmap,
                };

                Ok((mm, sq, cq))
            } else {
                let sq_mmap = Mmap::new(fd, sys::IORING_OFF_SQ_RING as _, sq_len)?;
                let cq_mmap = Mmap::new(fd, sys::IORING_OFF_CQ_RING as _, cq_len)?;

                let sq = squeue::Inner::new(&sq_mmap, &sqe_mmap, p);
                let cq = cqueue::Inner::new(&cq_mmap, p);
                let mm = MemoryMap {
                    cq_mmap: Some(cq_mmap),
                    sq_mmap,
                    sqe_mmap,
                };

                Ok((mm, sq, cq))
            }
        }

        let (mm, sq, cq) = unsafe { setup_queue(&fd, &p)? };

        Ok(IoUring {
            sq,
            cq,
            fd,
            params: Parameters(p),
            memory: ManuallyDrop::new(mm),
        })
    }

    /// Get the submitter of this io_uring instance, which can be used to submit submission queue
    /// events to the kernel for execution and to register files or buffers with it.
    #[inline]
    pub fn submitter(&self) -> Submitter<'_> {
        Submitter::new(
            &self.fd,
            &self.params,
            self.sq.head,
            self.sq.tail,
            self.sq.flags,
        )
    }

    /// Get the parameters that were used to construct this instance.
    #[inline]
    pub fn params(&self) -> &Parameters {
        &self.params
    }

    /// Initiate asynchronous I/O. See [`Submitter::submit`] for more details.
    #[inline]
    pub fn submit(&self) -> io::Result<usize> {
        self.submitter().submit()
    }

    /// Initiate and/or complete asynchronous I/O. See [`Submitter::submit_and_wait`] for more
    /// details.
    #[inline]
    pub fn submit_and_wait(&self, want: usize) -> io::Result<usize> {
        self.submitter().submit_and_wait(want)
    }

    /// Get the submitter, submission queue and completion queue of the io_uring instance. This can
    /// be used to operate on the different parts of the io_uring instance independently.
    ///
    /// If you use this method to obtain `sq` and `cq`,
    /// please note that you need to `drop` or `sync` the queue before and after submit,
    /// otherwise the queue will not be updated.
    #[inline]
    pub fn split(
        &mut self,
    ) -> (
        Submitter<'_>,
        SubmissionQueue<'_, S>,
        CompletionQueue<'_, C>,
    ) {
        let submit = Submitter::new(
            &self.fd,
            &self.params,
            self.sq.head,
            self.sq.tail,
            self.sq.flags,
        );
        (submit, self.sq.borrow(), self.cq.borrow())
    }

    /// Get the submission queue of the io_uring instance. This is used to send I/O requests to the
    /// kernel.
    #[inline]
    pub fn submission(&mut self) -> SubmissionQueue<'_, S> {
        self.sq.borrow()
    }

    /// Get the submission queue of the io_uring instance from a shared reference.
    ///
    /// # Safety
    ///
    /// No other [`SubmissionQueue`]s may exist when calling this function.
    #[inline]
    pub unsafe fn submission_shared(&self) -> SubmissionQueue<'_, S> {
        self.sq.borrow_shared()
    }

    /// Get completion queue of the io_uring instance. This is used to receive I/O completion
    /// events from the kernel.
    #[inline]
    pub fn completion(&mut self) -> CompletionQueue<'_, C> {
        self.cq.borrow()
    }

    /// Get the completion queue of the io_uring instance from a shared reference.
    ///
    /// # Safety
    ///
    /// No other [`CompletionQueue`]s may exist when calling this function.
    #[inline]
    pub unsafe fn completion_shared(&self) -> CompletionQueue<'_, C> {
        self.cq.borrow_shared()
    }
}

impl<S: squeue::EntryMarker, C: cqueue::EntryMarker> Drop for IoUring<S, C> {
    fn drop(&mut self) {
        // Ensure that `MemoryMap` is released before `fd`.
        unsafe {
            ManuallyDrop::drop(&mut self.memory);
        }
    }
}

impl<S: squeue::EntryMarker, C: cqueue::EntryMarker> Builder<S, C> {
    /// Do not make this io_uring instance accessible by child processes after a fork.
    pub fn dontfork(&mut self) -> &mut Self {
        self.dontfork = true;
        self
    }

    /// Perform busy-waiting for I/O completion events, as opposed to getting notifications via an
    /// asynchronous IRQ (Interrupt Request). This will reduce latency, but increases CPU usage.
    ///
    /// This is only usable on file systems that support polling and files opened with `O_DIRECT`.
    pub fn setup_iopoll(&mut self) -> &mut Self {
        self.params.flags |= sys::IORING_SETUP_IOPOLL;
        self
    }

    /// Use a kernel thread to perform submission queue polling. This allows your application to
    /// issue I/O without ever context switching into the kernel, however it does use up a lot more
    /// CPU. You should use it when you are expecting very large amounts of I/O.
    ///
    /// After `idle` milliseconds, the kernel thread will go to sleep and you will have to wake it up
    /// again with a system call (this is handled by [`Submitter::submit`] and
    /// [`Submitter::submit_and_wait`] automatically).
    ///
    /// Before version 5.11 of the Linux kernel, to successfully use this feature, the application
    /// must register a set of files to be used for IO through io_uring_register(2) using the
    /// IORING_REGISTER_FILES opcode. Failure to do so will result in submitted IO being errored
    /// with EBADF. The presence of this feature can be detected by the IORING_FEAT_SQPOLL_NONFIXED
    /// feature flag. In version 5.11 and later, it is no longer necessary to register files to use
    /// this feature. 5.11 also allows using this as non-root, if the user has the CAP_SYS_NICE
    /// capability. In 5.13 this requirement was also relaxed, and no special privileges are needed
    /// for SQPOLL in newer kernels. Certain stable kernels older than 5.13 may also support
    /// unprivileged SQPOLL.
    pub fn setup_sqpoll(&mut self, idle: u32) -> &mut Self {
        self.params.flags |= sys::IORING_SETUP_SQPOLL;
        self.params.sq_thread_idle = idle;
        self
    }

    /// Bind the kernel's poll thread to the specified cpu. This flag is only meaningful when
    /// [`Builder::setup_sqpoll`] is enabled.
    pub fn setup_sqpoll_cpu(&mut self, cpu: u32) -> &mut Self {
        self.params.flags |= sys::IORING_SETUP_SQ_AFF;
        self.params.sq_thread_cpu = cpu;
        self
    }

    /// Create the completion queue with the specified number of entries. The value must be greater
    /// than `entries`, and may be rounded up to the next power-of-two.
    pub fn setup_cqsize(&mut self, entries: u32) -> &mut Self {
        self.params.flags |= sys::IORING_SETUP_CQSIZE;
        self.params.cq_entries = entries;
        self
    }

    /// Clamp the sizes of the submission queue and completion queue at their maximum values instead
    /// of returning an error when you attempt to resize them beyond their maximum values.
    pub fn setup_clamp(&mut self) -> &mut Self {
        self.params.flags |= sys::IORING_SETUP_CLAMP;
        self
    }

    /// Share the asynchronous worker thread backend of this io_uring with the specified io_uring
    /// file descriptor instead of creating a new thread pool.
    pub fn setup_attach_wq(&mut self, fd: RawFd) -> &mut Self {
        self.params.flags |= sys::IORING_SETUP_ATTACH_WQ;
        self.params.wq_fd = fd as _;
        self
    }

    /// Start the io_uring instance with all its rings disabled. This allows you to register
    /// restrictions, buffers and files before the kernel starts processing submission queue
    /// events. You are only able to [register restrictions](Submitter::register_restrictions) when
    /// the rings are disabled due to concurrency issues. You can enable the rings with
    /// [`Submitter::register_enable_rings`]. Available since 5.10.
    pub fn setup_r_disabled(&mut self) -> &mut Self {
        self.params.flags |= sys::IORING_SETUP_R_DISABLED;
        self
    }

    /// Normally io_uring stops submitting a batch of request, if one of these requests results in
    /// an error. This can cause submission of less than what is expected, if a request ends in
    /// error while being submitted. If the ring is created with this flag, io_uring_enter(2) will
    /// continue submitting requests even if it encounters an error submitting a request. CQEs are
    /// still posted for errored request regardless of whether or not this flag is set at ring
    /// creation time, the only difference is if the submit sequence is halted or continued when an
    /// error is observed. Available since 5.18.
    pub fn setup_submit_all(&mut self) -> &mut Self {
        self.params.flags |= sys::IORING_SETUP_SUBMIT_ALL;
        self
    }

    /// By default, io_uring will interrupt a task running in userspace when a completion event
    /// comes in. This is to ensure that completions run in a timely manner. For a lot of use
    /// cases, this is overkill and can cause reduced performance from both the inter-processor
    /// interrupt used to do this, the kernel/user transition, the needless interruption of the
    /// tasks userspace activities, and reduced batching if completions come in at a rapid rate.
    /// Most applications don't need the forceful interruption, as the events are processed at any
    /// kernel/user transition. The exception are setups where the application uses multiple
    /// threads operating on the same ring, where the application waiting on completions isn't the
    /// one that submitted them. For most other use cases, setting this flag will improve
    /// performance. Available since 5.19.
    pub fn setup_coop_taskrun(&mut self) -> &mut Self {
        self.params.flags |= sys::IORING_SETUP_COOP_TASKRUN;
        self
    }

    /// Used in conjunction with IORING_SETUP_COOP_TASKRUN, this provides a flag,
    /// IORING_SQ_TASKRUN, which is set in the SQ ring flags whenever completions are pending that
    /// should be processed. As an example, liburing will check for this flag even when doing
    /// io_uring_peek_cqe(3) and enter the kernel to process them, and applications can do the
    /// same. This makes IORING_SETUP_TASKRUN_FLAG safe to use even when applications rely on a
    /// peek style operation on the CQ ring to see if anything might be pending to reap. Available
    /// since 5.19.
    pub fn setup_taskrun_flag(&mut self) -> &mut Self {
        self.params.flags |= sys::IORING_SETUP_TASKRUN_FLAG;
        self
    }

    /// By default, io_uring will process all outstanding work at the end of any system call or
    /// thread interrupt. This can delay the application from making other progress. Setting this
    /// flag will hint to io_uring that it should defer work until an io_uring_enter(2) call with
    /// the IORING_ENTER_GETEVENTS flag set. This allows the application to request work to run
    /// just just before it wants to process completions. This flag requires the
    /// IORING_SETUP_SINGLE_ISSUER flag to be set, and also enforces that the call to
    /// io_uring_enter(2) is called from the same thread that submitted requests. Note that if this
    /// flag is set then it is the application's responsibility to periodically trigger work (for
    /// example via any of the CQE waiting functions) or else completions may not be delivered.
    /// Available since 6.1.
    pub fn setup_defer_taskrun(&mut self) -> &mut Self {
        self.params.flags |= sys::IORING_SETUP_DEFER_TASKRUN;
        self
    }

    /// Hint the kernel that a single task will submit requests. Used for optimizations. This is
    /// enforced by the kernel, and request that don't respect that will fail with -EEXIST.
    /// If [`Builder::setup_sqpoll`] is enabled, the polling task is doing the submissions and multiple
    /// userspace tasks can call [`Submitter::enter`] and higher level APIs. Available since 6.0.
    pub fn setup_single_issuer(&mut self) -> &mut Self {
        self.params.flags |= sys::IORING_SETUP_SINGLE_ISSUER;
        self
    }

    /// Build an [IoUring], with the specified number of entries in the submission queue and
    /// completion queue unless [`setup_cqsize`](Self::setup_cqsize) has been called.
    pub fn build(&self, entries: u32) -> io::Result<IoUring<S, C>> {
        let ring = IoUring::with_params(entries, self.params)?;

        if self.dontfork {
            ring.memory.sq_mmap.dontfork()?;
            ring.memory.sqe_mmap.dontfork()?;
            if let Some(cq_mmap) = ring.memory.cq_mmap.as_ref() {
                cq_mmap.dontfork()?;
            }
        }

        Ok(ring)
    }
}

impl Parameters {
    /// Whether a kernel thread is performing queue polling. Enabled with [`Builder::setup_sqpoll`].
    pub fn is_setup_sqpoll(&self) -> bool {
        self.0.flags & sys::IORING_SETUP_SQPOLL != 0
    }

    /// Whether waiting for completion events is done with a busy loop instead of using IRQs.
    /// Enabled with [`Builder::setup_iopoll`].
    pub fn is_setup_iopoll(&self) -> bool {
        self.0.flags & sys::IORING_SETUP_IOPOLL != 0
    }

    /// Whether the single issuer hint is enabled. Enabled with [`Builder::setup_single_issuer`].
    pub fn is_setup_single_issuer(&self) -> bool {
        self.0.flags & sys::IORING_SETUP_SINGLE_ISSUER != 0
    }

    /// If this flag is set, the SQ and CQ rings were mapped with a single `mmap(2)` call. This
    /// means that only two syscalls were used instead of three.
    pub fn is_feature_single_mmap(&self) -> bool {
        self.0.features & sys::IORING_FEAT_SINGLE_MMAP != 0
    }

    /// If this flag is set, io_uring supports never dropping completion events. If a completion
    /// event occurs and the CQ ring is full, the kernel stores the event internally until such a
    /// time that the CQ ring has room for more entries.
    pub fn is_feature_nodrop(&self) -> bool {
        self.0.features & sys::IORING_FEAT_NODROP != 0
    }

    /// If this flag is set, applications can be certain that any data for async offload has been
    /// consumed when the kernel has consumed the SQE.
    pub fn is_feature_submit_stable(&self) -> bool {
        self.0.features & sys::IORING_FEAT_SUBMIT_STABLE != 0
    }

    /// If this flag is set, applications can specify offset == -1 with [`Readv`](opcode::Readv),
    /// [`Writev`](opcode::Writev), [`ReadFixed`](opcode::ReadFixed),
    /// [`WriteFixed`](opcode::WriteFixed), [`Read`](opcode::Read) and [`Write`](opcode::Write),
    /// which behaves exactly like setting offset == -1 in `preadv2(2)` and `pwritev2(2)`: it’ll use
    /// (and update) the current file position.
    ///
    /// This obviously comes with the caveat that if the application has multiple reads or writes in flight,
    /// then the end result will not be as expected.
    /// This is similar to threads sharing a file descriptor and doing IO using the current file position.
    pub fn is_feature_rw_cur_pos(&self) -> bool {
        self.0.features & sys::IORING_FEAT_RW_CUR_POS != 0
    }

    /// If this flag is set, then io_uring guarantees that both sync and async execution of
    /// a request assumes the credentials of the task that called [`Submitter::enter`] to queue the requests.
    /// If this flag isn’t set, then requests are issued with the credentials of the task that originally registered the io_uring.
    /// If only one task is using a ring, then this flag doesn’t matter as the credentials will always be the same.
    ///
    /// Note that this is the default behavior, tasks can still register different personalities
    /// through [`Submitter::register_personality`].
    pub fn is_feature_cur_personality(&self) -> bool {
        self.0.features & sys::IORING_FEAT_CUR_PERSONALITY != 0
    }

    /// Whether async pollable I/O is fast.
    ///
    /// See [the commit message that introduced
    /// it](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/commit/?id=d7718a9d25a61442da8ee8aeeff6a0097f0ccfd6)
    /// for more details.
    ///
    /// If this flag is set, then io_uring supports using an internal poll mechanism to drive
    /// data/space readiness. This means that requests that cannot read or write data to a file no
    /// longer need to be punted to an async thread for handling, instead they will begin operation
    /// when the file is ready. This is similar to doing poll + read/write in userspace, but
    /// eliminates the need to do so. If this flag is set, requests waiting on space/data consume a
    /// lot less resources doing so as they are not blocking a thread. Available since kernel 5.7.
    pub fn is_feature_fast_poll(&self) -> bool {
        self.0.features & sys::IORING_FEAT_FAST_POLL != 0
    }

    /// Whether poll events are stored using 32 bits instead of 16. This allows the user to use
    /// `EPOLLEXCLUSIVE`.
    ///
    /// If this flag is set, the IORING_OP_POLL_ADD command accepts the full 32-bit range of epoll
    /// based flags. Most notably EPOLLEXCLUSIVE which allows exclusive (waking single waiters)
    /// behavior. Available since kernel 5.9.
    pub fn is_feature_poll_32bits(&self) -> bool {
        self.0.features & sys::IORING_FEAT_POLL_32BITS != 0
    }

    /// If this flag is set, the IORING_SETUP_SQPOLL feature no longer requires the use of fixed
    /// files. Any normal file descriptor can be used for IO commands without needing registration.
    /// Available since kernel 5.11.
    pub fn is_feature_sqpoll_nonfixed(&self) -> bool {
        self.0.features & sys::IORING_FEAT_SQPOLL_NONFIXED != 0
    }

    /// If this flag is set, then the io_uring_enter(2) system call supports passing in an extended
    /// argument instead of just the sigset_t of earlier kernels. This extended argument is of type
    /// struct io_uring_getevents_arg and allows the caller to pass in both a sigset_t and a
    /// timeout argument for waiting on events. The struct layout is as follows:
    ///
    /// // struct io_uring_getevents_arg {
    /// //     __u64 sigmask;
    /// //     __u32 sigmask_sz;
    /// //     __u32 pad;
    /// //     __u64 ts;
    /// // };
    ///
    /// and a pointer to this struct must be passed in if IORING_ENTER_EXT_ARG is set in the flags
    /// for the enter system call. Available since kernel 5.11.
    pub fn is_feature_ext_arg(&self) -> bool {
        self.0.features & sys::IORING_FEAT_EXT_ARG != 0
    }

    /// If this flag is set, io_uring is using native workers for its async helpers. Previous
    /// kernels used kernel threads that assumed the identity of the original io_uring owning task,
    /// but later kernels will actively create what looks more like regular process threads
    /// instead. Available since kernel 5.12.
    pub fn is_feature_native_workers(&self) -> bool {
        self.0.features & sys::IORING_FEAT_NATIVE_WORKERS != 0
    }

    /// Whether the kernel supports tagging resources.
    ///
    /// If this flag is set, then io_uring supports a variety of features related to fixed files
    /// and buffers. In particular, it indicates that registered buffers can be updated in-place,
    /// whereas before the full set would have to be unregistered first. Available since kernel
    /// 5.13.
    pub fn is_feature_resource_tagging(&self) -> bool {
        self.0.features & sys::IORING_FEAT_RSRC_TAGS != 0
    }

    /// Whether the kernel supports `IOSQE_CQE_SKIP_SUCCESS`.
    ///
    /// This feature allows skipping the generation of a CQE if a SQE executes normally. Available
    /// since kernel 5.17.
    pub fn is_feature_skip_cqe_on_success(&self) -> bool {
        self.0.features & sys::IORING_FEAT_CQE_SKIP != 0
    }

    /// Whether the kernel supports deferred file assignment.
    ///
    /// If this flag is set, then io_uring supports sane assignment of files for SQEs that have
    /// dependencies. For example, if a chain of SQEs are submitted with IOSQE_IO_LINK, then
    /// kernels without this flag will prepare the file for each link upfront. If a previous link
    /// opens a file with a known index, eg if direct descriptors are used with open or accept,
    /// then file assignment needs to happen post execution of that SQE. If this flag is set, then
    /// the kernel will defer file assignment until execution of a given request is started.
    /// Available since kernel 5.17.
    pub fn is_feature_linked_file(&self) -> bool {
        self.0.features & sys::IORING_FEAT_LINKED_FILE != 0
    }

    /// Whether the kernel supports `IORING_RECVSEND_BUNDLE`.
    ///
    /// This feature allows sending and recieving multiple buffers as a single bundle. Available
    /// since kernel 6.10.
    pub fn is_feature_recvsend_bundle(&self) -> bool {
        self.0.features & sys::IORING_FEAT_RECVSEND_BUNDLE != 0
    }

    /// The number of submission queue entries allocated.
    pub fn sq_entries(&self) -> u32 {
        self.0.sq_entries
    }

    /// The idle time of the SQ poll thread in milliseconds.
    pub fn sq_thread_idle(&self) -> u32 {
        self.0.sq_thread_idle
    }

    /// The number of completion queue entries allocated.
    pub fn cq_entries(&self) -> u32 {
        self.0.cq_entries
    }
}

impl std::fmt::Debug for Parameters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parameters")
            .field("is_setup_sqpoll", &self.is_setup_sqpoll())
            .field("is_setup_iopoll", &self.is_setup_iopoll())
            .field("is_setup_single_issuer", &self.is_setup_single_issuer())
            .field("is_feature_single_mmap", &self.is_feature_single_mmap())
            .field("is_feature_nodrop", &self.is_feature_nodrop())
            .field("is_feature_submit_stable", &self.is_feature_submit_stable())
            .field("is_feature_rw_cur_pos", &self.is_feature_rw_cur_pos())
            .field(
                "is_feature_cur_personality",
                &self.is_feature_cur_personality(),
            )
            .field("is_feature_poll_32bits", &self.is_feature_poll_32bits())
            .field("sq_entries", &self.0.sq_entries)
            .field("cq_entries", &self.0.cq_entries)
            .finish()
    }
}

impl<S: squeue::EntryMarker, C: cqueue::EntryMarker> AsRawFd for IoUring<S, C> {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

#[cfg(feature = "io_safety")]
impl<S: squeue::EntryMarker, C: cqueue::EntryMarker> AsFd for IoUring<S, C> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}
