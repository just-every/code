use chrono::Local;
use os_info::Type as OsType;
use os_info::Version;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use sha1::{Digest, Sha1};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use strum_macros::Display as DeriveDisplay;
use which::which;

use crate::protocol::AskForApproval;
use crate::protocol::SandboxPolicy;
use crate::shell::Shell;
use code_protocol::config_types::SandboxMode;
use code_protocol::models::ContentItem;
use code_protocol::models::ResponseItem;
use code_protocol::protocol::BROWSER_SNAPSHOT_CLOSE_TAG;
use code_protocol::protocol::BROWSER_SNAPSHOT_OPEN_TAG;
use code_protocol::protocol::ENVIRONMENT_CONTEXT_CLOSE_TAG;
use code_protocol::protocol::ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG;
use code_protocol::protocol::ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG;
use code_protocol::protocol::ENVIRONMENT_CONTEXT_OPEN_TAG;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, DeriveDisplay)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum NetworkAccess {
    Restricted,
    Enabled,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename = "environment_context", rename_all = "snake_case")]
pub(crate) struct EnvironmentContext {
    pub cwd: Option<PathBuf>,
    pub approval_policy: Option<AskForApproval>,
    pub sandbox_mode: Option<SandboxMode>,
    pub network_access: Option<NetworkAccess>,
    pub writable_roots: Option<Vec<PathBuf>>,
    pub operating_system: Option<OperatingSystemInfo>,
    pub common_tools: Option<Vec<String>>,
    pub shell: Option<Shell>,
    pub current_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OperatingSystemInfo {
    pub family: Option<String>,
    pub version: Option<String>,
    pub architecture: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolCandidate {
    pub label: &'static str,
    pub detection_names: &'static [&'static str],
}

pub const TOOL_CANDIDATES: &[ToolCandidate] = &[
    ToolCandidate { label: "git", detection_names: &["git"] },
    ToolCandidate { label: "gh", detection_names: &["gh"] },
    ToolCandidate { label: "rg", detection_names: &["rg"] },
    ToolCandidate { label: "fd", detection_names: &["fd", "fdfind"] },
    ToolCandidate { label: "fzf", detection_names: &["fzf"] },
    ToolCandidate { label: "jq", detection_names: &["jq"] },
    ToolCandidate { label: "yq", detection_names: &["yq"] },
    ToolCandidate { label: "sed", detection_names: &["sed"] },
    ToolCandidate { label: "awk", detection_names: &["awk"] },
    ToolCandidate { label: "xargs", detection_names: &["xargs"] },
    ToolCandidate { label: "parallel", detection_names: &["parallel"] },
    ToolCandidate { label: "curl", detection_names: &["curl"] },
    ToolCandidate { label: "wget", detection_names: &["wget"] },
    ToolCandidate { label: "tar", detection_names: &["tar"] },
    ToolCandidate { label: "unzip", detection_names: &["unzip"] },
    ToolCandidate { label: "gzip", detection_names: &["gzip"] },
    ToolCandidate { label: "zstd", detection_names: &["zstd"] },
    ToolCandidate { label: "make", detection_names: &["make"] },
    ToolCandidate { label: "just", detection_names: &["just"] },
    ToolCandidate { label: "node", detection_names: &["node"] },
    ToolCandidate { label: "npm", detection_names: &["npm"] },
    ToolCandidate { label: "pnpm", detection_names: &["pnpm"] },
    ToolCandidate { label: "python3", detection_names: &["python3"] },
    ToolCandidate { label: "pipx", detection_names: &["pipx"] },
    ToolCandidate { label: "go", detection_names: &["go"] },
    ToolCandidate { label: "rustup", detection_names: &["rustup"] },
    ToolCandidate { label: "cargo", detection_names: &["cargo"] },
    ToolCandidate { label: "rustc", detection_names: &["rustc"] },
    ToolCandidate { label: "shellcheck", detection_names: &["shellcheck"] },
    ToolCandidate { label: "shfmt", detection_names: &["shfmt"] },
    ToolCandidate { label: "docker", detection_names: &["docker"] },
    ToolCandidate { label: "docker compose", detection_names: &["docker", "docker-compose"] },
    ToolCandidate { label: "sqlite3", detection_names: &["sqlite3"] },
    ToolCandidate { label: "duckdb", detection_names: &["duckdb"] },
    ToolCandidate { label: "rsync", detection_names: &["rsync"] },
    ToolCandidate { label: "openssl", detection_names: &["openssl"] },
    ToolCandidate { label: "ssh", detection_names: &["ssh"] },
    ToolCandidate { label: "dig", detection_names: &["dig"] },
    ToolCandidate { label: "nc", detection_names: &["nc", "netcat"] },
    ToolCandidate { label: "lsof", detection_names: &["lsof"] },
    ToolCandidate { label: "ripgrep-all", detection_names: &["ripgrep-all", "rga"] },
    ToolCandidate { label: "entr", detection_names: &["entr"] },
    ToolCandidate { label: "watchexec", detection_names: &["watchexec"] },
    ToolCandidate { label: "hyperfine", detection_names: &["hyperfine"] },
    ToolCandidate { label: "pv", detection_names: &["pv"] },
    ToolCandidate { label: "bat", detection_names: &["bat"] },
    ToolCandidate { label: "delta", detection_names: &["delta"] },
    ToolCandidate { label: "tree", detection_names: &["tree"] },
    ToolCandidate { label: "python", detection_names: &["python"] },
    ToolCandidate { label: "deno", detection_names: &["deno"] },
    ToolCandidate { label: "bun", detection_names: &["bun"] },
    ToolCandidate { label: "js", detection_names: &["js"] },
];

impl EnvironmentContext {
    pub fn new(
        cwd: Option<PathBuf>,
        approval_policy: Option<AskForApproval>,
        sandbox_policy: Option<SandboxPolicy>,
        shell: Option<Shell>,
    ) -> Self {
        Self {
            cwd,
            approval_policy,
            sandbox_mode: match sandbox_policy {
                Some(SandboxPolicy::DangerFullAccess) => Some(SandboxMode::DangerFullAccess),
                Some(SandboxPolicy::ReadOnly) => Some(SandboxMode::ReadOnly),
                Some(SandboxPolicy::WorkspaceWrite { .. }) => Some(SandboxMode::WorkspaceWrite),
                None => None,
            },
            network_access: match sandbox_policy {
                Some(SandboxPolicy::DangerFullAccess) => Some(NetworkAccess::Enabled),
                Some(SandboxPolicy::ReadOnly) => Some(NetworkAccess::Restricted),
                Some(SandboxPolicy::WorkspaceWrite { network_access, .. }) => {
                    if network_access {
                        Some(NetworkAccess::Enabled)
                    } else {
                        Some(NetworkAccess::Restricted)
                    }
                }
                None => None,
            },
            writable_roots: match sandbox_policy {
                Some(SandboxPolicy::WorkspaceWrite { writable_roots, .. }) => {
                    if writable_roots.is_empty() {
                        None
                    } else {
                        Some(writable_roots.clone())
                    }
                }
                _ => None,
            },
            operating_system: detect_operating_system_info(),
            common_tools: detect_common_tools(),
            shell,
            current_date: Some(Local::now().format("%Y-%m-%d").to_string()),
        }
    }

    /// Compares two environment contexts, ignoring the shell. Useful when
    /// comparing turn to turn, since the initial environment_context will
    /// include the shell, and then it is not configurable from turn to turn.
    #[cfg(test)]
    pub fn equals_except_shell(&self, other: &EnvironmentContext) -> bool {
        let EnvironmentContext {
            cwd,
            approval_policy,
            sandbox_mode,
            network_access,
            writable_roots,
            operating_system,
            common_tools,
            current_date,
            // should compare all fields except shell
            shell: _,
        } = other;

        self.cwd == *cwd
            && self.approval_policy == *approval_policy
            && self.sandbox_mode == *sandbox_mode
            && self.network_access == *network_access
            && self.writable_roots == *writable_roots
            && self.operating_system == *operating_system
            && self.common_tools == *common_tools
            && self.current_date == *current_date
    }
}

// Note: The core no longer exposes `TurnContext` here; callers construct
// `EnvironmentContext` directly via `EnvironmentContext::new(...)`.

impl EnvironmentContext {
    /// Serializes the environment context to XML. Libraries like `quick-xml`
    /// require custom macros to handle Enums with newtypes, so we just do it
    /// manually, to keep things simple. Output looks like:
    ///
    /// ```xml
    /// <environment_context>
    ///   <cwd>...</cwd>
    ///   <approval_policy>...</approval_policy>
    ///   <sandbox_mode>...</sandbox_mode>
    ///   <writable_roots>...</writable_roots>
    ///   <network_access>...</network_access>
    ///   <operating_system>
    ///     <family>...</family>
    ///     <version>...</version>
    ///     <architecture>...</architecture>
    ///   </operating_system>
    ///   <common_tools>...</common_tools>
    ///   <shell>...</shell>
    /// </environment_context>
    /// ```
    pub fn serialize_to_xml(self) -> String {
        let mut lines = vec![ENVIRONMENT_CONTEXT_OPEN_TAG.to_string()];
        if let Some(cwd) = self.cwd {
            lines.push(format!("  <cwd>{}</cwd>", cwd.to_string_lossy()));
        }
        if let Some(approval_policy) = self.approval_policy {
            lines.push(format!(
                "  <approval_policy>{approval_policy}</approval_policy>"
            ));
        }
        if let Some(sandbox_mode) = self.sandbox_mode {
            lines.push(format!("  <sandbox_mode>{sandbox_mode}</sandbox_mode>"));
        }
        if let Some(network_access) = self.network_access {
            lines.push(format!(
                "  <network_access>{network_access}</network_access>"
            ));
        }
        if let Some(writable_roots) = self.writable_roots {
            lines.push("  <writable_roots>".to_string());
            for writable_root in writable_roots {
                lines.push(format!(
                    "    <root>{}</root>",
                    writable_root.to_string_lossy()
                ));
            }
            lines.push("  </writable_roots>".to_string());
        }
        if let Some(operating_system) = self.operating_system {
            lines.push("  <operating_system>".to_string());
            if let Some(family) = operating_system.family {
                lines.push(format!("    <family>{family}</family>"));
            }
            if let Some(version) = operating_system.version {
                lines.push(format!("    <version>{version}</version>"));
            }
            if let Some(architecture) = operating_system.architecture {
                lines.push(format!("    <architecture>{architecture}</architecture>"));
            }
            lines.push("  </operating_system>".to_string());
        }
        if let Some(common_tools) = self.common_tools {
            if !common_tools.is_empty() {
                lines.push("  <common_tools>".to_string());
                for tool in common_tools {
                    lines.push(format!("    <tool>{tool}</tool>"));
                }
                lines.push("  </common_tools>".to_string());
            }
        }
        if let Some(current_date) = self.current_date {
            lines.push(format!("  <current_date>{current_date}</current_date>"));
        }
        if let Some(shell) = self.shell
            && let Some(shell_name) = shell.name()
        {
            lines.push(format!("  <shell>{shell_name}</shell>"));
        }
        lines.push(ENVIRONMENT_CONTEXT_CLOSE_TAG.to_string());
        lines.join("\n")
    }
}

impl From<EnvironmentContext> for ResponseItem {
    fn from(ec: EnvironmentContext) -> Self {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: ec.serialize_to_xml(),
            }],
        }
    }
}

/// Canonical snapshot of the environment context that is safe to serialize as JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnvironmentContextSnapshot {
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<AskForApproval>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_mode: Option<SandboxMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_access: Option<NetworkAccess>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub writable_roots: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operating_system: Option<OperatingSystemInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub common_tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<Shell>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

impl EnvironmentContextSnapshot {
    pub const VERSION: u32 = 1;

    pub(crate) fn from_context(ctx: &EnvironmentContext) -> Self {
        Self {
            version: Self::VERSION,
            cwd: ctx.cwd.as_ref().map(|p| p.display().to_string()),
            approval_policy: ctx.approval_policy.clone(),
            sandbox_mode: ctx.sandbox_mode.clone(),
            network_access: ctx.network_access.clone(),
            writable_roots: ctx
                .writable_roots
                .as_ref()
                .map(|roots| roots.iter().map(|p| p.display().to_string()).collect())
                .unwrap_or_default(),
            operating_system: ctx.operating_system.clone(),
            common_tools: ctx.common_tools.clone().unwrap_or_default(),
            shell: ctx.shell.clone(),
            git_branch: None,
            reasoning_effort: None,
        }
    }

    /// Compute a stable fingerprint that ignores volatile fields such as OS info and shell.
    pub fn fingerprint(&self) -> String {
        let mut map = BTreeMap::new();
        if let Some(cwd) = &self.cwd {
            map.insert("cwd", cwd.clone());
        }
        if let Some(policy) = &self.approval_policy {
            map.insert("approval_policy", format!("{policy}"));
        }
        if let Some(mode) = &self.sandbox_mode {
            map.insert("sandbox_mode", format!("{mode}"));
        }
        if let Some(access) = &self.network_access {
            map.insert("network_access", format!("{access}"));
        }
        if !self.writable_roots.is_empty() {
            map.insert("writable_roots", self.writable_roots.join("|"));
        }
        if let Some(branch) = &self.git_branch {
            map.insert("git_branch", branch.clone());
        }
        if let Some(reasoning) = &self.reasoning_effort {
            map.insert("reasoning_effort", reasoning.clone());
        }

        let encoded = serde_json::to_vec(&map).expect("serializing fingerprint fields");
        let mut sha = Sha1::new();
        sha.update(encoded);
        format!("{:x}", sha.finalize())
    }

    pub fn diff_from(&self, previous: &EnvironmentContextSnapshot) -> EnvironmentContextDelta {
        let mut changes = BTreeMap::new();

        if self.cwd != previous.cwd {
            changes.insert("cwd".to_string(), option_string_to_json(&self.cwd));
        }
        if self.approval_policy != previous.approval_policy {
            changes.insert(
                "approval_policy".to_string(),
                serde_json::to_value(&self.approval_policy).unwrap_or(JsonValue::Null),
            );
        }
        if self.sandbox_mode != previous.sandbox_mode {
            changes.insert(
                "sandbox_mode".to_string(),
                serde_json::to_value(&self.sandbox_mode).unwrap_or(JsonValue::Null),
            );
        }
        if self.network_access != previous.network_access {
            changes.insert(
                "network_access".to_string(),
                serde_json::to_value(&self.network_access).unwrap_or(JsonValue::Null),
            );
        }
        if self.writable_roots != previous.writable_roots {
            changes.insert(
                "writable_roots".to_string(),
                JsonValue::Array(self.writable_roots.iter().map(|s| JsonValue::String(s.clone())).collect()),
            );
        }
        if self.operating_system != previous.operating_system {
            changes.insert(
                "operating_system".to_string(),
                serde_json::to_value(&self.operating_system).unwrap_or(JsonValue::Null),
            );
        }
        if self.common_tools != previous.common_tools {
            changes.insert(
                "common_tools".to_string(),
                JsonValue::Array(self.common_tools.iter().map(|s| JsonValue::String(s.clone())).collect()),
            );
        }
        if self.shell != previous.shell {
            changes.insert(
                "shell".to_string(),
                serde_json::to_value(&self.shell).unwrap_or(JsonValue::Null),
            );
        }
        if self.git_branch != previous.git_branch {
            changes.insert(
                "git_branch".to_string(),
                option_string_to_json(&self.git_branch),
            );
        }
        if self.reasoning_effort != previous.reasoning_effort {
            changes.insert(
                "reasoning_effort".to_string(),
                option_string_to_json(&self.reasoning_effort),
            );
        }

        EnvironmentContextDelta {
            version: Self::VERSION,
            base_fingerprint: previous.fingerprint(),
            changes,
        }
    }

    pub fn to_response_item(&self) -> serde_json::Result<ResponseItem> {
        self.to_response_item_with_id(None)
    }

    pub fn to_response_item_with_id(
        &self,
        stream_id: Option<&str>,
    ) -> serde_json::Result<ResponseItem> {
        snapshot_to_response_item(self, stream_id)
    }

    pub fn with_metadata(mut self, git_branch: Option<String>, reasoning_effort: Option<String>) -> Self {
        self.git_branch = git_branch;
        self.reasoning_effort = reasoning_effort;
        self
    }

    /// Applies a delta to this snapshot, producing an updated snapshot.
    pub fn apply_delta(&self, delta: &EnvironmentContextDelta) -> EnvironmentContextSnapshot {
        let mut updated = self.clone();
        for (key, value) in &delta.changes {
            match key.as_str() {
                "cwd" => {
                    updated.cwd = value.as_str().map(|s| s.to_string());
                }
                "approval_policy" => {
                    if let Ok(policy) = serde_json::from_value::<AskForApproval>(value.clone()) {
                        updated.approval_policy = Some(policy);
                    }
                }
                "sandbox_mode" => {
                    if let Ok(mode) = serde_json::from_value::<SandboxMode>(value.clone()) {
                        updated.sandbox_mode = Some(mode);
                    }
                }
                "network_access" => {
                    if let Ok(access) = serde_json::from_value::<NetworkAccess>(value.clone()) {
                        updated.network_access = Some(access);
                    }
                }
                "writable_roots" => {
                    if let Ok(roots) = serde_json::from_value::<Vec<String>>(value.clone()) {
                        updated.writable_roots = roots;
                    }
                }
                "operating_system" => {
                    if let Ok(os) = serde_json::from_value::<OperatingSystemInfo>(value.clone()) {
                        updated.operating_system = Some(os);
                    }
                }
                "common_tools" => {
                    if let Ok(tools) = serde_json::from_value::<Vec<String>>(value.clone()) {
                        updated.common_tools = tools;
                    }
                }
                "shell" => {
                    if let Ok(shell) = serde_json::from_value::<Shell>(value.clone()) {
                        updated.shell = Some(shell);
                    }
                }
                "git_branch" => {
                    updated.git_branch = match value {
                        JsonValue::Null => None,
                        JsonValue::String(s) => Some(s.clone()),
                        _ => updated.git_branch.clone(),
                    };
                }
                "reasoning_effort" => {
                    updated.reasoning_effort = match value {
                        JsonValue::Null => None,
                        JsonValue::String(s) => Some(s.clone()),
                        _ => updated.reasoning_effort.clone(),
                    };
                }
                _ => {}
            }
        }
        updated
    }
}

impl From<&EnvironmentContext> for EnvironmentContextSnapshot {
    fn from(ctx: &EnvironmentContext) -> Self {
        EnvironmentContextSnapshot::from_context(ctx)
    }
}

impl From<EnvironmentContext> for EnvironmentContextSnapshot {
    fn from(ctx: EnvironmentContext) -> Self {
        EnvironmentContextSnapshot::from_context(&ctx)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnvironmentContextDelta {
    pub version: u32,
    pub base_fingerprint: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub changes: BTreeMap<String, JsonValue>,
}

impl EnvironmentContextDelta {
    pub fn to_response_item(&self) -> serde_json::Result<ResponseItem> {
        self.to_response_item_with_id(None)
    }

    pub fn to_response_item_with_id(
        &self,
        stream_id: Option<&str>,
    ) -> serde_json::Result<ResponseItem> {
        delta_to_response_item(self, stream_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrowserSnapshot {
    pub version: u32,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub captured_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewport: Option<ViewportDimensions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

impl BrowserSnapshot {
    pub const VERSION: u32 = 1;

    pub fn new(url: String, captured_at: String) -> Self {
        Self {
            version: Self::VERSION,
            url,
            title: None,
            captured_at,
            viewport: None,
            metadata: None,
        }
    }

    pub fn to_response_item(&self) -> serde_json::Result<ResponseItem> {
        self.to_response_item_with_id(None)
    }

    pub fn to_response_item_with_id(
        &self,
        stream_id: Option<&str>,
    ) -> serde_json::Result<ResponseItem> {
        browser_snapshot_to_response_item(self, stream_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ViewportDimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Default, Clone)]
pub struct EnvironmentContextTracker {
    next_sequence: u64,
    last_snapshot: Option<EnvironmentContextSnapshot>,
}

impl EnvironmentContextTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(
        &mut self,
        snapshot: EnvironmentContextSnapshot,
    ) -> Option<EnvironmentContextEmission> {
        match &self.last_snapshot {
            None => {
                self.next_sequence += 1;
                self.last_snapshot = Some(snapshot.clone());
                Some(EnvironmentContextEmission::Full {
                    sequence: self.next_sequence,
                    snapshot,
                })
            }
            Some(previous) => {
                if snapshot.fingerprint() == previous.fingerprint() {
                    None
                } else {
                    self.next_sequence += 1;
                    let delta = snapshot.diff_from(previous);
                    self.last_snapshot = Some(snapshot.clone());
                    Some(EnvironmentContextEmission::Delta {
                        sequence: self.next_sequence,
                        snapshot,
                        delta,
                    })
                }
            }
        }
    }

    pub fn last_snapshot(&self) -> Option<&EnvironmentContextSnapshot> {
        self.last_snapshot.as_ref()
    }

    pub(crate) fn emit_response_items(
        &mut self,
        env_context: &EnvironmentContext,
        git_branch: Option<String>,
        reasoning_effort: Option<String>,
        stream_id: Option<&str>,
    ) -> serde_json::Result<Option<(EnvironmentContextEmission, Vec<ResponseItem>)>> {
        let emission = match self.observe(
            EnvironmentContextSnapshot::from_context(env_context)
                .with_metadata(git_branch, reasoning_effort),
        ) {
            Some(emission) => emission,
            None => return Ok(None),
        };

        let items = emission
            .clone()
            .into_response_items_with_id(stream_id)?;
        Ok(Some((emission, items)))
    }

    /// Restores tracker state so the next emission continues at the provided sequence.
    pub fn restore(&mut self, last_snapshot: EnvironmentContextSnapshot, next_sequence: u64) {
        self.last_snapshot = Some(last_snapshot);
        self.next_sequence = next_sequence.max(1);
    }
}

#[derive(Debug, Clone)]
pub enum EnvironmentContextEmission {
    Full {
        sequence: u64,
        snapshot: EnvironmentContextSnapshot,
    },
    Delta {
        sequence: u64,
        snapshot: EnvironmentContextSnapshot,
        delta: EnvironmentContextDelta,
    },
}

impl EnvironmentContextEmission {
    pub fn sequence(&self) -> u64 {
        match self {
            EnvironmentContextEmission::Full { sequence, .. }
            | EnvironmentContextEmission::Delta { sequence, .. } => *sequence,
        }
    }

    pub fn into_response_items(self) -> serde_json::Result<Vec<ResponseItem>> {
        self.into_response_items_with_id(None)
    }

    pub fn into_response_items_with_id(
        self,
        stream_id: Option<&str>,
    ) -> serde_json::Result<Vec<ResponseItem>> {
        match self {
            EnvironmentContextEmission::Full { snapshot, .. } => Ok(vec![
                snapshot.to_response_item_with_id(stream_id)?,
            ]),
            EnvironmentContextEmission::Delta { delta, .. } => Ok(vec![
                delta.to_response_item_with_id(stream_id)?,
            ]),
        }
    }

    pub fn snapshot(&self) -> &EnvironmentContextSnapshot {
        match self {
            EnvironmentContextEmission::Full { snapshot, .. }
            | EnvironmentContextEmission::Delta { snapshot, .. } => snapshot,
        }
    }
}

fn option_string_to_json(value: &Option<String>) -> JsonValue {
    match value {
        Some(v) => JsonValue::String(v.clone()),
        None => JsonValue::Null,
    }
}

fn snapshot_to_response_item(
    snapshot: &EnvironmentContextSnapshot,
    stream_id: Option<&str>,
) -> serde_json::Result<ResponseItem> {
    let json = serde_json::to_string_pretty(snapshot)?;
    Ok(ResponseItem::Message {
        id: stream_id.map(|id| id.to_string()),
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: format!(
                "{}\n{}\n{}",
                ENVIRONMENT_CONTEXT_OPEN_TAG, json, ENVIRONMENT_CONTEXT_CLOSE_TAG
            ),
        }],
    })
}

fn delta_to_response_item(
    delta: &EnvironmentContextDelta,
    stream_id: Option<&str>,
) -> serde_json::Result<ResponseItem> {
    let json = serde_json::to_string_pretty(delta)?;
    Ok(ResponseItem::Message {
        id: stream_id.map(|id| id.to_string()),
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: format!(
                "{}\n{}\n{}",
                ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG, json, ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG
            ),
        }],
    })
}

fn browser_snapshot_to_response_item(
    snapshot: &BrowserSnapshot,
    stream_id: Option<&str>,
) -> serde_json::Result<ResponseItem> {
    let json = serde_json::to_string_pretty(snapshot)?;
    Ok(ResponseItem::Message {
        id: stream_id.map(|id| id.to_string()),
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: format!(
                "{}\n{}\n{}",
                BROWSER_SNAPSHOT_OPEN_TAG, json, BROWSER_SNAPSHOT_CLOSE_TAG
            ),
        }],
    })
}

fn detect_operating_system_info() -> Option<OperatingSystemInfo> {
    let info = os_info::get();
    let family = match info.os_type() {
        OsType::Unknown => None,
        other => Some(other.to_string()),
    };
    let version = match info.version() {
        Version::Unknown => None,
        other => {
            let text = other.to_string();
            if text.trim().is_empty() {
                None
            } else {
                Some(text)
            }
        }
    };
    let architecture = {
        let arch = std::env::consts::ARCH;
        if arch.is_empty() {
            None
        } else {
            Some(arch.to_string())
        }
    };

    if family.is_none() && version.is_none() && architecture.is_none() {
        return None;
    }

    Some(OperatingSystemInfo {
        family,
        version,
        architecture,
    })
}

fn detect_common_tools() -> Option<Vec<String>> {
    let mut available = Vec::new();
    for candidate in TOOL_CANDIDATES {
        let detection_names = if candidate.detection_names.is_empty() {
            &[candidate.label][..]
        } else {
            candidate.detection_names
        };

        if detection_names
            .iter()
            .any(|name| which(name).is_ok())
        {
            available.push(candidate.label.to_string());
        }
    }

    if available.is_empty() {
        None
    } else {
        Some(available)
    }
}

#[cfg(test)]
mod tests {
    use crate::shell::BashShell;
    use crate::shell::ZshShell;

    use super::*;
    use pretty_assertions::assert_eq;

    fn workspace_write_policy(writable_roots: Vec<&str>, network_access: bool) -> SandboxPolicy {
        SandboxPolicy::WorkspaceWrite {
            writable_roots: writable_roots.into_iter().map(PathBuf::from).collect(),
            network_access,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
            allow_git_writes: true,
        }
    }

    #[test]
    fn serialize_workspace_write_environment_context() {
        let mut context = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(workspace_write_policy(vec!["/repo", "/tmp"], false)),
            None,
        );
        context.operating_system = None;
        context.common_tools = None;
        context.current_date = Some("2025-01-02".to_string());

        let expected = r#"<environment_context>
  <cwd>/repo</cwd>
  <approval_policy>on-request</approval_policy>
  <sandbox_mode>workspace-write</sandbox_mode>
  <network_access>restricted</network_access>
  <writable_roots>
    <root>/repo</root>
    <root>/tmp</root>
  </writable_roots>
  <current_date>2025-01-02</current_date>
</environment_context>"#;

        assert_eq!(context.serialize_to_xml(), expected);
    }

    #[test]
    fn serialize_read_only_environment_context() {
        let mut context = EnvironmentContext::new(
            None,
            Some(AskForApproval::Never),
            Some(SandboxPolicy::ReadOnly),
            None,
        );
        context.operating_system = None;
        context.common_tools = None;
        context.current_date = Some("2025-01-02".to_string());

        let expected = r#"<environment_context>
  <approval_policy>never</approval_policy>
  <sandbox_mode>read-only</sandbox_mode>
  <network_access>restricted</network_access>
  <current_date>2025-01-02</current_date>
</environment_context>"#;

        assert_eq!(context.serialize_to_xml(), expected);
    }

    #[test]
    fn serialize_full_access_environment_context() {
        let mut context = EnvironmentContext::new(
            None,
            Some(AskForApproval::OnFailure),
            Some(SandboxPolicy::DangerFullAccess),
            None,
        );
        context.operating_system = None;
        context.common_tools = None;
        context.current_date = Some("2025-01-02".to_string());

        let expected = r#"<environment_context>
  <approval_policy>on-failure</approval_policy>
  <sandbox_mode>danger-full-access</sandbox_mode>
  <network_access>enabled</network_access>
  <current_date>2025-01-02</current_date>
</environment_context>"#;

        assert_eq!(context.serialize_to_xml(), expected);
    }

    #[test]
    fn serialize_environment_context_includes_os_and_tools() {
        let mut context = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(workspace_write_policy(vec!["/repo"], true)),
            None,
        );
        context.operating_system = Some(OperatingSystemInfo {
            family: Some("macos".to_string()),
            version: Some("14.0".to_string()),
            architecture: Some("aarch64".to_string()),
        });
        context.common_tools = Some(vec!["rg".to_string(), "git".to_string()]);
        context.current_date = Some("2025-01-02".to_string());

        let xml = context.serialize_to_xml();
        assert!(xml.contains("<operating_system>"));
        assert!(xml.contains("<family>macos</family>"));
        assert!(xml.contains("<version>14.0</version>"));
        assert!(xml.contains("<architecture>aarch64</architecture>"));
        assert!(xml.contains("<common_tools>"));
        assert!(xml.contains("<tool>rg</tool>"));
        assert!(xml.contains("<tool>git</tool>"));
        assert!(xml.contains("<current_date>2025-01-02</current_date>"));
    }

    fn message_text(item: &ResponseItem) -> &str {
        match item {
            ResponseItem::Message { content, .. } => match &content[0] {
                ContentItem::InputText { text } => text,
                other => panic!("unexpected content item: {other:?}"),
            },
            other => panic!("unexpected response item: {other:?}"),
        }
    }

    fn parse_tagged_json(text: &str, open: &str, close: &str) -> serde_json::Value {
        let trimmed = text.trim();
        assert!(trimmed.starts_with(open), "expected prefix {open}");
        assert!(trimmed.ends_with(close), "expected suffix {close}");
        let inner = &trimmed[open.len()..trimmed.len() - close.len()];
        serde_json::from_str(inner.trim()).expect("valid tagged json")
    }

    #[test]
    fn tracker_emits_full_and_delta_messages() {
        let mut tracker = EnvironmentContextTracker::new();
        let mut ctx = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(SandboxPolicy::DangerFullAccess),
            None,
        );

        let first = tracker
            .emit_response_items(&ctx, Some("main".into()), Some("Medium".into()), Some("env-stream"))
            .expect("serialize full")
            .expect("full emission");
        assert!(matches!(first.0, EnvironmentContextEmission::Full { .. }));
        assert_eq!(first.0.sequence(), 1);
        let full_text = message_text(&first.1[0]);
        assert_eq!(message_id(&first.1[0]), Some("env-stream"));
        assert!(full_text.contains(ENVIRONMENT_CONTEXT_OPEN_TAG));
        assert!(!full_text.contains("== System Status =="));
        let full_json = parse_tagged_json(
            full_text,
            ENVIRONMENT_CONTEXT_OPEN_TAG,
            ENVIRONMENT_CONTEXT_CLOSE_TAG,
        );
        assert_eq!(full_json["git_branch"], "main");

        // Unchanged context should not emit again
        let none = tracker
            .emit_response_items(&ctx, Some("main".into()), Some("Medium".into()), Some("env-stream"))
            .expect("unchanged serialize");
        assert!(none.is_none());

        // Changing stable fields triggers a delta emission
        ctx.cwd = Some(PathBuf::from("/repo-two"));
        let delta = tracker
            .emit_response_items(&ctx, Some("feature".into()), Some("High".into()), Some("env-stream"))
            .expect("serialize delta")
            .expect("delta emission");
        assert!(matches!(delta.0, EnvironmentContextEmission::Delta { .. }));
        assert_eq!(delta.0.sequence(), 2);
        let delta_text = message_text(&delta.1[0]);
        assert_eq!(message_id(&delta.1[0]), Some("env-stream"));
        assert!(delta_text.contains(ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG));
        let delta_json = parse_tagged_json(
            delta_text,
            ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG,
            ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG,
        );
        assert_eq!(delta_json["changes"]["cwd"], "/repo-two");
        assert_eq!(delta_json["changes"]["git_branch"], "feature");
        assert_eq!(delta_json["changes"]["reasoning_effort"], "High");
    }

    fn message_id(item: &ResponseItem) -> Option<&str> {
        match item {
            ResponseItem::Message { id, .. } => id.as_deref(),
            _ => None,
        }
    }

    #[test]
    fn equals_except_shell_compares_approval_policy() {
        // Approval policy
        let context1 = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(workspace_write_policy(vec!["/repo"], false)),
            None,
        );
        let context2 = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::Never),
            Some(workspace_write_policy(vec!["/repo"], true)),
            None,
        );
        // ensure current_date doesn't influence this comparison
        let fixed_date = Some("2025-01-02".to_string());
        let mut context1 = context1;
        context1.current_date = fixed_date.clone();
        let mut context2 = context2;
        context2.current_date = fixed_date;
        assert!(!context1.equals_except_shell(&context2));
    }

    #[test]
    fn equals_except_shell_compares_sandbox_policy() {
        let context1 = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(SandboxPolicy::new_read_only_policy()),
            None,
        );
        let context2 = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(SandboxPolicy::new_workspace_write_policy()),
            None,
        );
        let mut context1 = context1;
        context1.current_date = Some("2025-01-02".to_string());
        let mut context2 = context2;
        context2.current_date = Some("2025-01-02".to_string());

        assert!(!context1.equals_except_shell(&context2));
    }

    #[test]
    fn equals_except_shell_compares_workspace_write_policy() {
        let context1 = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(workspace_write_policy(vec!["/repo", "/tmp", "/var"], false)),
            None,
        );
        let context2 = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(workspace_write_policy(vec!["/repo", "/tmp"], true)),
            None,
        );
        let mut context1 = context1;
        context1.current_date = Some("2025-01-02".to_string());
        let mut context2 = context2;
        context2.current_date = Some("2025-01-02".to_string());

        assert!(!context1.equals_except_shell(&context2));
    }

    #[test]
    fn browser_snapshot_respects_stream_id() {
        let mut snapshot = BrowserSnapshot::new(
            "https://example.test".to_string(),
            "2025-11-04T00:00:00Z".to_string(),
        );
        snapshot.title = Some("Example".to_string());

        let message = snapshot
            .to_response_item_with_id(Some("browser-stream"))
            .expect("serialize snapshot");

        assert_eq!(message_id(&message), Some("browser-stream"));
        if let ResponseItem::Message { content, .. } = message {
            if let ContentItem::InputText { text } = &content[0] {
                assert!(text.contains(BROWSER_SNAPSHOT_OPEN_TAG));
                assert!(text.contains("Example"));
            } else {
                panic!("expected text content");
            }
        } else {
            panic!("expected message response item");
        }
    }

    #[test]
    fn equals_except_shell_ignores_shell() {
        let context1 = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(workspace_write_policy(vec!["/repo"], false)),
            Some(Shell::Bash(BashShell {
                shell_path: "/bin/bash".into(),
                bashrc_path: "/home/user/.bashrc".into(),
            })),
        );
        let context2 = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(workspace_write_policy(vec!["/repo"], false)),
            Some(Shell::Zsh(ZshShell {
                shell_path: "/bin/zsh".into(),
                zshrc_path: "/home/user/.zshrc".into(),
            })),
        );
        let mut context1 = context1;
        context1.current_date = Some("2025-01-02".to_string());
        let mut context2 = context2;
        context2.current_date = Some("2025-01-02".to_string());

        assert!(context1.equals_except_shell(&context2));
    }

    #[test]
    fn snapshot_fingerprint_ignores_volatile_fields() {
        let mut ctx_a = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(workspace_write_policy(vec!["/repo"], true)),
            None,
        );
        ctx_a.operating_system = Some(OperatingSystemInfo {
            family: Some("macos".into()),
            version: Some("14.0".into()),
            architecture: Some("arm64".into()),
        });
        ctx_a.common_tools = Some(vec!["git".into(), "rg".into()]);

        let mut ctx_b = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(workspace_write_policy(vec!["/repo"], true)),
            None,
        );
        ctx_b.operating_system = Some(OperatingSystemInfo {
            family: Some("ubuntu".into()),
            version: Some("22.04".into()),
            architecture: Some("x86_64".into()),
        });
        ctx_b.common_tools = Some(vec!["npm".into(), "node".into()]);

        let snap_a = EnvironmentContextSnapshot::from(&ctx_a);
        let snap_b = EnvironmentContextSnapshot::from(&ctx_b);

        assert_eq!(snap_a.fingerprint(), snap_b.fingerprint());
    }

    #[test]
    fn snapshot_diff_detects_changes() {
        let ctx_a = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(workspace_write_policy(vec!["/repo"], false)),
            None,
        );
        let ctx_b = EnvironmentContext::new(
            Some(PathBuf::from("/other")),
            Some(AskForApproval::Never),
            Some(workspace_write_policy(vec!["/repo"], true)),
            None,
        );

        let snap_a = EnvironmentContextSnapshot::from(&ctx_a);
        let snap_b = EnvironmentContextSnapshot::from(&ctx_b);
        let delta = snap_b.diff_from(&snap_a);

        assert!(delta.changes.contains_key("cwd"));
        assert!(delta.changes.contains_key("approval_policy"));
        assert!(delta.changes.contains_key("network_access"));
        assert_eq!(delta.base_fingerprint, snap_a.fingerprint());
    }

    #[test]
    fn tracker_emits_full_then_none_when_unchanged() {
        let ctx = EnvironmentContext::new(
            Some(PathBuf::from("/repo")),
            Some(AskForApproval::OnRequest),
            Some(workspace_write_policy(vec!["/repo"], false)),
            None,
        );
        let snapshot = EnvironmentContextSnapshot::from(&ctx);

        let mut tracker = EnvironmentContextTracker::new();

        let first = tracker
            .observe(snapshot.clone())
            .expect("first snapshot should emit");
        assert!(matches!(first, EnvironmentContextEmission::Full { .. }));

        let second = tracker.observe(snapshot);
        assert!(second.is_none(), "unchanged snapshot should not emit");
    }

    #[test]
    fn serialize_environment_context_includes_current_date() {
        let mut context = EnvironmentContext::new(None, None, None, None);
        context.current_date = Some("2025-01-02".to_string());

        let xml = context.serialize_to_xml();
        assert!(xml.contains("<current_date>2025-01-02</current_date>"));
    }

    #[test]
    fn current_date_format_is_iso8601() {
        let context = EnvironmentContext::new(None, None, None, None);
        let date = context
            .current_date
            .expect("current_date should be populated");
        assert_eq!(date.len(), 10);
        assert_eq!(date.chars().nth(4), Some('-'));
        assert_eq!(date.chars().nth(7), Some('-'));
    }
}
