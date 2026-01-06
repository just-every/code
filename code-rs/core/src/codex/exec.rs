use super::*;
use super::session::{HookGuard, RunningExecMeta};
use code_protocol::models::ContentItem;
use serde_json::{json, Map, Value};
use shlex::split as shlex_split;

fn synthetic_exec_end_payload(cancelled: bool) -> (i32, String) {
    if cancelled {
        (130, "Command cancelled by user.".to_string())
    } else {
        (130, "Command interrupted before completion.".to_string())
    }
}

struct ExecDropGuard {
    sub_id: String,
    call_id: String,
    order_meta: crate::protocol::OrderMeta,
    tx_event: Sender<Event>,
    cancel_flag: Arc<AtomicBool>,
    end_emitted: Arc<AtomicBool>,
    session: Weak<Session>,
    completed: bool,
}

impl ExecDropGuard {
    fn new(
        session: Weak<Session>,
        tx_event: Sender<Event>,
        sub_id: String,
        call_id: String,
        order_meta: crate::protocol::OrderMeta,
        cancel_flag: Arc<AtomicBool>,
        end_emitted: Arc<AtomicBool>,
    ) -> Self {
        Self {
            sub_id,
            call_id,
            order_meta,
            tx_event,
            cancel_flag,
            end_emitted,
            session,
            completed: false,
        }
    }

    fn mark_completed(&mut self) {
        self.completed = true;
        self.end_emitted.store(true, Ordering::Release);
        self.remove_from_registry();
    }

    fn remove_from_registry(&self) {
        if let Some(session) = self.session.upgrade() {
            session.unregister_running_exec(&self.call_id);
        }
    }
}

impl Drop for ExecDropGuard {
    fn drop(&mut self) {
        self.remove_from_registry();

        if self.completed {
            return;
        }

        if self.end_emitted.swap(true, Ordering::AcqRel) {
            return;
        }

        let (exit_code, stderr) = synthetic_exec_end_payload(
            self.cancel_flag.load(Ordering::Acquire),
        );
        let msg = EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: self.call_id.clone(),
            stdout: String::new(),
            stderr,
            exit_code,
            duration: Duration::ZERO,
        });

        if let Some(session) = self.session.upgrade() {
            let event = session.make_event_with_order(
                &self.sub_id,
                msg,
                self.order_meta.clone(),
                self.order_meta.sequence_number,
            );
            let _ = self.tx_event.try_send(event);
        } else {
            // Fallback: emit directly if session no longer exists.
            let event = Event {
                id: self.sub_id.clone(),
                event_seq: 0,
                msg,
                order: Some(self.order_meta.clone()),
            };
            let _ = self.tx_event.try_send(event);
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExecCommandContext {
    pub(crate) sub_id: String,
    pub(crate) call_id: String,
    pub(crate) command_for_display: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) apply_patch: Option<ApplyPatchCommandContext>,
}

#[derive(Clone, Debug)]
pub(crate) struct ApplyPatchCommandContext {
    pub(crate) user_explicitly_approved_this_action: bool,
    pub(crate) changes: HashMap<PathBuf, FileChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HookPermissionDecision {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum HookStopDecision {
    Approve,
    Block { reason: Option<String> },
}

#[derive(Debug, Default, Clone)]
pub(super) struct HookOutput {
    continue_processing: Option<bool>,
    suppress_output: Option<bool>,
    system_message: Option<String>,
    permission_decision: Option<HookPermissionDecision>,
    updated_input: Option<Value>,
    stop_decision: Option<HookStopDecision>,
}

#[derive(Debug, Clone)]
pub(super) struct HookRunResult {
    pub(super) continue_processing: bool,
    pub(super) suppress_output: bool,
    pub(super) system_messages: Vec<String>,
    pub(super) permission_decision: Option<HookPermissionDecision>,
    pub(super) updated_input: Option<Value>,
    pub(super) stop_decision: Option<HookStopDecision>,
}

impl Default for HookRunResult {
    fn default() -> Self {
        Self {
            continue_processing: true,
            suppress_output: false,
            system_messages: Vec::new(),
            permission_decision: None,
            updated_input: None,
            stop_decision: None,
        }
    }
}

impl HookRunResult {
    fn apply(&mut self, output: HookOutput) {
        if let Some(flag) = output.continue_processing {
            if !flag {
                self.continue_processing = false;
            }
        }
        if matches!(output.suppress_output, Some(true)) {
            self.suppress_output = true;
        }
        if let Some(message) = output.system_message {
            if !message.trim().is_empty() {
                self.system_messages.push(message);
            }
        }
        if let Some(decision) = output.permission_decision {
            self.permission_decision = Some(merge_permission_decision(self.permission_decision, decision));
        }
        if let Some(updated) = output.updated_input {
            self.updated_input = Some(updated);
        }
        if let Some(decision) = output.stop_decision {
            self.stop_decision = Some(merge_stop_decision(self.stop_decision.take(), decision));
        }
    }
}

fn hook_reason_from_messages(messages: &[String]) -> Option<String> {
    messages
        .iter()
        .find(|message| !message.trim().is_empty())
        .map(|message| message.trim().to_string())
}

fn command_from_value(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let text = item.as_str()?;
                out.push(text.to_string());
            }
            if out.is_empty() { None } else { Some(out) }
        }
        Value::String(text) => shlex_split(text).filter(|tokens| !tokens.is_empty()),
        _ => None,
    }
}

pub(super) fn apply_updated_exec_params(
    params: &mut ExecParams,
    ctx: &mut ExecCommandContext,
    updated_input: Value,
) -> bool {
    if updated_input.is_null() {
        return false;
    }

    let mut changed = false;
    match updated_input {
        Value::Array(_) | Value::String(_) => {
            if let Some(command) = command_from_value(&updated_input) {
                params.command = command;
                changed = true;
            }
        }
        Value::Object(map) => {
            if let Some(command_value) = map.get("command") {
                if let Some(command) = command_from_value(command_value) {
                    params.command = command;
                    changed = true;
                }
            }
            if let Some(cwd_value) = map.get("cwd").and_then(|value| value.as_str()) {
                let trimmed = cwd_value.trim();
                if !trimmed.is_empty() {
                    let path = PathBuf::from(trimmed);
                    params.cwd = if path.is_absolute() {
                        path
                    } else {
                        params.cwd.join(path)
                    };
                    changed = true;
                }
            }
            if let Some(timeout_value) = map.get("timeout_ms") {
                if timeout_value.is_null() {
                    if params.timeout_ms.is_some() {
                        params.timeout_ms = None;
                        changed = true;
                    }
                } else if let Some(timeout_ms) = timeout_value.as_u64() {
                    params.timeout_ms = Some(timeout_ms);
                    changed = true;
                }
            }
            if let Some(env_map) = map.get("env").and_then(|value| value.as_object()) {
                for (key, value) in env_map {
                    if let Some(text) = value.as_str() {
                        params.env.insert(key.clone(), text.to_string());
                        changed = true;
                    }
                }
            }
        }
        _ => {}
    }

    if changed {
        ctx.command_for_display = params.command.clone();
        ctx.cwd = params.cwd.clone();
    }

    changed
}

fn merge_permission_decision(
    current: Option<HookPermissionDecision>,
    next: HookPermissionDecision,
) -> HookPermissionDecision {
    fn rank(decision: HookPermissionDecision) -> u8 {
        match decision {
            HookPermissionDecision::Deny => 3,
            HookPermissionDecision::Ask => 2,
            HookPermissionDecision::Allow => 1,
        }
    }
    match current {
        None => next,
        Some(existing) => {
            if rank(next) >= rank(existing) {
                next
            } else {
                existing
            }
        }
    }
}

fn merge_stop_decision(current: Option<HookStopDecision>, next: HookStopDecision) -> HookStopDecision {
    match (current, next) {
        (Some(HookStopDecision::Block { reason }), HookStopDecision::Block { reason: next_reason }) => {
            HookStopDecision::Block {
                reason: reason.or(next_reason),
            }
        }
        (Some(HookStopDecision::Block { reason }), HookStopDecision::Approve) => {
            HookStopDecision::Block { reason }
        }
        (Some(HookStopDecision::Approve), HookStopDecision::Block { reason }) => {
            HookStopDecision::Block { reason }
        }
        (None, HookStopDecision::Block { reason }) => HookStopDecision::Block { reason },
        _ => HookStopDecision::Approve,
    }
}

fn parse_hook_permission_decision(value: &str) -> Option<HookPermissionDecision> {
    match value.trim().to_ascii_lowercase().as_str() {
        "allow" | "approve" => Some(HookPermissionDecision::Allow),
        "deny" | "block" => Some(HookPermissionDecision::Deny),
        "ask" | "confirm" => Some(HookPermissionDecision::Ask),
        _ => None,
    }
}

fn parse_stop_decision(value: &str, reason: Option<String>) -> Option<HookStopDecision> {
    match value.trim().to_ascii_lowercase().as_str() {
        "approve" | "allow" => Some(HookStopDecision::Approve),
        "block" | "deny" => Some(HookStopDecision::Block { reason }),
        _ => None,
    }
}

fn extract_json_from_text(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let stripped = if trimmed.starts_with("```") {
        let mut lines = trimmed.lines();
        let _ = lines.next();
        let mut body = lines.collect::<Vec<_>>().join("\n");
        if let Some(idx) = body.rfind("```") {
            body.truncate(idx);
        }
        body.trim().to_string()
    } else {
        trimmed.to_string()
    };

    if serde_json::from_str::<Value>(&stripped).is_ok() {
        return Some(stripped);
    }

    let start = stripped.find('{')?;
    let end = stripped.rfind('}')?;
    if end <= start {
        return None;
    }
    let candidate = stripped[start..=end].trim();
    serde_json::from_str::<Value>(candidate).ok()?;
    Some(candidate.to_string())
}

fn parse_hook_output_from_text(raw: &str) -> HookOutput {
    let mut output = HookOutput::default();
    let Some(json_text) = extract_json_from_text(raw) else {
        return output;
    };
    let Ok(value) = serde_json::from_str::<Value>(&json_text) else {
        return output;
    };

    if let Some(flag) = value.get("continue").and_then(|v| v.as_bool()) {
        output.continue_processing = Some(flag);
    }
    if let Some(flag) = value.get("suppressOutput").and_then(|v| v.as_bool()) {
        output.suppress_output = Some(flag);
    }
    if let Some(message) = value.get("systemMessage").and_then(|v| v.as_str()) {
        output.system_message = Some(message.to_string());
    }

    let permission = value
        .pointer("/hookSpecificOutput/permissionDecision")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("permissionDecision").and_then(|v| v.as_str()));
    output.permission_decision = permission.and_then(parse_hook_permission_decision);

    output.updated_input = value
        .pointer("/hookSpecificOutput/updatedInput")
        .cloned()
        .or_else(|| value.get("updatedInput").cloned());

    if let Some(decision) = value.get("decision").and_then(|v| v.as_str()) {
        let reason = value.get("reason").and_then(|v| v.as_str()).map(|s| s.to_string());
        output.stop_decision = parse_stop_decision(decision, reason);
    }

    output
}

fn sanitize_identifier(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else {
            slug.push('_');
        }
    }
    while slug.starts_with('_') {
        slug.remove(0);
    }
    if slug.is_empty() {
        slug.push_str("hook");
    }
    slug
}

fn truncate_payload(text: &str, limit: usize) -> String {
    let mut iter = text.chars();
    let truncated: String = iter.by_ref().take(limit).collect();
    if iter.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn permission_mode_label(policy: AskForApproval) -> &'static str {
    match policy {
        AskForApproval::Never => "allow",
        _ => "ask",
    }
}

pub(super) fn build_base_hook_payload(
    sess: &Session,
    event: ProjectHookEvent,
) -> Map<String, Value> {
    let mut base = Map::new();
    base.insert("session_id".to_string(), Value::String(sess.id.to_string()));
    let transcript_path = sess
        .rollout
        .lock()
        .unwrap()
        .as_ref()
        .map(|r| r.rollout_path.to_string_lossy().to_string());
    base.insert(
        "transcript_path".to_string(),
        transcript_path.map(Value::String).unwrap_or(Value::Null),
    );
    base.insert(
        "cwd".to_string(),
        Value::String(sess.cwd.to_string_lossy().to_string()),
    );
    base.insert(
        "permission_mode".to_string(),
        Value::String(permission_mode_label(sess.approval_policy).to_string()),
    );
    base.insert(
        "hook_event_name".to_string(),
        Value::String(event.hook_event_name().to_string()),
    );
    base
}

fn build_exec_hook_payload(
    sess: &Session,
    event: ProjectHookEvent,
    ctx: &ExecCommandContext,
    params: &ExecParams,
    output: Option<&ExecToolCallOutput>,
    tool_name: Option<&str>,
) -> Value {
    let mut base = build_base_hook_payload(sess, event);
    base.insert("event".to_string(), Value::String(event.as_str().to_string()));
    base.insert("call_id".to_string(), Value::String(ctx.call_id.clone()));
    base.insert(
        "cwd".to_string(),
        Value::String(ctx.cwd.to_string_lossy().to_string()),
    );
    base.insert("command".to_string(), json!(params.command));
    base.insert("timeout_ms".to_string(), json!(params.timeout_ms));
    if let Some(tool_name) = tool_name {
        base.insert("tool_name".to_string(), Value::String(tool_name.to_string()));
        base.insert("tool_use_id".to_string(), Value::String(ctx.call_id.clone()));
    }

    match event {
        ProjectHookEvent::ToolAfter => {
            if let Some(out) = output {
                base.insert(
                    "tool_result".to_string(),
                    json!({
                        "exit_code": out.exit_code,
                        "duration_ms": out.duration.as_millis(),
                        "timed_out": out.timed_out,
                        "stdout": truncate_payload(&out.stdout.text, HOOK_OUTPUT_LIMIT),
                        "stderr": truncate_payload(&out.stderr.text, HOOK_OUTPUT_LIMIT),
                        "success": out.exit_code == 0,
                    }),
                );
                base.insert("exit_code".to_string(), json!(out.exit_code));
                base.insert("duration_ms".to_string(), json!(out.duration.as_millis()));
                base.insert("timed_out".to_string(), json!(out.timed_out));
                base.insert(
                    "stdout".to_string(),
                    json!(truncate_payload(&out.stdout.text, HOOK_OUTPUT_LIMIT)),
                );
                base.insert(
                    "stderr".to_string(),
                    json!(truncate_payload(&out.stderr.text, HOOK_OUTPUT_LIMIT)),
                );
            }
            Value::Object(base)
        }
        ProjectHookEvent::FileBeforeWrite => {
            let changes = ctx
                .apply_patch
                .as_ref()
                .and_then(|p| serde_json::to_value(&p.changes).ok())
                .unwrap_or(Value::Null);
            base.insert("changes".to_string(), changes);
            Value::Object(base)
        }
        ProjectHookEvent::FileAfterWrite => {
            let changes = ctx
                .apply_patch
                .as_ref()
                .and_then(|p| serde_json::to_value(&p.changes).ok())
                .unwrap_or(Value::Null);
            base.insert("changes".to_string(), changes);
            if let Some(out) = output {
                base.insert("exit_code".to_string(), json!(out.exit_code));
                base.insert("duration_ms".to_string(), json!(out.duration.as_millis()));
                base.insert("timed_out".to_string(), json!(out.timed_out));
                base.insert(
                    "stdout".to_string(),
                    json!(truncate_payload(&out.stdout.text, HOOK_OUTPUT_LIMIT)),
                );
                base.insert(
                    "stderr".to_string(),
                    json!(truncate_payload(&out.stderr.text, HOOK_OUTPUT_LIMIT)),
                );
                base.insert("success".to_string(), json!(out.exit_code == 0));
            }
            Value::Object(base)
        }
        _ => Value::Object(base),
    }
}

pub(super) fn build_user_prompt_hook_payload(sess: &Session, prompt: &str) -> Value {
    let mut base = build_base_hook_payload(sess, ProjectHookEvent::UserPromptSubmit);
    base.insert(
        "event".to_string(),
        Value::String(ProjectHookEvent::UserPromptSubmit.as_str().to_string()),
    );
    base.insert("user_prompt".to_string(), Value::String(prompt.to_string()));
    Value::Object(base)
}

pub(super) fn build_stop_hook_payload(
    sess: &Session,
    event: ProjectHookEvent,
    reason: Option<String>,
    details: Option<Value>,
) -> Value {
    let mut base = build_base_hook_payload(sess, event);
    base.insert("event".to_string(), Value::String(event.as_str().to_string()));
    if let Some(reason) = reason {
        base.insert("reason".to_string(), Value::String(reason));
    }
    if let Some(details) = details {
        base.insert("details".to_string(), details);
    }
    Value::Object(base)
}

pub(super) fn build_precompact_hook_payload(sess: &Session, reason: &str) -> Value {
    let mut base = build_base_hook_payload(sess, ProjectHookEvent::PreCompact);
    base.insert(
        "event".to_string(),
        Value::String(ProjectHookEvent::PreCompact.as_str().to_string()),
    );
    base.insert("reason".to_string(), Value::String(reason.to_string()));
    Value::Object(base)
}

pub(super) fn build_postcompact_hook_payload(sess: &Session, reason: &str) -> Value {
    let mut base = build_base_hook_payload(sess, ProjectHookEvent::PostCompact);
    base.insert(
        "event".to_string(),
        Value::String(ProjectHookEvent::PostCompact.as_str().to_string()),
    );
    base.insert("reason".to_string(), Value::String(reason.to_string()));
    Value::Object(base)
}

pub(super) fn build_notification_hook_payload(
    sess: &Session,
    notification: &UserNotification,
) -> Value {
    let mut base = build_base_hook_payload(sess, ProjectHookEvent::Notification);
    base.insert(
        "event".to_string(),
        Value::String(ProjectHookEvent::Notification.as_str().to_string()),
    );
    let payload = serde_json::to_value(notification).unwrap_or(Value::Null);
    base.insert("notification".to_string(), payload);
    Value::Object(base)
}

pub struct ExecInvokeArgs<'a> {
    pub params: ExecParams,
    pub sandbox_type: SandboxType,
    pub sandbox_policy: &'a SandboxPolicy,
    pub sandbox_cwd: &'a std::path::Path,
    pub code_linux_sandbox_exe: &'a Option<PathBuf>,
    pub stdout_stream: Option<StdoutStream>,
}

pub(super) fn maybe_run_with_user_profile(mut params: ExecParams, sess: &Session) -> ExecParams {
    if sess.shell_environment_policy.use_profile {
        let maybe_command = sess
            .user_shell
            .format_default_shell_invocation(params.command.clone());
        if let Some(command) = maybe_command {
            params.command = command;
        }
    }

    suppress_bash_job_control(&mut params.command);

    params
}

fn suppress_bash_job_control(command: &mut [String]) {
    let [program, flag, script] = command else {
        return;
    };
    if !is_bash_executable(program) || flag != "-lc" {
        return;
    }

    let trimmed = script.trim_start();
    if trimmed.starts_with("set +m") {
        return;
    }

    let original = script.clone();
    *script = format!("set +m; {original}");
}

fn is_bash_executable(token: &str) -> bool {
    let trimmed = token.trim_matches('"').trim_matches('\'');
    let name = std::path::Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    matches!(name.as_str(), "bash" | "bash.exe")
}

impl Session {
    pub(super) async fn on_exec_command_begin(
        &self,
        turn_diff_tracker: &mut TurnDiffTracker,
        exec_command_context: ExecCommandContext,
        seq_hint: Option<u64>,
        output_index: Option<u32>,
        attempt_req: u64,
    ) {
        let ExecCommandContext {
            sub_id,
            call_id,
            command_for_display,
            cwd,
            apply_patch,
        } = exec_command_context;
        let msg = match apply_patch {
            Some(ApplyPatchCommandContext {
                user_explicitly_approved_this_action,
                changes,
            }) => {
                turn_diff_tracker.on_patch_begin(&changes);

                EventMsg::PatchApplyBegin(PatchApplyBeginEvent {
                    call_id,
                    auto_approved: !user_explicitly_approved_this_action,
                    changes,
                })
            }
            None => EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id,
                command: command_for_display.clone(),
                cwd,
                parsed_cmd: parse_command(&command_for_display),
            }),
        };
        let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number: seq_hint };
        let event = self.make_event_with_order(&sub_id, msg, order, seq_hint);
        let _ = self.tx_event.send(event).await;
    }

    async fn on_exec_command_end(
        &self,
        turn_diff_tracker: &mut TurnDiffTracker,
        sub_id: &str,
        call_id: &str,
        output: &ExecToolCallOutput,
        is_apply_patch: bool,
        seq_hint: Option<u64>,
        output_index: Option<u32>,
        attempt_req: u64,
    ) {
        let ExecToolCallOutput {
            stdout,
            stderr,
            aggregated_output: _,
            duration,
            exit_code,
            timed_out: _,
        } = output;
        // Because stdout and stderr could each be up to 100 KiB, we send
        // truncated versions.
        const MAX_STREAM_OUTPUT: usize = 5 * 1024; // 5KiB
        let stdout = stdout.text.chars().take(MAX_STREAM_OUTPUT).collect();
        let stderr = stderr.text.chars().take(MAX_STREAM_OUTPUT).collect();
        // Precompute formatted output if needed in future for logging/pretty UI.

        let msg = if is_apply_patch {
            EventMsg::PatchApplyEnd(PatchApplyEndEvent {
                call_id: call_id.to_string(),
                stdout,
                stderr,
                success: *exit_code == 0,
            })
        } else {
            EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                call_id: call_id.to_string(),
                stdout,
                stderr,
                exit_code: *exit_code,
                duration: *duration,
            })
        };
        let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number: seq_hint };
        let event = self.make_event_with_order(sub_id, msg, order, seq_hint);
        let _ = self.tx_event.send(event).await;

        // If this is an apply_patch, after we emit the end patch, emit a second event
        // with the full turn diff if there is one.
        if is_apply_patch {
            let unified_diff = turn_diff_tracker.get_unified_diff();
            if let Ok(Some(unified_diff)) = unified_diff {
                let msg = EventMsg::TurnDiff(TurnDiffEvent { unified_diff });
                let event = self.make_event(sub_id, msg);
                let _ = self.tx_event.send(event).await;
            }
        }

    }
    /// Runs the exec tool call and emits events for the begin and end of the
    /// command even on error.
    ///
    /// Returns the output of the exec tool call.
    pub(super) async fn run_exec_with_events<'a>(
        &self,
        turn_diff_tracker: &mut TurnDiffTracker,
        begin_ctx: ExecCommandContext,
        exec_args: ExecInvokeArgs<'a>,
        seq_hint: Option<u64>,
        output_index: Option<u32>,
        attempt_req: u64,
    ) -> crate::error::Result<ExecToolCallOutput> {
        self
            .run_exec_with_events_inner(
                turn_diff_tracker,
                begin_ctx,
                exec_args,
                seq_hint,
                output_index,
                attempt_req,
                true,
            )
            .await
    }

    fn track_running_exec(
        &self,
        call_id: &str,
        sub_id: &str,
        order_meta: crate::protocol::OrderMeta,
        cancel_flag: Arc<AtomicBool>,
        end_emitted: Arc<AtomicBool>,
    ) {
        let mut state = self.state.lock().unwrap();
        state.running_execs.insert(
            call_id.to_string(),
            RunningExecMeta {
                sub_id: sub_id.to_string(),
                order_meta,
                cancel_flag,
                end_emitted,
            },
        );
    }

    fn unregister_running_exec(&self, call_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.running_execs.remove(call_id);
    }

    fn mark_running_exec_as_cancelled(&self, sub_id: &str) {
        let state = self.state.lock().unwrap();
        for meta in state.running_execs.values() {
            if meta.sub_id == sub_id {
                meta.cancel_flag.store(true, Ordering::Release);
            }
        }
    }

    pub(super) fn mark_all_running_execs_as_cancelled(&self) {
        let sub_ids: Vec<String> = {
            let state = self.state.lock().unwrap();
            state
                .running_execs
                .values()
                .map(|meta| meta.sub_id.clone())
                .collect()
        };
        for sub_id in sub_ids {
            self.mark_running_exec_as_cancelled(&sub_id);
        }
    }

    async fn finalize_cancelled_execs(&self, sub_id: &str) {
        let mut to_emit = Vec::new();
        {
            let mut state = self.state.lock().unwrap();
            let mut remove_keys = Vec::new();
            for (call_id, meta) in state.running_execs.iter() {
                if meta.sub_id == sub_id && !meta.end_emitted.load(Ordering::Acquire) {
                    to_emit.push((
                        call_id.clone(),
                        meta.order_meta.clone(),
                        meta.cancel_flag.clone(),
                        meta.end_emitted.clone(),
                    ));
                    remove_keys.push(call_id.clone());
                }
            }
            for key in remove_keys {
                state.running_execs.remove(&key);
            }
        }

        for (call_id, order_meta, cancel_flag, end_emitted) in to_emit {
            cancel_flag.store(true, Ordering::Release);
            if !end_emitted.swap(true, Ordering::AcqRel) {
                let (exit_code, stderr) = synthetic_exec_end_payload(true);
                let msg = EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                    call_id,
                    stdout: String::new(),
                    stderr,
                    exit_code,
                    duration: Duration::ZERO,
                });
                let event = self.make_event_with_order(sub_id, msg, order_meta.clone(), order_meta.sequence_number);
                let _ = self.tx_event.send(event).await;
            }
        }
    }

    async fn run_exec_with_events_inner<'a>(
        &self,
        turn_diff_tracker: &mut TurnDiffTracker,
        mut begin_ctx: ExecCommandContext,
        exec_args: ExecInvokeArgs<'a>,
        seq_hint: Option<u64>,
        output_index: Option<u32>,
        attempt_req: u64,
        enable_hooks: bool,
    ) -> crate::error::Result<ExecToolCallOutput> {
        let is_apply_patch = begin_ctx.apply_patch.is_some();
        let sub_id = begin_ctx.sub_id.clone();
        let call_id = begin_ctx.call_id.clone();

        let order_for_end = crate::protocol::OrderMeta {
            request_ordinal: attempt_req,
            output_index,
            sequence_number: seq_hint.map(|h| h.saturating_add(1)),
        };

        let cancel_flag = Arc::new(AtomicBool::new(false));
        let end_emitted = Arc::new(AtomicBool::new(false));
        self.track_running_exec(&call_id, &sub_id, order_for_end.clone(), cancel_flag.clone(), end_emitted.clone());

        let mut exec_guard = ExecDropGuard::new(
            self.self_handle.clone(),
            self.tx_event.clone(),
            sub_id.clone(),
            call_id.clone(),
            order_for_end.clone(),
            cancel_flag,
            end_emitted,
        );

        let ExecInvokeArgs { params, sandbox_type, sandbox_policy, sandbox_cwd, code_linux_sandbox_exe, stdout_stream } = exec_args;
        let mut params = maybe_run_with_user_profile(params, self);
        let mut params_for_hooks = if enable_hooks {
            Some(params.clone())
        } else {
            None
        };

        if enable_hooks {
            if let Some(params_ref) = params_for_hooks.as_ref() {
                let before_event = if is_apply_patch {
                    ProjectHookEvent::FileBeforeWrite
                } else {
                    ProjectHookEvent::ToolBefore
                };
                let hook_result = self
                    .run_hooks_for_exec_event(
                        turn_diff_tracker,
                        before_event,
                        &begin_ctx,
                        params_ref,
                        None,
                        attempt_req,
                    )
                    .await;
                let HookRunResult {
                    updated_input,
                    permission_decision,
                    system_messages,
                    ..
                } = hook_result;
                let hook_reason = hook_reason_from_messages(&system_messages);
                self.enqueue_hook_system_messages(system_messages);
                if let Some(updated_input) = updated_input {
                    if apply_updated_exec_params(&mut params, &mut begin_ctx, updated_input) {
                        if let Some(params_ref) = params_for_hooks.as_mut() {
                            *params_ref = params.clone();
                        }
                    }
                }
                if let Some(decision) = permission_decision {
                    let rejection = match decision {
                        HookPermissionDecision::Allow => None,
                        HookPermissionDecision::Deny => Some("exec command blocked by hook".to_string()),
                        HookPermissionDecision::Ask => {
                            let rx_approve = self
                                .request_command_approval(
                                    sub_id.clone(),
                                    call_id.clone(),
                                    params.command.clone(),
                                    params.cwd.clone(),
                                    hook_reason.clone(),
                                )
                                .await;
                            let decision = rx_approve.await.unwrap_or_default();
                            match decision {
                                ReviewDecision::Approved => None,
                                ReviewDecision::ApprovedForSession => {
                                    self.add_approved_command(ApprovedCommandPattern::new(
                                        params.command.clone(),
                                        ApprovedCommandMatchKind::Exact,
                                        None,
                                    ));
                                    None
                                }
                                ReviewDecision::Denied | ReviewDecision::Abort => {
                                    Some("exec command blocked by hook".to_string())
                                }
                            }
                        }
                    };

                    if let Some(message) = rejection {
                        let message = match hook_reason {
                            Some(reason) => format!("{message}: {reason}"),
                            None => message,
                        };
                        let output = ExecToolCallOutput {
                            exit_code: 1,
                            stdout: StreamOutput::new(String::new()),
                            stderr: StreamOutput::new(message.clone()),
                            aggregated_output: StreamOutput::new(message.clone()),
                            duration: Duration::ZERO,
                            timed_out: false,
                        };
                        self.on_exec_command_begin(
                            turn_diff_tracker,
                            begin_ctx.clone(),
                            seq_hint,
                            output_index,
                            attempt_req,
                        )
                        .await;
                        self
                            .on_exec_command_end(
                                turn_diff_tracker,
                                &sub_id,
                                &call_id,
                                &output,
                                is_apply_patch,
                                seq_hint.map(|h| h.saturating_add(1)),
                                output_index,
                                attempt_req,
                            )
                            .await;
                        exec_guard.mark_completed();
                        self.finalize_cancelled_execs(&sub_id).await;
                        return Ok(output);
                    }
                }
            }
        }

        let tracking_command = params.command.clone();
        let dry_run_analysis = analyze_command(&tracking_command);

        self.on_exec_command_begin(turn_diff_tracker, begin_ctx.clone(), seq_hint, output_index, attempt_req)
            .await;

        let result = process_exec_tool_call(params, sandbox_type, sandbox_policy, sandbox_cwd, code_linux_sandbox_exe, stdout_stream)
        .await;

        let output_stderr;
        let borrowed: &ExecToolCallOutput = match &result {
            Ok(output) => output,
            Err(CodexErr::Sandbox(SandboxErr::Timeout { output })) => output,
            Err(e) => {
                output_stderr = ExecToolCallOutput {
                    exit_code: -1,
                    stdout: StreamOutput::new(String::new()),
                    stderr: StreamOutput::new(get_error_message_ui(e)),
                    aggregated_output: StreamOutput::new(get_error_message_ui(e)),
                    duration: Duration::default(),
                    timed_out: false,
                };
                &output_stderr
            }
        };
        self.on_exec_command_end(
            turn_diff_tracker,
            &sub_id,
            &call_id,
            borrowed,
            is_apply_patch,
            seq_hint.map(|h| h.saturating_add(1)),
            output_index,
            attempt_req,
        )
        .await;

        exec_guard.mark_completed();
        self.finalize_cancelled_execs(&sub_id).await;

        if enable_hooks {
            if let Some(params_ref) = params_for_hooks.as_ref() {
                let after_event = if is_apply_patch {
                    ProjectHookEvent::FileAfterWrite
                } else {
                    ProjectHookEvent::ToolAfter
                };
                let hook_result = self
                    .run_hooks_for_exec_event(
                        turn_diff_tracker,
                        after_event,
                        &begin_ctx,
                        params_ref,
                        Some(borrowed),
                        attempt_req,
                    )
                    .await;
                self.enqueue_hook_system_messages(hook_result.system_messages);
            }
        }

        if let Some(analysis) = dry_run_analysis.as_ref() {
            let mut state = self.state.lock().unwrap();
            state.dry_run_guard.note_execution(analysis);
        }

        result
    }

    /// Helper that emits a BackgroundEvent with explicit ordering metadata.
    pub(crate) async fn notify_background_event_with_order(
        &self,
        sub_id: &str,
        order: crate::protocol::OrderMeta,
        message: impl Into<String>,
    ) {
        let event = self.make_event_with_order(
            sub_id,
            EventMsg::BackgroundEvent(BackgroundEventEvent { message: message.into() }),
            order,
            None,
        );
        let _ = self.tx_event.send(event).await;
    }

    pub(super) async fn notify_stream_error(&self, sub_id: &str, message: impl Into<String>) {
        let event = self.make_event(
            sub_id,
            EventMsg::Error(ErrorEvent { message: message.into() }),
        );
        let _ = self.tx_event.send(event).await;
    }

    fn resolve_internal_sandbox(&self, with_escalated_permissions: bool) -> SandboxType {
        match assess_safety_for_untrusted_command(
            self.approval_policy,
            &self.sandbox_policy,
            with_escalated_permissions,
        ) {
            SafetyCheck::AutoApprove { sandbox_type, .. } => sandbox_type,
            SafetyCheck::AskUser | SafetyCheck::Reject { .. } => {
                crate::safety::get_platform_sandbox().unwrap_or(SandboxType::None)
            }
        }
    }

    pub(super) async fn run_hooks_for_exec_event(
        &self,
        turn_diff_tracker: &mut TurnDiffTracker,
        event: ProjectHookEvent,
        exec_ctx: &ExecCommandContext,
        params: &ExecParams,
        output: Option<&ExecToolCallOutput>,
        attempt_req: u64,
    ) -> HookRunResult {
        if self.project_hooks.is_empty() {
            return HookRunResult::default();
        }
        let hooks: Vec<ProjectHook> = self.project_hooks.hooks_for(event).cloned().collect();
        if hooks.is_empty() {
            return HookRunResult::default();
        }
        let Some(_guard) = HookGuard::try_acquire(&self.hook_guard) else {
            return HookRunResult::default();
        };
        let payload = build_exec_hook_payload(self, event, exec_ctx, params, output, None);
        let mut result = HookRunResult::default();
        for (idx, hook) in hooks.into_iter().enumerate() {
            let output = self
                .run_hook_command(
                    turn_diff_tracker,
                    &hook,
                    event,
                    &payload,
                    Some(exec_ctx),
                    attempt_req,
                    idx,
                )
                .await;
            result.apply(output);
            if !result.continue_processing {
                break;
            }
        }
        result
    }

    pub(super) async fn run_hooks_for_event(
        &self,
        turn_diff_tracker: &mut TurnDiffTracker,
        event: ProjectHookEvent,
        payload: &Value,
        base_ctx: Option<&ExecCommandContext>,
        attempt_req: u64,
    ) -> HookRunResult {
        if self.project_hooks.is_empty() {
            return HookRunResult::default();
        }
        let hooks: Vec<ProjectHook> = self.project_hooks.hooks_for(event).cloned().collect();
        if hooks.is_empty() {
            return HookRunResult::default();
        }
        let Some(_guard) = HookGuard::try_acquire(&self.hook_guard) else {
            return HookRunResult::default();
        };

        let mut result = HookRunResult::default();
        for (idx, hook) in hooks.into_iter().enumerate() {
            let output = self
                .run_hook_command(
                    turn_diff_tracker,
                    &hook,
                    event,
                    payload,
                    base_ctx,
                    attempt_req,
                    idx,
                )
                .await;
            result.apply(output);
            if !result.continue_processing {
                break;
            }
        }

        result
    }

    pub(super) async fn run_session_hooks(&self, event: ProjectHookEvent) {
        let payload = self.build_session_payload(event);
        let mut tracker = TurnDiffTracker::new();
        let attempt_req = self.current_request_ordinal();
        let result = self
            .run_hooks_for_event(&mut tracker, event, &payload, None, attempt_req)
            .await;
        self.enqueue_hook_system_messages(result.system_messages);
    }

    pub(super) fn enqueue_hook_system_messages(&self, messages: Vec<String>) {
        if self.client.get_provider().wire_api == crate::model_provider_info::WireApi::Responses {
            return;
        }
        for message in messages {
            let trimmed = message.trim();
            if trimmed.is_empty() {
                continue;
            }
            self.add_pending_input(ResponseInputItem::Message {
                role: "system".to_string(),
                content: vec![ContentItem::InputText {
                    text: trimmed.to_string(),
                }],
            });
        }
    }

    fn build_session_payload(&self, event: ProjectHookEvent) -> Value {
        match event {
            ProjectHookEvent::SessionStart => json!({
                "event": event.as_str(),
                "cwd": self.cwd.to_string_lossy(),
                "sandbox_policy": format!("{}", self.sandbox_policy),
                "approval_policy": format!("{}", self.approval_policy),
            }),
            ProjectHookEvent::SessionEnd => json!({
                "event": event.as_str(),
                "cwd": self.cwd.to_string_lossy(),
                "sandbox_policy": format!("{}", self.sandbox_policy),
                "approval_policy": format!("{}", self.approval_policy),
            }),
            _ => json!({ "event": event.as_str() }),
        }
    }

    async fn run_hook_command(
        &self,
        turn_diff_tracker: &mut TurnDiffTracker,
        hook: &ProjectHook,
        event: ProjectHookEvent,
        payload: &Value,
        base_ctx: Option<&ExecCommandContext>,
        attempt_req: u64,
        index: usize,
    ) -> HookOutput {
        let sub_id = base_ctx
            .map(|ctx| ctx.sub_id.clone())
            .unwrap_or_else(|| INITIAL_SUBMIT_ID.to_string());
        let base_slug = base_ctx
            .map(|ctx| sanitize_identifier(&ctx.call_id))
            .unwrap_or_else(|| event.slug().to_string());
        let call_id = format!("{base_slug}_hook_{}_{}", event.slug(), index + 1);

        let mut env = hook.env.clone();
        env.entry("CODE_HOOK_EVENT".to_string())
            .or_insert_with(|| event.as_str().to_string());
        env.entry("CODE_HOOK_TRIGGER".to_string())
            .or_insert_with(|| event.slug().to_string());
        env.insert("CODE_HOOK_CALL_ID".to_string(), call_id.clone());
        env.insert("CODE_HOOK_SUB_ID".to_string(), sub_id.clone());
        env.insert("CODE_HOOK_INDEX".to_string(), (index + 1).to_string());
        env.insert("CODE_HOOK_PAYLOAD".to_string(), payload.to_string());
        env.entry("CODE_SESSION_CWD".to_string())
            .or_insert_with(|| self.cwd.to_string_lossy().to_string());
        if let Some(name) = &hook.name {
            env.entry("CODE_HOOK_NAME".to_string())
                .or_insert_with(|| name.clone());
        }
        if let Some(ctx) = base_ctx {
            env.entry("CODE_HOOK_SOURCE_CALL_ID".to_string())
                .or_insert_with(|| ctx.call_id.clone());
        }

        let exec_params = ExecParams {
            command: hook.command.clone(),
            cwd: hook.resolved_cwd(self.get_cwd()),
            timeout_ms: hook.timeout_ms,
            env,
            with_escalated_permissions: Some(false),
            justification: None,
        };

        let exec_ctx = ExecCommandContext {
            sub_id: sub_id.clone(),
            call_id: call_id.clone(),
            command_for_display: exec_params.command.clone(),
            cwd: exec_params.cwd.clone(),
            apply_patch: None,
        };

        let sandbox_type = self.resolve_internal_sandbox(false);
        let exec_args = ExecInvokeArgs {
            params: exec_params,
            sandbox_type,
            sandbox_policy: &self.sandbox_policy,
            sandbox_cwd: self.get_cwd(),
            code_linux_sandbox_exe: &self.code_linux_sandbox_exe,
            stdout_stream: None,
        };

        let ExecInvokeArgs {
            params,
            sandbox_type,
            sandbox_policy,
            sandbox_cwd,
            code_linux_sandbox_exe,
            stdout_stream,
        } = exec_args;

        let result = if hook.run_in_background {
            crate::exec::process_exec_tool_call(
                params,
                sandbox_type,
                sandbox_policy,
                sandbox_cwd,
                code_linux_sandbox_exe,
                None,
            )
            .await
        } else {
            let exec_args = ExecInvokeArgs {
                params,
                sandbox_type,
                sandbox_policy,
                sandbox_cwd,
                code_linux_sandbox_exe,
                stdout_stream,
            };
            Box::pin(self.run_exec_with_events_inner(
                turn_diff_tracker,
                exec_ctx,
                exec_args,
                None,
                None,
                attempt_req,
                false,
            ))
            .await
        };

        match result {
            Ok(output) => {
                let stdout_text = output.stdout.text.trim();
                let stderr_text = output.stderr.text.trim();
                let mut hook_output = if !stdout_text.is_empty() {
                    parse_hook_output_from_text(&output.stdout.text)
                } else if !stderr_text.is_empty() {
                    parse_hook_output_from_text(&output.stderr.text)
                } else {
                    HookOutput::default()
                };
                if output.exit_code == 2 {
                    if hook_output.system_message.is_none() && !stderr_text.is_empty() {
                        hook_output.system_message = Some(output.stderr.text.clone());
                    }
                    hook_output.continue_processing = Some(false);
                }
                hook_output
            }
            Err(err) => {
                let hook_label = hook
                    .name
                    .as_deref()
                    .unwrap_or_else(|| hook.command.first().map(String::as_str).unwrap_or("hook"));
                let order = self.next_background_order(&sub_id, attempt_req, None);
                self
                    .notify_background_event_with_order(
                        &sub_id,
                        order,
                        format!("Hook `{}` failed: {}", hook_label, get_error_message_ui(&err)),
                    )
                    .await;
                HookOutput::default()
            }
        }
    }

    fn find_project_command(&self, candidate: &str) -> Option<ProjectCommand> {
        self.project_commands
            .iter()
            .find(|cmd| cmd.matches(candidate))
            .cloned()
    }

    pub(super) async fn run_project_command(
        &self,
        turn_diff_tracker: &mut TurnDiffTracker,
        sub_id: &str,
        name: &str,
        attempt_req: u64,
    ) {
        let Some(command) = self.find_project_command(name) else {
            let order = self.next_background_order(sub_id, attempt_req, None);
            self
                .notify_background_event_with_order(
                    sub_id,
                    order,
                    format!("Unknown project command `{}`", name.trim()),
                )
                .await;
            return;
        };

        let mut env = command.env.clone();
        env.entry("CODE_PROJECT_COMMAND_NAME".to_string())
            .or_insert_with(|| command.name.clone());
        if let Some(desc) = &command.description {
            env.entry("CODE_PROJECT_COMMAND_DESCRIPTION".to_string())
                .or_insert_with(|| desc.clone());
        }
        env.entry("CODE_SESSION_CWD".to_string())
            .or_insert_with(|| self.cwd.to_string_lossy().to_string());

        let exec_params = ExecParams {
            command: command.command.clone(),
            cwd: command.resolved_cwd(self.get_cwd()),
            timeout_ms: command.timeout_ms,
            env,
            with_escalated_permissions: Some(false),
            justification: None,
        };

        let call_id = format!("project_cmd_{}", sanitize_identifier(&command.name));
        let exec_ctx = ExecCommandContext {
            sub_id: sub_id.to_string(),
            call_id: call_id.clone(),
            command_for_display: exec_params.command.clone(),
            cwd: exec_params.cwd.clone(),
            apply_patch: None,
        };

        let sandbox_type = self.resolve_internal_sandbox(false);
        let exec_args = ExecInvokeArgs {
            params: exec_params,
            sandbox_type,
            sandbox_policy: &self.sandbox_policy,
            sandbox_cwd: self.get_cwd(),
            code_linux_sandbox_exe: &self.code_linux_sandbox_exe,
            stdout_stream: None,
        };

        if let Err(err) = self
            .run_exec_with_events(turn_diff_tracker, exec_ctx, exec_args, None, None, attempt_req)
            .await
        {
            let order = self.next_background_order(sub_id, attempt_req, None);
            self
                .notify_background_event_with_order(
                    sub_id,
                    order,
                    format!(
                        "Project command `{}` failed: {}",
                        command.name,
                        get_error_message_ui(&err)
                    ),
                )
                .await;
        }
    }
}
