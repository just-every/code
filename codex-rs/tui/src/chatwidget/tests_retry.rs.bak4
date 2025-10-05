use super::auto_coordinator::test_classify_model_error;
use super::retry::{retry_with_backoff, RetryDecision, RetryError, RetryOptions, RetryStatus};
use anyhow::{anyhow, Error};
use chrono::{Duration as ChronoDuration, Utc};
use codex_core::error::{CodexErr, UnexpectedResponseError, UsageLimitReachedError};
use rand::{rngs::StdRng, Rng, SeedableRng};
use reqwest::StatusCode;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::yield_now;
use tokio::time::advance;
use tokio_util::sync::CancellationToken;

fn expected_backoffs(options: &RetryOptions, count: usize) -> Vec<Duration> {
    let mut rng = StdRng::seed_from_u64(options.jitter_seed.expect("expected deterministic seed"));
    (1..=count)
        .map(|attempt| {
            let exponent = attempt.saturating_sub(1) as i32;
            let cap = (options.base_delay.as_secs_f64() * options.factor.powi(exponent))
                .min(options.max_delay.as_secs_f64());
            if cap <= f64::EPSILON {
                Duration::ZERO
            } else {
                Duration::from_secs_f64(rng.random_range(0.0..cap))
            }
        })
        .collect()
}

async fn wait_for_status_len(statuses: &Arc<Mutex<Vec<RetryStatus>>>, len: usize) {
    loop {
        if statuses.lock().unwrap().len() >= len {
            break;
        }
        yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn retry_retries_transient_disconnect_then_succeeds() {
    let cancel = CancellationToken::new();
    let options = RetryOptions {
        base_delay: Duration::from_secs(4),
        factor: 2.0,
        max_delay: Duration::from_secs(60),
        max_elapsed: Duration::from_secs(3600),
        jitter_seed: Some(42),
    };

    let attempts = Arc::new(Mutex::new(VecDeque::from([
        Err(anyhow!("model stream error: stream disconnected before completion")),
        Err(anyhow!("model stream error: stream disconnected before completion")),
        Ok::<(), Error>(()),
    ])));

    let statuses: Arc<Mutex<Vec<RetryStatus>>> = Arc::new(Mutex::new(Vec::new()));
    let run_attempts = attempts.clone();
    let status_log = statuses.clone();

    let task = tokio::spawn(retry_with_backoff(
        move || {
            let run_attempts = run_attempts.clone();
            async move {
                let mut guard = run_attempts.lock().unwrap();
                guard.pop_front().expect("attempt available")
            }
        },
        |err| {
            if err.to_string().contains("stream disconnected") {
                RetryDecision::RetryAfterBackoff {
                    reason: err.to_string(),
                }
            } else {
                RetryDecision::Fatal(anyhow!(err.to_string()))
            }
        },
        options.clone(),
        &cancel,
        move |status| {
            status_log.lock().unwrap().push(status);
        },
    ));

    wait_for_status_len(&statuses, 1).await;
    let sleep1 = statuses.lock().unwrap()[0]
        .sleep
        .expect("sleep recorded");
    advance(sleep1).await;
    yield_now().await;

    wait_for_status_len(&statuses, 2).await;
    let sleep2 = statuses.lock().unwrap()[1]
        .sleep
        .expect("sleep recorded");
    advance(sleep2).await;
    yield_now().await;

    let result = task.await.unwrap();
    assert!(result.is_ok(), "retry should eventually succeed");

    let recorded = statuses.lock().unwrap();
    assert_eq!(recorded.len(), 2, "expected two backoff sleeps");

    let expected = expected_backoffs(&options, 2);
    for (observed, expected_delay) in recorded.iter().zip(expected) {
        let sleep = observed.sleep.expect("sleep");
        let delta = (sleep.as_secs_f64() - expected_delay.as_secs_f64()).abs();
        assert!(delta < 1e-6, "sleep {:?} differed from expected {:?}", sleep, expected_delay);
    }
}

#[tokio::test(start_paused = true)]
async fn retry_respects_reset_seconds_rate_limit() {
    let cancel = CancellationToken::new();
    let options = RetryOptions {
        base_delay: Duration::from_secs(2),
        factor: 2.0,
        max_delay: Duration::from_secs(30),
        max_elapsed: Duration::from_secs(3600),
        jitter_seed: Some(7),
    };

    let body = json!({
        "error": {
            "resets_in_seconds": 60
        }
    })
    .to_string();
    let rate_limit_err = anyhow!(CodexErr::UnexpectedStatus(UnexpectedResponseError {
        status: StatusCode::TOO_MANY_REQUESTS,
        body,
        request_id: None,
    }));

    let attempts = Arc::new(Mutex::new(VecDeque::from([
        Err(rate_limit_err),
        Ok::<(), Error>(()),
    ])));

    let statuses: Arc<Mutex<Vec<RetryStatus>>> = Arc::new(Mutex::new(Vec::new()));
    let run_attempts = attempts.clone();
    let status_log = statuses.clone();

    let task = tokio::spawn(retry_with_backoff(
        move || {
            let run_attempts = run_attempts.clone();
            async move {
                let mut guard = run_attempts.lock().unwrap();
                guard.pop_front().expect("attempt available")
            }
        },
        |err| test_classify_model_error(err),
        options,
        &cancel,
        move |status| {
            status_log.lock().unwrap().push(status);
        },
    ));

    wait_for_status_len(&statuses, 1).await;
    let status = statuses.lock().unwrap()[0].clone();
    assert!(status.is_rate_limit, "expected rate limit status");
    assert!(status.reason.contains("rate") || status.reason.contains("usage"));
    let sleep = status.sleep.expect("sleep duration");

    // reset (60s) + buffer (120s) + jitter (<=30s)
    let sleep_secs = sleep.as_secs_f64();
    assert!(sleep_secs >= 180.0 - 0.5, "sleep too short: {sleep_secs}");
    assert!(sleep_secs <= 210.0 + 1.0, "sleep too long: {sleep_secs}");

    advance(sleep).await;
    yield_now().await;

    let result = task.await.unwrap();
    assert!(result.is_ok());
}

#[tokio::test(start_paused = true)]
async fn retry_respects_reset_at_rate_limit() {
    let cancel = CancellationToken::new();
    let options = RetryOptions {
        base_delay: Duration::from_secs(1),
        factor: 2.0,
        max_delay: Duration::from_secs(30),
        max_elapsed: Duration::from_secs(3600),
        jitter_seed: Some(9),
    };

    let reset_at = (Utc::now() + ChronoDuration::seconds(45)).to_rfc3339();
    let body = json!({
        "error": {
            "reset_at": reset_at
        }
    })
    .to_string();
    let rate_limit_err = anyhow!(CodexErr::UnexpectedStatus(UnexpectedResponseError {
        status: StatusCode::TOO_MANY_REQUESTS,
        body,
        request_id: None,
    }));

    let attempts = Arc::new(Mutex::new(VecDeque::from([
        Err(rate_limit_err),
        Ok::<(), Error>(()),
    ])));

    let statuses: Arc<Mutex<Vec<RetryStatus>>> = Arc::new(Mutex::new(Vec::new()));
    let run_attempts = attempts.clone();
    let status_log = statuses.clone();

    let task = tokio::spawn(retry_with_backoff(
        move || {
            let run_attempts = run_attempts.clone();
            async move {
                let mut guard = run_attempts.lock().unwrap();
                guard.pop_front().expect("attempt available")
            }
        },
        |err| test_classify_model_error(err),
        options,
        &cancel,
        move |status| {
            status_log.lock().unwrap().push(status);
        },
    ));

    wait_for_status_len(&statuses, 1).await;
    let status = statuses.lock().unwrap()[0].clone();
    assert!(status.is_rate_limit);
    let sleep = status.sleep.expect("sleep duration");
    let sleep_secs = sleep.as_secs_f64();
    assert!(sleep_secs >= 120.0 - 1.0, "sleep too short: {sleep_secs}");
    assert!(sleep_secs <= 150.0 + 1.5, "sleep too long: {sleep_secs}");

    advance(sleep).await;
    yield_now().await;

    let result = task.await.unwrap();
    assert!(result.is_ok());
}

#[tokio::test(start_paused = true)]
async fn retry_rate_limit_without_reset_falls_back_to_backoff() {
    let cancel = CancellationToken::new();
    let options = RetryOptions {
        base_delay: Duration::from_secs(3),
        factor: 2.0,
        max_delay: Duration::from_secs(30),
        max_elapsed: Duration::from_secs(3600),
        jitter_seed: Some(12),
    };

    let body = json!({ "error": { "message": "slow down" } }).to_string();
    let rate_limit_err = anyhow!(CodexErr::UnexpectedStatus(UnexpectedResponseError {
        status: StatusCode::TOO_MANY_REQUESTS,
        body,
        request_id: None,
    }));

    let attempts = Arc::new(Mutex::new(VecDeque::from([
        Err(rate_limit_err),
        Ok::<(), Error>(()),
    ])));

    let statuses: Arc<Mutex<Vec<RetryStatus>>> = Arc::new(Mutex::new(Vec::new()));
    let run_attempts = attempts.clone();
    let status_log = statuses.clone();

    let task = tokio::spawn(retry_with_backoff(
        move || {
            let run_attempts = run_attempts.clone();
            async move {
                let mut guard = run_attempts.lock().unwrap();
                guard.pop_front().expect("attempt available")
            }
        },
        |err| test_classify_model_error(err),
        options.clone(),
        &cancel,
        move |status| {
            status_log.lock().unwrap().push(status);
        },
    ));

    wait_for_status_len(&statuses, 1).await;
    let status = statuses.lock().unwrap()[0].clone();
    assert!(
        !status.is_rate_limit,
        "should fall back to exponential backoff when no reset hints are present"
    );
    let expected = expected_backoffs(&options, 1)[0];
    let sleep = status.sleep.expect("sleep duration");
    let delta = (sleep.as_secs_f64() - expected.as_secs_f64()).abs();
    assert!(delta < 1e-6);

    advance(sleep).await;
    yield_now().await;

    let result = task.await.unwrap();
    assert!(result.is_ok());
}

#[tokio::test(start_paused = true)]
async fn retry_cancellation_interrupts_sleep() {
    let cancel = CancellationToken::new();
    let options = RetryOptions {
        base_delay: Duration::from_secs(5),
        factor: 2.0,
        max_delay: Duration::from_secs(30),
        max_elapsed: Duration::from_secs(3600),
        jitter_seed: Some(21),
    };

    let rate_limit_err = anyhow!(CodexErr::UsageLimitReached(UsageLimitReachedError {
        plan_type: None,
        resets_in_seconds: Some(90),
    }));

    let attempts = Arc::new(Mutex::new(VecDeque::from([
        Err(rate_limit_err),
    ])));

    let statuses: Arc<Mutex<Vec<RetryStatus>>> = Arc::new(Mutex::new(Vec::new()));
    let run_attempts = attempts.clone();
    let status_log = statuses.clone();

    let cancel_clone = cancel.clone();
    let task = tokio::spawn(retry_with_backoff(
        move || {
            let run_attempts = run_attempts.clone();
            async move {
                let mut guard = run_attempts.lock().unwrap();
                guard.pop_front().unwrap()
            }
        },
        |err| test_classify_model_error(err),
        options,
        &cancel_clone,
        move |status| {
            status_log.lock().unwrap().push(status);
        },
    ));

    wait_for_status_len(&statuses, 1).await;
    cancel.cancel();

    let result = task.await.unwrap();
    assert!(matches!(result, Err(RetryError::Aborted)));
}
