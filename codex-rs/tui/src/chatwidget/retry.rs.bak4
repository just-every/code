use std::time::{Duration, Instant};

use anyhow::Error;
use rand::{rngs::StdRng, Rng, SeedableRng};
use tokio::time;
use tokio_util::sync::CancellationToken;
use tracing::warn;

#[derive(Debug, Clone)]
pub(crate) struct RetryOptions {
    pub base_delay: Duration,
    pub factor: f64,
    pub max_delay: Duration,
    pub max_elapsed: Duration,
    pub jitter_seed: Option<u64>,
}

impl RetryOptions {
    pub fn with_defaults(max_elapsed: Duration) -> Self {
        Self {
            base_delay: Duration::from_secs(1),
            factor: 2.0,
            max_delay: Duration::from_secs(15 * 60),
            max_elapsed,
            jitter_seed: None,
        }
    }
}

#[derive(Debug)]
pub(crate) enum RetryDecision {
    RetryAfterBackoff { reason: String },
    RateLimited { wait_until: Instant, reason: String },
    Fatal(Error),
}

#[derive(Debug, Clone)]
pub(crate) struct RetryStatus {
    pub attempt: u32,
    pub elapsed: Duration,
    pub sleep: Option<Duration>,
    pub resume_at: Option<Instant>,
    pub reason: String,
    pub is_rate_limit: bool,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum RetryError {
    #[error("retry aborted")]
    Aborted,
    #[error("retry timed out after {elapsed:?}")]
    Timeout { elapsed: Duration, last_error: Error },
    #[error(transparent)]
    Fatal(Error),
}

pub(crate) async fn retry_with_backoff<F, Fut, T, Classify, StatusCb>(
    mut run: F,
    mut classify: Classify,
    options: RetryOptions,
    cancel: &CancellationToken,
    mut status_cb: StatusCb,
) -> Result<T, RetryError>
where
    F: FnMut() -> Fut + Send,
    Fut: std::future::Future<Output = Result<T, Error>> + Send,
    T: Send,
    Classify: FnMut(&Error) -> RetryDecision + Send,
    StatusCb: FnMut(RetryStatus) + Send,
{
    let start_time = Instant::now();
    let mut attempt: u32 = 0;
    let mut rng = if let Some(seed) = options.jitter_seed {
        StdRng::seed_from_u64(seed)
    } else {
        let mut thread = rand::rng();
        StdRng::from_rng(&mut thread)
    };

    loop {
        if cancel.is_cancelled() {
            return Err(RetryError::Aborted);
        }

        attempt = attempt.saturating_add(1);
        let output = run().await;
        match output {
            Ok(value) => return Ok(value),
            Err(error) => {
                let elapsed = start_time.elapsed();
                if elapsed >= options.max_elapsed {
                    return Err(RetryError::Timeout {
                        elapsed,
                        last_error: error,
                    });
                }

                match classify(&error) {
                    RetryDecision::Fatal(fatal) => return Err(RetryError::Fatal(fatal)),
                    RetryDecision::RateLimited { wait_until, reason } => {
                        let now = Instant::now();
                        if wait_until <= now {
                            warn!(attempt, elapsed = ?elapsed, "{reason}; retrying immediately");
                            continue;
                        }
                        let sleep = wait_until.duration_since(now);
                        warn!(attempt, elapsed = ?elapsed, wait = ?sleep, resume_at = ?wait_until, "{reason}");
                        status_cb(RetryStatus {
                            attempt,
                            elapsed,
                            sleep: Some(sleep),
                            resume_at: Some(wait_until),
                            reason,
                            is_rate_limit: true,
                        });
                        wait_with_cancel(cancel, sleep).await?;
                    }
                    RetryDecision::RetryAfterBackoff { reason } => {
                        let sleep = compute_delay(&options, attempt, &mut rng);
                        let resume_at = Instant::now() + sleep;
                        warn!(attempt, elapsed = ?elapsed, wait = ?sleep, resume_at = ?resume_at, "{reason}");
                        status_cb(RetryStatus {
                            attempt,
                            elapsed,
                            sleep: Some(sleep),
                            resume_at: Some(resume_at),
                            reason,
                            is_rate_limit: false,
                        });
                        wait_with_cancel(cancel, sleep).await?;
                    }
                }
            }
        }
    }
}

fn compute_delay(options: &RetryOptions, attempt: u32, rng: &mut StdRng) -> Duration {
    let exponent = attempt.saturating_sub(1) as i32;
    let factor = options.factor.powi(exponent);
    let base = options.base_delay.as_secs_f64() * factor;
    let capped = base.min(options.max_delay.as_secs_f64());
    if capped <= f64::EPSILON {
        return Duration::ZERO;
    }

    let jitter = rng.random_range(0.0..capped);
    Duration::from_secs_f64(jitter)
}

async fn wait_with_cancel(cancel: &CancellationToken, duration: Duration) -> Result<(), RetryError> {
    if duration.is_zero() {
        return Ok(());
    }

    tokio::select! {
        _ = time::sleep(duration) => Ok(()),
        _ = cancel.cancelled() => Err(RetryError::Aborted),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use rand::{rngs::StdRng, SeedableRng};

    #[tokio::test]
    async fn aborts_immediately_when_cancelled() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let options = RetryOptions::with_defaults(Duration::from_secs(10));
        let result = retry_with_backoff(
            || async { Err(anyhow!("boom")) },
            |_| RetryDecision::RetryAfterBackoff {
                reason: "retry".to_string(),
            },
            options,
            &cancel,
            |_status| {},
        )
        .await;
        assert!(matches!(result, Err(RetryError::Aborted)));
    }

    #[tokio::test]
    async fn backoff_uses_seeded_full_jitter() {
        let cancel = CancellationToken::new();
        let mut sleeps = Vec::new();
        let options = RetryOptions {
            base_delay: Duration::from_millis(100),
            factor: 2.0,
            max_delay: Duration::from_millis(800),
            max_elapsed: Duration::from_secs(1),
            jitter_seed: Some(7),
        };
        let expected_opts = options.clone();

        let result = retry_with_backoff(
            || async { Err(anyhow!("boom")) },
            |_| RetryDecision::RetryAfterBackoff {
                reason: "transient".to_string(),
            },
            options,
            &cancel,
            |status| {
                if let Some(delay) = status.sleep {
                    sleeps.push(delay);
                }
            },
        )
        .await;

        assert!(matches!(result, Err(RetryError::Timeout { .. })));
        assert!(!sleeps.is_empty());

        let mut rng = StdRng::seed_from_u64(7);
        for (idx, observed) in sleeps.iter().enumerate().take(4) {
            let exponent = idx as i32;
            let factor = expected_opts.factor.powi(exponent);
            let cap = (expected_opts.base_delay.as_secs_f64() * factor)
                .min(expected_opts.max_delay.as_secs_f64());
            let expected = Duration::from_secs_f64(rng.random_range(0.0..cap));
            let diff = (observed.as_secs_f64() - expected.as_secs_f64()).abs();
            assert!(diff < 1e-6, "attempt {idx} diff {diff}");
        }
    }

    #[tokio::test]
    async fn stops_after_max_elapsed_budget() {
        let cancel = CancellationToken::new();
        let options = RetryOptions {
            base_delay: Duration::from_millis(10),
            factor: 2.0,
            max_delay: Duration::from_millis(50),
            max_elapsed: Duration::from_millis(60),
            jitter_seed: Some(5),
        };
        let result = retry_with_backoff(
            || async { Err(anyhow!("boom")) },
            |_| RetryDecision::RetryAfterBackoff {
                reason: "transient".to_string(),
            },
            options,
            &cancel,
            |_status| {},
        )
        .await;

        assert!(matches!(result, Err(RetryError::Timeout { .. })));
    }
}
