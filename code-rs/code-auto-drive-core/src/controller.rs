use std::time::{Duration, Instant};

use code_common::elapsed::format_duration;
use code_core::protocol::ReviewContextMetadata;
use code_core::protocol::ReviewOutputEvent;
use code_core::review_coord::{bump_snapshot_epoch, try_acquire_lock};
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
/// High-level Auto Drive run phase. Each variant implies a precise legacy
/// boolean configuration so older call sites can continue to rely on mirrored
/// flags during the migration.
pub enum AutoRunPhase {
    /// No run in flight; all transient state must be cleared.
    Idle,
    /// Goal entry panel is visible; awaiting user input to launch.
    AwaitingGoalEntry,
    /// Launch sequence is preparing the first turn; coordinator not armed yet.
    Launching,
    /// Run is executing normally with no outstanding coordinator or review gates.
    Active,
    /// User is editing the prompt; resume behaviour and next-submit bypass state carried in payload.
    PausedManual {
        resume_after_submit: bool,
        bypass_next_submit: bool,
    },
    /// Coordinator has a prompt ready for approval.
    AwaitingCoordinator { prompt_ready: bool },
    /// Model response is streaming or pending diagnostics.
    AwaitingDiagnostics { coordinator_waiting: bool },
    /// Awaiting user review/approval; may include diagnostics to show.
    AwaitingReview { diagnostics_pending: bool },
    /// Backoff between restart attempts after a transient failure.
    TransientRecovery { backoff_ms: u64 },
}

impl AutoRunPhase {
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Launching
                | Self::Active
                | Self::PausedManual { .. }
                | Self::AwaitingCoordinator { .. }
                | Self::AwaitingDiagnostics { .. }
                | Self::AwaitingReview { .. }
                | Self::TransientRecovery { .. }
        )
    }

    pub fn is_running(&self) -> bool {
        matches!(
            self,
            Self::Launching
                | Self::Active
                | Self::PausedManual { .. }
                | Self::AwaitingCoordinator { .. }
                | Self::AwaitingDiagnostics { .. }
                | Self::AwaitingReview { .. }
                | Self::TransientRecovery { .. }
        )
    }

    pub fn awaiting_coordinator_submit(&self) -> bool {
        matches!(self, Self::AwaitingCoordinator { prompt_ready: true })
    }

    pub fn awaiting_review(&self) -> bool {
        matches!(self, Self::AwaitingReview { .. })
    }

    pub fn in_transient_recovery(&self) -> bool {
        matches!(self, Self::TransientRecovery { .. })
    }

    pub fn is_paused_manual(&self) -> bool {
        matches!(self, Self::PausedManual { .. })
    }

    pub fn should_show_goal_entry(&self) -> bool {
        matches!(self, Self::AwaitingGoalEntry)
    }

    pub fn prompt_ready(&self) -> bool {
        matches!(self, Self::AwaitingCoordinator { prompt_ready: true })
    }

    pub fn diagnostics_pending(&self) -> bool {
        matches!(self, Self::AwaitingReview { diagnostics_pending: true })
    }

    pub fn is_waiting_for_response(&self) -> bool {
        matches!(self, Self::AwaitingDiagnostics { .. })
    }

    pub fn resume_after_submit(&self) -> Option<bool> {
        match self {
            Self::PausedManual {
                resume_after_submit,
                ..
            } => Some(*resume_after_submit),
            _ => None,
        }
    }
}

impl Default for AutoRunPhase {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhaseTransition {
    BeginLaunch,
    LaunchSuccess,
    LaunchFailed,
    PauseForManualEdit { resume_after_submit: bool },
    ResumeFromManual,
    PromptReady,
    SubmitPrompt,
    AwaitDiagnostics,
    BeginReview { diagnostics_pending: bool },
    CompleteReview,
    TransientFailure { backoff_ms: u64 },
    RecoveryAttempt,
    Stop,
}

#[derive(Clone, Debug)]
pub struct TransitionEffects {
    pub effects: Vec<AutoControllerEffect>,
    pub phase_changed: bool,
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
    pub last_reviewed_commit: Option<String>,
    pub snapshot_epoch: Option<u64>,
}

impl AutoResolveState {
    pub fn new(prompt: String, hint: String, metadata: Option<ReviewContextMetadata>) -> Self {
        Self::new_with_limit(prompt, hint, metadata, AUTO_RESOLVE_MAX_REVIEW_ATTEMPTS)
    }

    pub fn new_with_limit(
        prompt: String,
        hint: String,
        metadata: Option<ReviewContextMetadata>,
        max_attempts: u32,
    ) -> Self {
        Self {
            prompt,
            hint,
            metadata,
            attempt: 0,
            max_attempts,
            phase: AutoResolvePhase::WaitingForReview,
            last_review: None,
            last_fix_message: None,
            last_reviewed_commit: None,
            snapshot_epoch: None,
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

pub const AUTO_RESTART_BASE_DELAY: Duration = Duration::from_secs(5);
pub const AUTO_RESTART_MAX_DELAY: Duration = Duration::from_secs(120);
pub const AUTO_RESOLVE_MAX_REVIEW_ATTEMPTS: u32 = 3;
pub const AUTO_RESOLVE_REVIEW_FOLLOWUP: &str = "This issue has been resolved. Please continue your search and return all remaining issues you find.";

#[derive(Debug, Clone)]
pub enum AutoControllerEffect {
    RefreshUi,
    StartCountdown { countdown_id: u64, decision_seq: u64, seconds: u8 },
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
    pub goal: Option<String>,
    pub current_summary: Option<String>,
    pub current_status_sent_to_user: Option<String>,
    pub current_status_title: Option<String>,
    pub current_cli_prompt: Option<String>,
    pub current_cli_context: Option<String>,
    pub hide_cli_context_in_ui: bool,
    pub suppress_next_cli_display: bool,
    pub current_display_line: Option<String>,
    pub current_display_is_summary: bool,
    pub current_reasoning_title: Option<String>,
    pub placeholder_phrase: Option<String>,
    pub thinking_prefix_stripped: bool,
    pub current_summary_index: Option<u32>,
    pub countdown_id: u64,
    pub countdown_decision_seq: u64,
    pub seconds_remaining: u8,
    pub countdown_override: Option<u8>,
    pub last_broadcast_summary: Option<String>,
    pub last_decision_summary: Option<String>,
    pub last_decision_status_sent_to_user: Option<String>,
    pub last_decision_status_title: Option<String>,
    pub last_decision_display: Option<String>,
    pub last_decision_display_is_summary: bool,
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
    pub pending_restart: Option<AutoRestartState>,
    pub restart_token: u64,
    pub transient_restart_attempts: u32,
    pub intro_started_at: Option<Instant>,
    pub intro_reduced_motion: bool,
    pub intro_pending: bool,
    pub elapsed_override: Option<Duration>,
    pub pending_stop_message: Option<String>,
    pub last_completion_explanation: Option<String>,
    pub phase: AutoRunPhase,
    // Non-cloneable guard is kept separately; controller stays Clone.
    pub review_lock: Option<std::sync::Arc<code_core::review_coord::ReviewGuard>>, 
}

impl AutoDriveController {
    fn apply_phase(&mut self, phase: AutoRunPhase) {
        self.phase = phase;
    }

    pub fn set_waiting_for_response(&mut self, waiting: bool) {
        if waiting {
            match &mut self.phase {
                AutoRunPhase::AwaitingDiagnostics { coordinator_waiting } => {
                    *coordinator_waiting = true;
                }
                _ => {
                    self.apply_phase(AutoRunPhase::AwaitingDiagnostics { coordinator_waiting: true });
                }
            }
        } else if self.phase.is_waiting_for_response() {
            self.apply_phase(AutoRunPhase::Active);
        }
    }

    pub fn set_coordinator_waiting(&mut self, waiting: bool) {
        match &mut self.phase {
            AutoRunPhase::AwaitingDiagnostics { coordinator_waiting } => {
                *coordinator_waiting = waiting;
            }
            _ if waiting => {
                self.apply_phase(AutoRunPhase::AwaitingDiagnostics { coordinator_waiting: true });
            }
            _ => {}
        }
    }

    pub fn on_prompt_ready(&mut self, prompt_ready: bool) {
        self.apply_phase(AutoRunPhase::AwaitingCoordinator { prompt_ready });
    }

    pub fn on_prompt_submitted(&mut self) {
        self.countdown_override = None;
        self.apply_phase(AutoRunPhase::AwaitingDiagnostics { coordinator_waiting: true });
    }

    pub fn on_pause_for_manual(&mut self, resume_after_submit: bool) {
        self.apply_phase(AutoRunPhase::PausedManual {
            resume_after_submit,
            bypass_next_submit: false,
        });
    }

    pub fn on_resume_from_manual(&mut self) {
        self.apply_phase(AutoRunPhase::Active);
        bump_snapshot_epoch();
    }

    pub fn on_begin_review(&mut self, diagnostics_pending: bool) {
        self.apply_phase(AutoRunPhase::AwaitingReview { diagnostics_pending });
        // Acquire global review lock; if busy or error, fall back to Active to avoid overlap.
        self.review_lock = try_acquire_lock("auto-drive-review", std::path::Path::new("."))
            .ok()
            .flatten()
            .map(std::sync::Arc::new);
        if self.review_lock.is_none() {
            self.apply_phase(AutoRunPhase::Active);
        }
    }

    pub fn on_complete_review(&mut self) {
        self.apply_phase(AutoRunPhase::Active);
        self.review_lock = None;
    }

    pub fn on_transient_failure(&mut self, backoff_ms: u64) {
        self.apply_phase(AutoRunPhase::TransientRecovery { backoff_ms });
    }

    pub fn on_recovery_attempt(&mut self) {
        if self.phase.in_transient_recovery() {
            self.apply_phase(AutoRunPhase::Active);
        }
    }

    pub fn transition(&mut self, transition: PhaseTransition) -> TransitionEffects {
        let old_phase = self.phase.clone();
        let effects = Vec::new();

        match (&self.phase, &transition) {
            (AutoRunPhase::Idle | AutoRunPhase::AwaitingGoalEntry, PhaseTransition::BeginLaunch) => {
                self.apply_phase(AutoRunPhase::Launching);
            }
            (AutoRunPhase::Launching, PhaseTransition::LaunchSuccess) => {
                self.apply_phase(AutoRunPhase::Active);
            }
            (AutoRunPhase::Launching, PhaseTransition::LaunchFailed) => {
                self.apply_phase(AutoRunPhase::AwaitingGoalEntry);
            }
            (AutoRunPhase::Active, PhaseTransition::PauseForManualEdit { resume_after_submit }) => {
                self.apply_phase(AutoRunPhase::PausedManual {
                    resume_after_submit: *resume_after_submit,
                    bypass_next_submit: false,
                });
            }
            (AutoRunPhase::PausedManual { .. }, PhaseTransition::ResumeFromManual) => {
                self.apply_phase(AutoRunPhase::Active);
            }
            (AutoRunPhase::Active, PhaseTransition::PromptReady) => {
                self.apply_phase(AutoRunPhase::AwaitingCoordinator { prompt_ready: true });
            }
            (AutoRunPhase::AwaitingCoordinator { .. }, PhaseTransition::SubmitPrompt) => {
                self.apply_phase(AutoRunPhase::Active);
            }
            (AutoRunPhase::Active, PhaseTransition::AwaitDiagnostics) => {
                self.apply_phase(AutoRunPhase::AwaitingDiagnostics { coordinator_waiting: true });
            }
            (AutoRunPhase::AwaitingDiagnostics { .. } | AutoRunPhase::Active, PhaseTransition::BeginReview { diagnostics_pending }) => {
                if self.review_lock.is_none() {
                    self.review_lock = code_core::review_coord::try_acquire_lock(
                        "auto-drive-review",
                        std::path::Path::new("."),
                    )
                    .ok()
                    .flatten()
                    .map(std::sync::Arc::new);
                    if self.review_lock.is_none() {
                        // Unable to secure the global lock; stay active to avoid overlapping reviews.
                        self.apply_phase(AutoRunPhase::Active);
                        return TransitionEffects { effects, phase_changed: old_phase != self.phase };
                    }
                }

                self.apply_phase(AutoRunPhase::AwaitingReview { diagnostics_pending: *diagnostics_pending });
            }
            (AutoRunPhase::AwaitingReview { .. }, PhaseTransition::CompleteReview) => {
                self.apply_phase(AutoRunPhase::Active);
                self.review_lock = None;
            }
            (_, PhaseTransition::TransientFailure { backoff_ms }) => {
                if self.phase.is_active() {
                    self.apply_phase(AutoRunPhase::TransientRecovery { backoff_ms: *backoff_ms });
                }
            }
            (AutoRunPhase::TransientRecovery { .. }, PhaseTransition::RecoveryAttempt) => {
                self.apply_phase(AutoRunPhase::Active);
            }
            (_, PhaseTransition::Stop) => {
                self.apply_phase(AutoRunPhase::Idle);
                self.review_lock = None;
            }
            _ => {
            }
        }

        let phase_changed = old_phase != self.phase;
        TransitionEffects { effects, phase_changed }
    }

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
        self.transition(PhaseTransition::BeginLaunch);
    }

    pub fn launch_succeeded(
        &mut self,
        goal: String,
        placeholder_phrase: Option<String>,
        now: Instant,
    ) -> Vec<AutoControllerEffect> {
        self.started_at = Some(now);
        self.turns_completed = 0;
        self.last_run_summary = None;
        self.last_completion_explanation = None;
        self.goal = Some(goal.clone());
        self.current_summary = None;
        self.current_status_sent_to_user = None;
        self.current_status_title = None;
        self.current_cli_prompt = None;
        self.current_cli_context = None;
        self.hide_cli_context_in_ui = false;
        self.suppress_next_cli_display = false;
        self.current_display_line = None;
        self.current_display_is_summary = false;
        self.current_reasoning_title = None;
        self.current_summary_index = None;
        self.placeholder_phrase = placeholder_phrase;
        self.thinking_prefix_stripped = false;
        self.last_broadcast_summary = None;
        self.countdown_override = None;
        self.last_decision_status_sent_to_user = None;
        self.last_decision_status_title = None;
        self.reset_countdown();
        self.apply_phase(AutoRunPhase::AwaitingDiagnostics { coordinator_waiting: true });

        vec![
            AutoControllerEffect::LaunchStarted { goal },
            AutoControllerEffect::RefreshUi,
        ]
    }

    pub fn launch_failed(&mut self, goal: String, error: String) -> Vec<AutoControllerEffect> {
        self.goal = None;
        self.mark_intro_pending();
        self.countdown_override = None;
        self.reset_countdown();
        self.apply_phase(AutoRunPhase::AwaitingGoalEntry);

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
        self.transition(PhaseTransition::Stop);
        self.apply_phase(AutoRunPhase::AwaitingGoalEntry);

        self.pending_stop_message = None;
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
        _now: Instant,
        reason: String,
    ) -> Vec<AutoControllerEffect> {
        let pending_attempt = self.transient_restart_attempts.saturating_add(1);
        let truncated_reason = Self::truncate_error(&reason);
        self.current_cli_prompt = None;
        self.current_cli_context = None;
        self.hide_cli_context_in_ui = false;
        self.suppress_next_cli_display = false;
        self.pending_agent_actions.clear();
        self.pending_agent_timing = None;
        let delay = Self::auto_restart_delay(pending_attempt);
        self.apply_phase(AutoRunPhase::TransientRecovery { backoff_ms: delay.as_millis() as u64 });

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
            "Waiting for connection… retrying in {human_delay} (attempt {pending_attempt})"
        ));
        self.current_display_is_summary = true;
        self.current_status_title = Some(format!("Retrying after error"));
        self.current_status_sent_to_user = Some(format!(
            "Encountered an error: {truncated_reason}. Waiting before retrying."
        ));
        self.placeholder_phrase = Some("Waiting for connection…".to_string());
        self.thinking_prefix_stripped = false;

        vec![
            AutoControllerEffect::CancelCoordinator,
            AutoControllerEffect::SetTaskRunning { running: false },
            AutoControllerEffect::UpdateTerminalHint {
                hint: Some("Press Esc to exit Auto Drive".to_string()),
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

    pub fn schedule_cli_prompt(
        &mut self,
        decision_seq: u64,
        prompt_text: String,
        countdown_override: Option<u8>,
    ) -> Vec<AutoControllerEffect> {
        self.current_cli_prompt = Some(prompt_text);
        self.apply_phase(AutoRunPhase::AwaitingCoordinator { prompt_ready: true });
        self.countdown_override = countdown_override;
        self.reset_countdown();
        self.countdown_id = self.countdown_id.wrapping_add(1);
        self.countdown_decision_seq = decision_seq;
        let countdown_id = self.countdown_id;
        let countdown = self.countdown_seconds();
        self.seconds_remaining = countdown.unwrap_or(0);

        let mut effects = vec![AutoControllerEffect::RefreshUi];
        if let Some(seconds) = countdown {
            effects.push(AutoControllerEffect::StartCountdown {
                countdown_id,
                decision_seq,
                seconds,
            });
        }
        effects
    }

    pub fn update_continue_mode(&mut self, mode: AutoContinueMode) -> Vec<AutoControllerEffect> {
        self.continue_mode = mode;
        self.countdown_override = None;
        self.reset_countdown();
        self.seconds_remaining = self.countdown_seconds().unwrap_or(0);

        let mut effects = vec![AutoControllerEffect::RefreshUi];
        if self.phase.awaiting_coordinator_submit() && !self.phase.is_paused_manual() {
            self.countdown_id = self.countdown_id.wrapping_add(1);
            let countdown_id = self.countdown_id;
            let decision_seq = self.countdown_decision_seq;
            let countdown = self.countdown_seconds();
            if let Some(seconds) = countdown {
                effects.push(AutoControllerEffect::StartCountdown {
                    countdown_id,
                    decision_seq,
                    seconds,
                });
            }
        }
        effects
    }

    pub fn handle_countdown_tick(
        &mut self,
        countdown_id: u64,
        decision_seq: u64,
        seconds_left: u8,
    ) -> Vec<AutoControllerEffect> {
        if !self.phase.is_active()
            || countdown_id != self.countdown_id
            || decision_seq != self.countdown_decision_seq
            || !self.phase.awaiting_coordinator_submit()
            || self.phase.is_paused_manual()
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
        let pending_stop_message = self.pending_stop_message.clone();
        let last_completion_explanation = self.last_completion_explanation.clone();

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
        self.pending_stop_message = pending_stop_message;
        self.last_completion_explanation = last_completion_explanation;
        self.review_lock = None;
        self.phase = if self.phase.is_active() {
            AutoRunPhase::Active
        } else {
            AutoRunPhase::Idle
        };
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
        self.phase.awaiting_coordinator_submit()
            && !self.phase.is_paused_manual()
            && self
                .countdown_seconds()
                .map(|seconds| seconds > 0)
                .unwrap_or(false)
    }

    pub fn countdown_seconds(&self) -> Option<u8> {
        self.countdown_override.or_else(|| self.continue_mode.seconds())
    }

    pub fn reset_countdown(&mut self) {
        self.seconds_remaining = self.countdown_seconds().unwrap_or(0);
        if self.seconds_remaining == 0 {
            self.countdown_decision_seq = 0;
        }
    }

    pub fn set_phase(&mut self, phase: AutoRunPhase) {
        self.phase = phase;
    }

    pub fn phase(&self) -> &AutoRunPhase {
        &self.phase
    }

    pub fn is_active(&self) -> bool {
        self.phase.is_active()
    }

    pub fn is_auto_active(&self) -> bool {
        self.phase.is_active()
    }

    pub fn current_phase(&self) -> &AutoRunPhase {
        &self.phase
    }

    pub fn should_show_goal_entry(&self) -> bool {
        self.phase.should_show_goal_entry()
    }

    pub fn set_bypass_coordinator_next_submit(&mut self) {
        if let AutoRunPhase::PausedManual { bypass_next_submit, .. } = &mut self.phase {
            *bypass_next_submit = true;
        }
    }

    pub fn should_bypass_coordinator_next_submit(&self) -> bool {
        matches!(
            self.phase,
            AutoRunPhase::PausedManual {
                bypass_next_submit: true,
                ..
            }
        )
    }

    pub fn clear_bypass_coordinator_flag(&mut self) {
        if let AutoRunPhase::PausedManual { bypass_next_submit, .. } = &mut self.phase {
            *bypass_next_submit = false;
        }
    }

    pub fn is_paused_manual(&self) -> bool {
        self.phase.is_paused_manual()
    }

    pub fn resume_after_submit(&self) -> bool {
        self.phase.resume_after_submit().unwrap_or(false)
    }

    pub fn awaiting_coordinator_submit(&self) -> bool {
        self.phase.awaiting_coordinator_submit()
    }

    pub fn awaiting_review(&self) -> bool {
        self.phase.awaiting_review()
    }

    pub fn in_transient_recovery(&self) -> bool {
        self.phase.in_transient_recovery()
    }

    pub fn is_waiting_for_response(&self) -> bool {
        self.phase.is_waiting_for_response()
    }

    pub fn is_coordinator_waiting(&self) -> bool {
        matches!(
            self.phase,
            AutoRunPhase::AwaitingDiagnostics {
                coordinator_waiting: true,
            }
        )
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

#[cfg(test)]
mod tests {
    use super::{AutoControllerEffect, AutoDriveController, AutoRunPhase};
    use std::time::Instant;

    #[test]
    fn bypass_flag_only_applies_once_across_manual_resume() {
        let mut controller = AutoDriveController::default();

        // Enter manual pause state with resume-after-submit set.
        controller.on_pause_for_manual(true);
        assert!(matches!(
            controller.current_phase(),
            AutoRunPhase::PausedManual {
                resume_after_submit: true,
                bypass_next_submit: false,
            }
        ));
        assert!(!controller.should_bypass_coordinator_next_submit());

        // First manual edit triggers bypass for the upcoming coordinator submit.
        controller.set_bypass_coordinator_next_submit();
        assert!(controller.should_bypass_coordinator_next_submit());

        // Simulate the manual path consuming the bypass before resuming automation.
        controller.clear_bypass_coordinator_flag();
        assert!(!controller.should_bypass_coordinator_next_submit());

        // Hitting bypass again should work, but resuming to Active must reset it.
        controller.set_bypass_coordinator_next_submit();
        assert!(controller.should_bypass_coordinator_next_submit());

        controller.on_resume_from_manual();
        assert!(matches!(controller.current_phase(), AutoRunPhase::Active));
        assert!(!controller.should_bypass_coordinator_next_submit());
    }

    #[test]
    fn awaiting_goal_entry_is_not_active_but_marks_goal_entry_visible() {
        let mut controller = AutoDriveController::default();
        assert!(!controller.should_show_goal_entry());
        assert!(!controller.is_active());

        controller.set_phase(AutoRunPhase::AwaitingGoalEntry);
        assert!(controller.should_show_goal_entry());
        assert!(!controller.is_active());
    }

    #[test]
    fn transient_failure_enters_recovery_and_schedules_restart() {
        let mut controller = AutoDriveController::default();
        controller.goal = Some("Investigate outage".to_string());
        controller.started_at = Some(Instant::now());

        let effects = controller.pause_for_transient_failure(
            Instant::now(),
            "network error".to_string(),
        );

        assert!(effects
            .iter()
            .any(|effect| matches!(effect, AutoControllerEffect::CancelCoordinator)));
        assert!(effects
            .iter()
            .any(|effect| matches!(effect, AutoControllerEffect::ScheduleRestart { .. })));
        assert!(matches!(
            controller.current_phase(),
            AutoRunPhase::TransientRecovery { .. }
        ));
        assert!(controller.last_run_summary.is_none());
    }

    #[test]
    fn countdown_tick_respects_decision_seq() {
        let mut controller = AutoDriveController::default();

        let _effects = controller.schedule_cli_prompt(1, "Test prompt".to_string(), None);
        let countdown_id = controller.countdown_id;

        let effects = controller.handle_countdown_tick(countdown_id, 1, 5);
        assert_eq!(effects.len(), 1);

        let effects = controller.handle_countdown_tick(countdown_id, 2, 5);
        assert!(effects.is_empty());
    }

    #[test]
    fn countdown_tick_final_emits_submit() {
        let mut controller = AutoDriveController::default();

        let _effects = controller.schedule_cli_prompt(7, "Prompt".to_string(), None);
        let countdown_id = controller.countdown_id;

        let effects = controller.handle_countdown_tick(countdown_id, 7, 0);
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], AutoControllerEffect::SubmitPrompt));
    }

    #[test]
    fn countdown_tick_ignores_stopped_phase() {
        let mut controller = AutoDriveController::default();
        let _effects = controller.schedule_cli_prompt(3, "Prompt".to_string(), None);
        let countdown_id = controller.countdown_id;
        controller.set_phase(AutoRunPhase::Idle);

        let effects = controller.handle_countdown_tick(countdown_id, 3, 5);
        assert!(effects.is_empty());
    }
}
