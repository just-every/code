use std::time::{Duration, Instant};

use code_common::elapsed::format_duration;
use code_core::protocol::ReviewContextMetadata;
use code_core::protocol::ReviewOutputEvent;
use code_git_tooling::GhostCommit;

use crate::AutoTurnAgentsAction;
use crate::AutoTurnAgentsTiming;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoContinueMode {
    Immediate,
    TenSeconds,
    SixtySeconds,
    Manual,
}

impl AutoContinueMode {
    pub fn seconds(self) -> Option<u8> {
        match self {
            Self::Immediate => Some(0),
            Self::TenSeconds => Some(10),
            Self::SixtySeconds => Some(60),
            Self::Manual => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Immediate => "Immediate",
            Self::TenSeconds => "10 seconds",
            Self::SixtySeconds => "60 seconds",
            Self::Manual => "Manual approval",
        }
    }

    pub fn cycle_forward(self) -> Self {
        match self {
            Self::Immediate => Self::TenSeconds,
            Self::TenSeconds => Self::SixtySeconds,
            Self::SixtySeconds => Self::Manual,
            Self::Manual => Self::Immediate,
        }
    }

    pub fn cycle_backward(self) -> Self {
        match self {
            Self::Immediate => Self::Manual,
            Self::TenSeconds => Self::Immediate,
            Self::SixtySeconds => Self::TenSeconds,
            Self::Manual => Self::SixtySeconds,
        }
    }
}

impl Default for AutoContinueMode {
    fn default() -> Self {
        Self::TenSeconds
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoRunPhase {
    Idle,
    Launching,
    Active,
    PausedManual,
    AwaitingReview,
    TransientRecovery,
}

impl AutoRunPhase {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Launching | Self::Active | Self::PausedManual | Self::AwaitingReview | Self::TransientRecovery
        )
    }
}

impl Default for AutoRunPhase {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone)]
pub struct AutoRunSummary {
    pub duration: Duration,
    pub turns_completed: usize,
    pub message: Option<String>,
    pub goal: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AutoRestartState {
    pub token: u64,
    pub attempt: u32,
    pub reason: String,
}

#[derive(Default, Clone)]
pub struct AutoTurnReviewState {
    #[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]
    pub base_commit: Option<GhostCommit>,
}

#[derive(Clone)]
pub struct AutoResolveState {
    pub prompt: String,
    pub hint: String,
    pub metadata: Option<ReviewContextMetadata>,
    pub attempt: u32,
    pub max_attempts: u32,
    pub phase: AutoResolvePhase,
    pub last_review: Option<ReviewOutputEvent>,
    pub last_fix_message: Option<String>,
}

impl AutoResolveState {
    pub fn new(prompt: String, hint: String, metadata: Option<ReviewContextMetadata>) -> Self {
        Self {
            prompt,
            hint,
            metadata,
            attempt: 0,
            max_attempts: AUTO_RESOLVE_MAX_REVIEW_ATTEMPTS,
            phase: AutoResolvePhase::WaitingForReview,
            last_review: None,
            last_fix_message: None,
        }
    }
}

#[derive(Clone)]
pub enum AutoResolvePhase {
    WaitingForReview,
    PendingFix { review: ReviewOutputEvent },
    AwaitingFix { review: ReviewOutputEvent },
    AwaitingJudge { review: ReviewOutputEvent },
}

pub const AUTO_RESTART_MAX_ATTEMPTS: u32 = 6;
pub const AUTO_RESTART_BASE_DELAY: Duration = Duration::from_secs(5);
pub const AUTO_RESTART_MAX_DELAY: Duration = Duration::from_secs(120);
pub const AUTO_RESOLVE_MAX_REVIEW_ATTEMPTS: u32 = 3;
pub const AUTO_RESOLVE_REVIEW_FOLLOWUP: &str = "This issue has been resolved. Please continue your search and return all remaining issues you find.";

#[derive(Debug, Clone)]
pub enum AutoControllerEffect {
    RefreshUi,
    StartCountdown { countdown_id: u64, seconds: u8 },
    SubmitPrompt,
    LaunchStarted { goal: String },
    LaunchFailed { goal: String, error: String },
    StopCompleted { summary: AutoRunSummary, message: Option<String> },
    TransientPause { attempt: u32, delay: Duration, reason: String },
    ScheduleRestart { token: u64, attempt: u32, delay: Duration },
    CancelCoordinator,
    ResetHistory,
    UpdateTerminalHint { hint: Option<String> },
    SetTaskRunning { running: bool },
    EnsureInputFocus,
    ClearCoordinatorView,
    ShowGoalEntry,
}

#[derive(Default, Clone)]
pub struct AutoDriveController {
    pub active: bool,
    pub goal: Option<String>,
    pub current_summary: Option<String>,
    pub current_progress_past: Option<String>,
    pub current_progress_current: Option<String>,
    pub current_cli_prompt: Option<String>,
    pub current_cli_context: Option<String>,
    pub current_display_line: Option<String>,
    pub current_display_is_summary: bool,
    pub current_reasoning_title: Option<String>,
    pub placeholder_phrase: Option<String>,
    pub thinking_prefix_stripped: bool,
    pub current_summary_index: Option<u32>,
    pub awaiting_submission: bool,
    pub waiting_for_response: bool,
    pub paused_for_manual_edit: bool,
    pub resume_after_manual_submit: bool,
    pub waiting_for_review: bool,
    pub countdown_id: u64,
    pub seconds_remaining: u8,
    pub awaiting_goal_input: bool,
    pub last_broadcast_summary: Option<String>,
    pub last_decision_summary: Option<String>,
    pub last_decision_progress_past: Option<String>,
    pub last_decision_progress_current: Option<String>,
    pub last_decision_display: Option<String>,
    pub last_decision_display_is_summary: bool,
    pub coordinator_waiting: bool,
    pub review_enabled: bool,
    pub subagents_enabled: bool,
    pub cross_check_enabled: bool,
    pub qa_automation_enabled: bool,
    pub pending_agent_actions: Vec<AutoTurnAgentsAction>,
    pub pending_agent_timing: Option<AutoTurnAgentsTiming>,
    pub continue_mode: AutoContinueMode,
    pub started_at: Option<Instant>,
    pub turns_completed: usize,
    pub last_run_summary: Option<AutoRunSummary>,
    pub waiting_for_transient_recovery: bool,
    pub pending_restart: Option<AutoRestartState>,
    pub restart_token: u64,
    pub transient_restart_attempts: u32,
    pub intro_started_at: Option<Instant>,
    pub intro_reduced_motion: bool,
    pub intro_pending: bool,
    pub elapsed_override: Option<Duration>,
    pub phase: AutoRunPhase,
    pub bypass_coordinator_next_submit: bool,
}

impl AutoDriveController {
    pub fn prepare_launch(
        &mut self,
        goal: String,
        review_enabled: bool,
        subagents_enabled: bool,
        cross_check_enabled: bool,
        qa_automation_enabled: bool,
        continue_mode: AutoContinueMode,
        reduced_motion: bool,
    ) {
        let seed_intro = self.take_intro_pending();
        self.reset();
        if seed_intro {
            self.mark_intro_pending();
        }

        self.review_enabled = review_enabled;
        self.subagents_enabled = subagents_enabled;
        self.cross_check_enabled = cross_check_enabled;
        self.qa_automation_enabled = qa_automation_enabled;
        self.continue_mode = continue_mode;
        self.reset_countdown();
        self.ensure_intro_timing(reduced_motion);
        self.goal = Some(goal);
        self.awaiting_goal_input = false;
        self.phase = AutoRunPhase::Launching;
        self.bypass_coordinator_next_submit = false;
    }

    pub fn launch_succeeded(
        &mut self,
        goal: String,
        placeholder_phrase: Option<String>,
        now: Instant,
    ) -> Vec<AutoControllerEffect> {
        self.active = true;
        self.started_at = Some(now);
        self.turns_completed = 0;
        self.last_run_summary = None;
        self.goal = Some(goal.clone());
        self.current_summary = None;
        self.current_progress_past = None;
        self.current_progress_current = None;
        self.current_cli_prompt = None;
        self.current_cli_context = None;
        self.current_display_line = None;
        self.current_display_is_summary = false;
        self.current_reasoning_title = None;
        self.current_summary_index = None;
        self.placeholder_phrase = placeholder_phrase;
        self.thinking_prefix_stripped = false;
        self.last_broadcast_summary = None;
        self.last_decision_progress_past = None;
        self.last_decision_progress_current = None;
        self.waiting_for_response = true;
        self.coordinator_waiting = true;
        self.reset_countdown();
        self.phase = AutoRunPhase::Active;
        self.bypass_coordinator_next_submit = false;

        vec![
            AutoControllerEffect::LaunchStarted { goal },
            AutoControllerEffect::RefreshUi,
        ]
    }

    pub fn launch_failed(&mut self, goal: String, error: String) -> Vec<AutoControllerEffect> {
        self.active = false;
        self.goal = None;
        self.awaiting_goal_input = true;
        self.mark_intro_pending();
        self.reset_countdown();
        self.phase = AutoRunPhase::Idle;
        self.bypass_coordinator_next_submit = false;

        vec![
            AutoControllerEffect::LaunchFailed { goal, error },
            AutoControllerEffect::ShowGoalEntry,
            AutoControllerEffect::RefreshUi,
        ]
    }

    pub fn stop_run(
        &mut self,
        now: Instant,
        message: Option<String>,
    ) -> Vec<AutoControllerEffect> {
        let duration = self
            .started_at
            .map(|start| now.saturating_duration_since(start))
            .unwrap_or_default();
        let summary = AutoRunSummary {
            duration,
            turns_completed: self.turns_completed,
            message: message.clone(),
            goal: self.goal.clone(),
        };

        self.reset();
        self.last_run_summary = Some(summary.clone());
        self.awaiting_goal_input = true;
        self.phase = AutoRunPhase::Idle;
        self.bypass_coordinator_next_submit = false;

        vec![
            AutoControllerEffect::CancelCoordinator,
            AutoControllerEffect::ResetHistory,
            AutoControllerEffect::ClearCoordinatorView,
            AutoControllerEffect::UpdateTerminalHint { hint: None },
            AutoControllerEffect::SetTaskRunning { running: false },
            AutoControllerEffect::EnsureInputFocus,
            AutoControllerEffect::StopCompleted { summary, message },
            AutoControllerEffect::RefreshUi,
        ]
    }

    pub fn pause_for_transient_failure(
        &mut self,
        now: Instant,
        reason: String,
    ) -> Vec<AutoControllerEffect> {
        let pending_attempt = self.transient_restart_attempts.saturating_add(1);
        let truncated_reason = Self::truncate_error(&reason);
        self.waiting_for_transient_recovery = true;
        self.waiting_for_response = false;
        self.coordinator_waiting = false;
        self.awaiting_submission = false;
        self.paused_for_manual_edit = false;
        self.resume_after_manual_submit = false;
        self.waiting_for_review = false;
        self.current_cli_prompt = None;
        self.current_cli_context = None;
        self.pending_agent_actions.clear();
        self.pending_agent_timing = None;
        self.phase = AutoRunPhase::TransientRecovery;
        self.bypass_coordinator_next_submit = false;

        if pending_attempt > AUTO_RESTART_MAX_ATTEMPTS {
            self.transient_restart_attempts = pending_attempt;
            let summary = AutoRunSummary {
                duration: self
                    .started_at
                    .map(|start| now.saturating_duration_since(start))
                    .unwrap_or_default(),
                turns_completed: self.turns_completed,
                message: Some(format!(
                    "Auto Drive stopped after {AUTO_RESTART_MAX_ATTEMPTS} reconnect attempts."
                )),
                goal: self.goal.clone(),
            };
            self.reset();
            self.last_run_summary = Some(summary.clone());
            self.awaiting_goal_input = true;

            return vec![
                AutoControllerEffect::CancelCoordinator,
                AutoControllerEffect::ResetHistory,
                AutoControllerEffect::ClearCoordinatorView,
                AutoControllerEffect::UpdateTerminalHint { hint: None },
                AutoControllerEffect::SetTaskRunning { running: false },
                AutoControllerEffect::EnsureInputFocus,
                AutoControllerEffect::StopCompleted {
                    summary,
                    message: Some(format!(
                        "Auto Drive stopped after {AUTO_RESTART_MAX_ATTEMPTS} reconnect attempts. Last error: {truncated_reason}"
                    )),
                },
                AutoControllerEffect::RefreshUi,
            ];
        }

        self.transient_restart_attempts = pending_attempt;
        let delay = Self::auto_restart_delay(pending_attempt);
        let token = self.restart_token.wrapping_add(1);
        self.restart_token = token;
        self.pending_restart = Some(AutoRestartState {
            token,
            attempt: pending_attempt,
            reason: truncated_reason.clone(),
        });

        let human_delay = format_duration(delay);
        self.current_display_line = Some(format!(
            "Waiting for connection… retrying in {human_delay} (attempt {pending_attempt}/{AUTO_RESTART_MAX_ATTEMPTS})"
        ));
        self.current_display_is_summary = true;
        self.current_progress_current = Some(format!("Last error: {truncated_reason}"));
        self.current_progress_past = None;
        self.placeholder_phrase = Some("Waiting for connection…".to_string());
        self.thinking_prefix_stripped = false;

        vec![
            AutoControllerEffect::CancelCoordinator,
            AutoControllerEffect::SetTaskRunning { running: false },
            AutoControllerEffect::UpdateTerminalHint {
                hint: Some("Press Esc again to exit Auto Drive".to_string()),
            },
            AutoControllerEffect::TransientPause {
                attempt: pending_attempt,
                delay,
                reason: truncated_reason,
            },
            AutoControllerEffect::ScheduleRestart {
                token,
                attempt: pending_attempt,
                delay,
            },
            AutoControllerEffect::RefreshUi,
        ]
    }

    pub fn schedule_cli_prompt(&mut self, prompt_text: String) -> Vec<AutoControllerEffect> {
        self.current_cli_prompt = Some(prompt_text);
        self.awaiting_submission = true;
        self.reset_countdown();
        self.countdown_id = self.countdown_id.wrapping_add(1);
        let countdown_id = self.countdown_id;
        let countdown = self.countdown_seconds();
        self.seconds_remaining = countdown.unwrap_or(0);

        let mut effects = vec![AutoControllerEffect::RefreshUi];
        if let Some(seconds) = countdown {
            effects.push(AutoControllerEffect::StartCountdown {
                countdown_id,
                seconds,
            });
        }
        effects
    }

    pub fn update_continue_mode(&mut self, mode: AutoContinueMode) -> Vec<AutoControllerEffect> {
        self.continue_mode = mode;
        self.reset_countdown();
        self.seconds_remaining = self.countdown_seconds().unwrap_or(0);

        let mut effects = vec![AutoControllerEffect::RefreshUi];
        if self.awaiting_submission && !self.paused_for_manual_edit {
            self.countdown_id = self.countdown_id.wrapping_add(1);
            let countdown_id = self.countdown_id;
            let countdown = self.countdown_seconds();
            if let Some(seconds) = countdown {
                effects.push(AutoControllerEffect::StartCountdown {
                    countdown_id,
                    seconds,
                });
            }
        }
        effects
    }

    pub fn handle_countdown_tick(
        &mut self,
        countdown_id: u64,
        seconds_left: u8,
    ) -> Vec<AutoControllerEffect> {
        if !self.active
            || countdown_id != self.countdown_id
            || !self.awaiting_submission
            || self.paused_for_manual_edit
        {
            return Vec::new();
        }

        self.seconds_remaining = seconds_left;
        if seconds_left == 0 {
            vec![AutoControllerEffect::SubmitPrompt]
        } else {
            vec![AutoControllerEffect::RefreshUi]
        }
    }

    pub fn reset(&mut self) {
        let review_enabled = self.review_enabled;
        let subagents_enabled = self.subagents_enabled;
        let cross_check_enabled = self.cross_check_enabled;
        let qa_automation_enabled = self.qa_automation_enabled;
        let continue_mode = self.continue_mode;
        let intro_pending = self.intro_pending;
        let intro_started_at = self.intro_started_at;
        let intro_reduced_motion = self.intro_reduced_motion;
        let elapsed_override = self.elapsed_override;

        *self = Self::default();

        self.review_enabled = review_enabled;
        self.subagents_enabled = subagents_enabled;
        self.cross_check_enabled = cross_check_enabled;
        self.qa_automation_enabled = qa_automation_enabled;
        self.continue_mode = continue_mode;
        self.seconds_remaining = self.continue_mode.seconds().unwrap_or(0);
        self.intro_pending = intro_pending;
        self.intro_started_at = intro_started_at;
        self.intro_reduced_motion = intro_reduced_motion;
        self.elapsed_override = elapsed_override;
        self.phase = if self.active {
            AutoRunPhase::Active
        } else {
            AutoRunPhase::Idle
        };
        if self.phase != AutoRunPhase::PausedManual {
            self.bypass_coordinator_next_submit = false;
        }
    }

    pub fn reset_intro_timing(&mut self) {
        self.intro_started_at = None;
        self.intro_reduced_motion = false;
    }

    pub fn ensure_intro_timing(&mut self, reduced_motion: bool) {
        if self.intro_started_at.is_none() {
            self.intro_started_at = Some(Instant::now());
        }
        self.intro_reduced_motion = reduced_motion;
    }

    pub fn mark_intro_pending(&mut self) {
        self.intro_pending = true;
    }

    pub fn take_intro_pending(&mut self) -> bool {
        if self.intro_pending {
            self.intro_pending = false;
            true
        } else {
            false
        }
    }

    pub fn countdown_active(&self) -> bool {
        self.awaiting_submission
            && !self.paused_for_manual_edit
            && self
                .countdown_seconds()
                .map(|seconds| seconds > 0)
                .unwrap_or(false)
    }

    pub fn countdown_seconds(&self) -> Option<u8> {
        self.continue_mode.seconds()
    }

    pub fn reset_countdown(&mut self) {
        self.seconds_remaining = self.countdown_seconds().unwrap_or(0);
    }

    pub fn set_phase(&mut self, phase: AutoRunPhase) {
        self.phase = phase;
        if phase != AutoRunPhase::PausedManual {
            self.bypass_coordinator_next_submit = false;
        }
    }

    pub fn is_auto_active(&self) -> bool {
        self.phase.is_active()
    }

    pub fn current_phase(&self) -> AutoRunPhase {
        self.phase
    }

    pub fn set_bypass_coordinator_next_submit(&mut self) {
        self.bypass_coordinator_next_submit = true;
    }

    pub fn should_bypass_coordinator_next_submit(&self) -> bool {
        self.bypass_coordinator_next_submit
    }

    pub fn clear_bypass_coordinator_flag(&mut self) {
        self.bypass_coordinator_next_submit = false;
    }

    fn auto_restart_delay(attempt: u32) -> Duration {
        if attempt == 0 {
            return AUTO_RESTART_BASE_DELAY.min(AUTO_RESTART_MAX_DELAY);
        }
        let exponent = attempt.saturating_sub(1).min(5);
        let multiplier = 1u32 << exponent;
        let mut delay = AUTO_RESTART_BASE_DELAY.saturating_mul(multiplier);
        if delay > AUTO_RESTART_MAX_DELAY {
            delay = AUTO_RESTART_MAX_DELAY;
        }
        delay
    }

    fn truncate_error(reason: &str) -> String {
        const MAX_LEN: usize = 160;
        let text = reason.trim();
        if text.len() <= MAX_LEN {
            return text.to_string();
        }
        let mut truncated = text.chars().take(MAX_LEN).collect::<String>();
        truncated.push('…');
        truncated
    }
}
