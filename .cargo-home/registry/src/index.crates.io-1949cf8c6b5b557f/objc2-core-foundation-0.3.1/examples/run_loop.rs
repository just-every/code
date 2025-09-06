//! Creating and attaching various runloop objects.

use std::sync::Arc;
use std::time::Duration;
use std::{cell::Cell, ffi::c_void};

use objc2_core_foundation::{
    kCFAllocatorDefault, kCFRunLoopCommonModes, CFAbsoluteTime, CFAbsoluteTimeGetCurrent, CFIndex,
    CFRetained, CFRunLoop, CFRunLoopActivity, CFRunLoopMode, CFRunLoopObserver,
    CFRunLoopObserverContext, CFRunLoopSource, CFRunLoopSourceContext, CFRunLoopTimer,
    CFRunLoopTimerContext, CFTimeInterval,
};

fn main() {
    let rl = CFRunLoop::main().unwrap();

    // Add an observer.
    let observer = create_observer(
        CFRunLoopActivity::AllActivities,
        true,
        0,
        |_observer, activity| match activity {
            CFRunLoopActivity::Entry => println!("-- observed: Entry"),
            CFRunLoopActivity::BeforeTimers => println!("-- observed: BeforeTimers"),
            CFRunLoopActivity::BeforeSources => println!("-- observed: BeforeSources"),
            CFRunLoopActivity::BeforeWaiting => println!("-- observed: BeforeWaiting"),
            CFRunLoopActivity::AfterWaiting => println!("-- observed: AfterWaiting"),
            CFRunLoopActivity::Exit => println!("-- observed: Exit"),
            _ => eprintln!("unknown observer activity"),
        },
    );
    rl.add_observer(Some(&observer), unsafe { kCFRunLoopCommonModes });

    // Add a simple timer.
    let iterations = Cell::new(0);
    let callback = {
        let rl = rl.clone();
        move |timer: &CFRunLoopTimer| {
            println!("Timer called: {}", iterations.get());

            if 10 <= iterations.get() {
                // Remove the timer.
                rl.remove_timer(Some(timer), unsafe { kCFRunLoopCommonModes });

                // Stop the run loop explicitly
                // (the main run-loop won't stop otherwise).
                println!("Stopping run loop");
                rl.stop();
            }

            iterations.set(iterations.get() + 1);
        }
    };
    // SAFETY: The timer is added to a run loop on the same thread.
    let timer = unsafe { create_timer_unchecked(0.0, 0.1, 0, callback) };
    rl.add_timer(Some(&timer), unsafe { kCFRunLoopCommonModes });

    // Add a single-shot timer.
    let fire_date = CFAbsoluteTimeGetCurrent() + 0.5;
    let timer = create_timer(fire_date, 0.0, 0, |_| {
        println!("Fired one-shot timer after 0.5 seconds");
        // Still runs on the main thread.
        assert_eq!(CFRunLoop::current().unwrap(), CFRunLoop::main().unwrap());
    });
    rl.add_timer(Some(&timer), unsafe { kCFRunLoopCommonModes });

    // Add an external source.
    let source = create_source(0, |data| match data {
        SourceData::Schedule { mode, .. } => println!("source added to runloop mode {mode}"),
        SourceData::Perform => println!("performing source!"),
        SourceData::Cancel { mode, .. } => {
            println!("source removed from runloop mode {mode}")
        }
    });
    rl.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });

    // Add a thread that signals the source.
    let source = {
        struct Wrapper(CFRetained<CFRunLoopSource>);
        // SAFETY: Still under discussion, see:
        // https://github.com/madsmtm/objc2/issues/696
        unsafe impl Send for Wrapper {}
        Wrapper(source)
    };
    let handle = std::thread::spawn(move || {
        let source = source;
        std::thread::sleep(Duration::from_millis(200));
        println!("signalling first time");
        source.0.signal();
        println!("signalling second time");
        source.0.signal();
        println!("signalling third time");
        source.0.signal();
        std::thread::sleep(Duration::from_millis(500));
        println!("signalling fourth time");
        source.0.signal();
        source.0.invalidate();
    });

    println!("Starting run loop");
    CFRunLoop::run();

    handle.join().unwrap();
}

// -------------------------------------------------------------------------
// Generic runloop helpers.
//
// These will probably become part of `objc2-core-foundation` one day, but
// more work is needed on the following issue first:
// https://github.com/madsmtm/objc2/issues/696
// -------------------------------------------------------------------------

/// Create a new `CFRunLoopObserver`.
fn create_observer<F: Fn(&CFRunLoopObserver, CFRunLoopActivity) + Send + Sync + 'static>(
    activities: CFRunLoopActivity,
    repeats: bool,
    order: CFIndex,
    callback: F,
) -> CFRetained<CFRunLoopObserver> {
    // SAFETY: The callback is `Send + Sync`.
    unsafe { create_observer_unchecked(activities, repeats, order, callback) }
}

/// Create a new `CFRunLoopObserver`, without enforcing the callback to be
/// thread-safe.
///
/// # Safety
///
/// The callback must be either `Send` + `Sync`, or the observer must only be
/// added to a run loop that runs on the current thread.
unsafe fn create_observer_unchecked<F: Fn(&CFRunLoopObserver, CFRunLoopActivity) + 'static>(
    activities: CFRunLoopActivity,
    repeats: bool,
    order: CFIndex,
    callback: F,
) -> CFRetained<CFRunLoopObserver> {
    // We use an `Arc` here to make sure that the reference-counting of the
    // signal container is atomic (`Retained`/`CFRetained` would be valid
    // alternatives too).
    let callback = Arc::new(callback);

    unsafe extern "C-unwind" fn retain<F>(info: *const c_void) -> *const c_void {
        // SAFETY: The pointer was passed to `CFRunLoopObserverContext.info` below.
        unsafe { Arc::increment_strong_count(info.cast::<F>()) };
        info
    }
    unsafe extern "C-unwind" fn release<F>(info: *const c_void) {
        // SAFETY: The pointer was passed to `CFRunLoopObserverContext.info` below.
        unsafe { Arc::decrement_strong_count(info.cast::<F>()) };
    }

    unsafe extern "C-unwind" fn callout<F: Fn(&CFRunLoopObserver, CFRunLoopActivity)>(
        observer: *mut CFRunLoopObserver,
        activity: CFRunLoopActivity,
        info: *mut c_void,
    ) {
        // SAFETY: The observer is valid for at least the duration of the callback.
        let observer = unsafe { &*observer };

        // SAFETY: The pointer was passed to `CFRunLoopObserverContext.info` below.
        let callback = unsafe { &*info.cast::<F>() };

        // Call the provided closure.
        callback(observer, activity);
    }

    // This is marked `mut` to match the signature of `CFRunLoopObserver::new`,
    // but the information is copied, and not actually mutated.
    let mut context = CFRunLoopObserverContext {
        version: 0,
        // This pointer is retained by CF on creation.
        info: Arc::as_ptr(&callback) as *mut c_void,
        retain: Some(retain::<F>),
        release: Some(release::<F>),
        copyDescription: None,
    };

    // SAFETY: The retain/release callbacks are thread-safe, and caller
    // upholds that the main callback is used in a thread-safe manner.
    //
    // `F: 'static`, so extending the lifetime of the closure is fine.
    unsafe {
        CFRunLoopObserver::new(
            kCFAllocatorDefault,
            activities.0,
            repeats,
            order,
            Some(callout::<F>),
            &mut context,
        )
    }
    .unwrap()
}

/// Create a new `CFRunLoopTimer`.
fn create_timer<F: Fn(&CFRunLoopTimer) + Send + Sync + 'static>(
    fire_date: CFAbsoluteTime,
    interval: CFTimeInterval,
    order: CFIndex,
    callback: F,
) -> CFRetained<CFRunLoopTimer> {
    // SAFETY: The callback is `Send + Sync`.
    unsafe { create_timer_unchecked(fire_date, interval, order, callback) }
}

/// Create a new `CFRunLoopTimer`, without enforcing the callback to be
/// thread-safe.
///
/// # Safety
///
/// The callback must be either `Send` + `Sync`, or the timer must only be
/// added to a run loop that runs on the current thread.
unsafe fn create_timer_unchecked<F: Fn(&CFRunLoopTimer) + 'static>(
    fire_date: CFAbsoluteTime,
    interval: CFTimeInterval,
    order: CFIndex,
    callback: F,
) -> CFRetained<CFRunLoopTimer> {
    // We use an `Arc` here to make sure that the reference-counting of the
    // signal container is atomic (`Retained`/`CFRetained` would be valid
    // alternatives too).
    let callback = Arc::new(callback);

    unsafe extern "C-unwind" fn retain<F>(info: *const c_void) -> *const c_void {
        // SAFETY: The pointer was passed to `CFRunLoopTimerContext.info` below.
        unsafe { Arc::increment_strong_count(info.cast::<F>()) };
        info
    }
    unsafe extern "C-unwind" fn release<F>(info: *const c_void) {
        // SAFETY: The pointer was passed to `CFRunLoopTimerContext.info` below.
        unsafe { Arc::decrement_strong_count(info.cast::<F>()) };
    }

    unsafe extern "C-unwind" fn callout<F: Fn(&CFRunLoopTimer)>(
        timer: *mut CFRunLoopTimer,
        info: *mut c_void,
    ) {
        // SAFETY: The timer is valid for at least the duration of the callback.
        let timer = unsafe { &*timer };

        // SAFETY: The pointer was passed to `CFRunLoopTimerContext.info` below.
        let callback = unsafe { &*info.cast::<F>() };

        // Call the provided closure.
        callback(timer);
    }

    // This is marked `mut` to match the signature of `CFRunLoopTimer::new`,
    // but the information is copied, and not actually mutated.
    let mut context = CFRunLoopTimerContext {
        version: 0,
        // This pointer is retained by CF on creation.
        info: Arc::as_ptr(&callback) as *mut c_void,
        retain: Some(retain::<F>),
        release: Some(release::<F>),
        copyDescription: None,
    };

    // SAFETY: The retain/release callbacks are thread-safe, and caller
    // upholds that the main callback is used in a thread-safe manner.
    //
    // `F: 'static`, so extending the lifetime of the closure is fine.
    unsafe {
        CFRunLoopTimer::new(
            kCFAllocatorDefault,
            fire_date,
            interval,
            0, // Documentation says to pass 0 for future compat.
            order,
            Some(callout::<F>),
            &mut context,
        )
    }
    .unwrap()
}

/// Data received in source callbacks.
#[derive(Debug)]
#[allow(dead_code)]
enum SourceData<'a> {
    Schedule {
        rl: &'a CFRunLoop,
        mode: &'a CFRunLoopMode,
    },
    Perform,
    Cancel {
        rl: &'a CFRunLoop,
        mode: &'a CFRunLoopMode,
    },
}

/// Create a new "version 0" `CFRunLoopSource`.
fn create_source<F: Fn(SourceData<'_>) + Send + Sync + 'static>(
    order: CFIndex,
    callback: F,
) -> CFRetained<CFRunLoopSource> {
    // We use an `Arc` here to make sure that the reference-counting of the
    // signal container is atomic (`Retained`/`CFRetained` would be valid
    // alternatives too).
    let callback = Arc::new(callback);

    unsafe extern "C-unwind" fn retain<F>(info: *const c_void) -> *const c_void {
        // SAFETY: The pointer was passed to `CFRunLoopSourceContext.info` below.
        unsafe { Arc::increment_strong_count(info.cast::<F>()) };
        info
    }
    unsafe extern "C-unwind" fn release<F>(info: *const c_void) {
        // SAFETY: The pointer was passed to `CFRunLoopSourceContext.info` below.
        unsafe { Arc::decrement_strong_count(info.cast::<F>()) };
    }

    // Pointer equality / hashing for the Arc.
    #[allow(clippy::ptr_eq)]
    extern "C-unwind" fn equal(info1: *const c_void, info2: *const c_void) -> u8 {
        (info1 == info2) as u8
    }
    extern "C-unwind" fn hash(info: *const c_void) -> usize {
        info as usize
    }

    unsafe extern "C-unwind" fn schedule<F: Fn(SourceData<'_>)>(
        info: *mut c_void,
        rl: *mut CFRunLoop,
        mode: *const CFRunLoopMode,
    ) {
        // SAFETY: The data is valid for at least the duration of the callback.
        let rl = unsafe { &*rl };
        let mode = unsafe { &*mode };

        // SAFETY: The pointer was passed to `CFRunLoopSourceContext.info` below.
        let signaller = unsafe { &*info.cast::<F>() };
        (signaller)(SourceData::Schedule { rl, mode });
    }

    unsafe extern "C-unwind" fn cancel<F: Fn(SourceData<'_>)>(
        info: *mut c_void,
        rl: *mut CFRunLoop,
        mode: *const CFRunLoopMode,
    ) {
        // SAFETY: The data is valid for at least the duration of the callback.
        let rl = unsafe { &*rl };
        let mode = unsafe { &*mode };

        // SAFETY: The pointer was passed to `CFRunLoopSourceContext.info` below.
        let signaller = unsafe { &*info.cast::<F>() };
        (signaller)(SourceData::Cancel { rl, mode });
    }

    unsafe extern "C-unwind" fn perform<F: Fn(SourceData<'_>)>(info: *mut c_void) {
        // SAFETY: The pointer was passed to `CFRunLoopSourceContext.info` below.
        let signaller = unsafe { &*info.cast::<F>() };
        (signaller)(SourceData::Perform);
    }

    // This is marked `mut` to match the signature of `CFRunLoopSource::new`,
    // but the information is copied, and not actually mutated.
    let mut context = CFRunLoopSourceContext {
        version: 0, // Version 0 source
        // This pointer is retained by CF on creation.
        info: Arc::as_ptr(&callback) as *mut c_void,
        retain: Some(retain::<F>),
        release: Some(release::<F>),
        copyDescription: None,
        equal: Some(equal),
        hash: Some(hash),
        schedule: Some(schedule::<F>),
        cancel: Some(cancel::<F>),
        perform: Some(perform::<F>),
    };

    // SAFETY: The retain/release callbacks are thread-safe, and F is marked
    // with `Send + Sync`, so that is thread-safe too.
    //
    // `F: 'static`, so extending the lifetime of the closure is fine.
    unsafe { CFRunLoopSource::new(kCFAllocatorDefault, order, &mut context) }.unwrap()
}
