use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// Background tasks occasionally spin up Tokio runtimes or TLS stacks; keep a
// modest stack while avoiding stack overflow in heavier workers.
const STACK_SIZE_BYTES: usize = 1024 * 1024;
const MAX_BACKGROUND_THREADS: usize = 32;
const LIMIT_LOG_THROTTLE: Duration = Duration::from_secs(5);

static ACTIVE_THREADS: AtomicUsize = AtomicUsize::new(0);
static LIMIT_LOG_STATE: OnceLock<Mutex<HashMap<String, (Instant, u64)>>> = OnceLock::new();

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

fn throttle_limit_log(name: &str) -> Option<u64> {
    let now = Instant::now();
    let state = LIMIT_LOG_STATE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut state = state.lock().unwrap();
    let entry = state
        .entry(name.to_string())
        .or_insert((now - LIMIT_LOG_THROTTLE, 0));
    if now.duration_since(entry.0) >= LIMIT_LOG_THROTTLE {
        let suppressed = entry.1;
        entry.0 = now;
        entry.1 = 0;
        Some(suppressed)
    } else {
        entry.1 = entry.1.saturating_add(1);
        None
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
            if let Some(suppressed) = throttle_limit_log(name) {
                tracing::error!(
                    active_threads = observed,
                    max_threads = MAX_BACKGROUND_THREADS,
                    thread_name = name,
                    suppressed,
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
