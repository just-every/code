#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    Model,
    Theme,
    Updates,
    Accounts,
    Agents,
    Prompts,
    Skills,
    AutoDrive,
    Review,
    Planning,
    Validation,
    Limits,
    Chrome,
    Mcp,
    Notifications,
}

impl SettingsSection {
    pub(crate) const ALL: [SettingsSection; 15] = [
        SettingsSection::Model,
        SettingsSection::Theme,
        SettingsSection::Updates,
        SettingsSection::Accounts,
        SettingsSection::Agents,
        SettingsSection::Prompts,
        SettingsSection::Skills,
        SettingsSection::AutoDrive,
        SettingsSection::Review,
        SettingsSection::Planning,
        SettingsSection::Validation,
        SettingsSection::Chrome,
        SettingsSection::Mcp,
        SettingsSection::Notifications,
        SettingsSection::Limits,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            SettingsSection::Model => "Model",
            SettingsSection::Theme => "Theme",
        SettingsSection::Planning => "Planning",
        SettingsSection::Updates => "Updates",
        SettingsSection::Accounts => "Accounts",
        SettingsSection::Agents => "Agents",
        SettingsSection::AutoDrive => "Auto Drive",
        SettingsSection::Review => "Review",
            SettingsSection::Validation => "Validation",
            SettingsSection::Limits => "Limits",
            SettingsSection::Chrome => "Chrome",
        SettingsSection::Mcp => "MCP",
        SettingsSection::Notifications => "Notifications",
        SettingsSection::Prompts => "Prompts",
        SettingsSection::Skills => "Skills",
        }
    }

    pub(crate) const fn help_line(self) -> &'static str {
        match self {
            SettingsSection::Model => "Choose the language model used for new completions.",
            SettingsSection::Theme => "Switch between preset color palettes and adjust contrast.",
        SettingsSection::Planning => "Choose the model used in Plan Mode (Read Only).",
        SettingsSection::Updates => "Control CLI auto-update cadence and release channels.",
        SettingsSection::Accounts => "Configure account switching behavior under rate and usage limits.",
        SettingsSection::Agents => "Configure linked agents and default task permissions.",
        SettingsSection::AutoDrive => "Manage Auto Drive defaults for review and cadence.",
        SettingsSection::Review => "Adjust Auto Review and Auto Resolve automation for /review.",
            SettingsSection::Validation => "Toggle validation groups and tool availability.",
            SettingsSection::Limits => "Inspect API usage, rate limits, and reset windows.",
            SettingsSection::Chrome => "Connect to Chrome or switch browser integrations.",
        SettingsSection::Mcp => "Enable and manage local MCP servers for tooling.",
        SettingsSection::Notifications => "Adjust desktop and terminal notification preferences.",
        SettingsSection::Prompts => "Create and edit custom prompt snippets.",
        SettingsSection::Skills => "Manage project-scoped and global skills.",
        }
    }

    pub(crate) const fn placeholder(self) -> &'static str {
        match self {
            SettingsSection::Model => "Model settings coming soon.",
            SettingsSection::Theme => "Theme settings coming soon.",
        SettingsSection::Planning => "Planning settings coming soon.",
        SettingsSection::Updates => "Upgrade Codex and manage automatic updates.",
        SettingsSection::Accounts => "Account switching settings coming soon.",
        SettingsSection::Agents => "Agents configuration coming soon.",
        SettingsSection::AutoDrive => "Auto Drive controls coming soon.",
            SettingsSection::Review => "Adjust Auto Review and Auto Resolve automation for /review.",
            SettingsSection::Validation => "Toggle validation groups and tools.",
            SettingsSection::Limits => "Limits usage visualization coming soon.",
            SettingsSection::Chrome => "Chrome integration settings coming soon.",
        SettingsSection::Mcp => "MCP server management coming soon.",
        SettingsSection::Notifications => "Notification preferences coming soon.",
        SettingsSection::Prompts => "Manage custom prompts.",
        SettingsSection::Skills => "Manage skills.",
        }
    }

    pub(crate) fn from_hint(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "model" | "models" => Some(SettingsSection::Model),
            "skill" | "skills" => Some(SettingsSection::Skills),
            "theme" | "themes" => Some(SettingsSection::Theme),
            "planning" | "plan" => Some(SettingsSection::Planning),
            "update" | "updates" => Some(SettingsSection::Updates),
            "account" | "accounts" | "auth" => Some(SettingsSection::Accounts),
            "agent" | "agents" => Some(SettingsSection::Agents),
            "auto" | "autodrive" | "drive" => Some(SettingsSection::AutoDrive),
            "review" | "reviews" => Some(SettingsSection::Review),
            "validation" | "validate" => Some(SettingsSection::Validation),
            "limit" | "limits" | "usage" => Some(SettingsSection::Limits),
            "chrome" | "browser" => Some(SettingsSection::Chrome),
            "mcp" => Some(SettingsSection::Mcp),
            "notification" | "notifications" | "notify" | "notif" => Some(SettingsSection::Notifications),
            _ => None,
        }
    }
}
