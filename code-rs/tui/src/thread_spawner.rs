use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

// Background tasks occasionally spin up Tokio runtimes or TLS stacks; keep a
// modest stack while avoiding stack overflow in heavier workers.
const STACK_SIZE_BYTES: usize = 1024 * 1024;
const MAX_BACKGROUND_THREADS: usize = 32;
const LIMIT_LOG_THROTTLE_SECS: u64 = 5;

static ACTIVE_THREADS: AtomicUsize = AtomicUsize::new(0);
static LAST_LIMIT_LOG_SECS: AtomicU64 = AtomicU64::new(0);

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

struct ThreadCountGuard;

impl ThreadCountGuard {
    fn new() -> Self {
        Self
    }
}

impl Drop for ThreadCountGuard {
    fn drop(&mut self) {
        ACTIVE_THREADS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Lightweight helper to spawn background threads with a lower stack size and
/// a descriptive, namespaced thread name. Keeps a simple global cap to avoid
/// runaway spawns when review flows create timers repeatedly.
pub(crate) fn spawn_lightweight<F>(name: &str, f: F) -> Option<std::thread::JoinHandle<()>>
where
    F: FnOnce() + Send + 'static,
{
    let mut observed = ACTIVE_THREADS.load(Ordering::SeqCst);
    loop {
        if observed >= MAX_BACKGROUND_THREADS {
            let now = now_epoch_secs();
            let last = LAST_LIMIT_LOG_SECS.load(Ordering::Relaxed);
            if now.saturating_sub(last) >= LIMIT_LOG_THROTTLE_SECS
                && LAST_LIMIT_LOG_SECS
                    .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
            {
                tracing::error!(
                    active_threads = observed,
                    max_threads = MAX_BACKGROUND_THREADS,
                    thread_name = name,
                    "background thread spawn rejected: limit reached"
                );
            }
            return None;
        }
        match ACTIVE_THREADS.compare_exchange(
            observed,
            observed + 1,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => break,
            Err(updated) => observed = updated,
        }
    }

    let thread_name = format!("code-{name}");
    let builder = std::thread::Builder::new()
        .name(thread_name)
        .stack_size(STACK_SIZE_BYTES);

    match builder.spawn(move || {
        let _guard = ThreadCountGuard::new();
        f();
    }) {
        Ok(handle) => Some(handle),
        Err(error) => {
            ACTIVE_THREADS.fetch_sub(1, Ordering::SeqCst);
            tracing::error!(thread_name = name, %error, "failed to spawn background thread");
            None
        }
    }
}
