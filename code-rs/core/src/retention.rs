//! Retention and budgeting policies for env_ctx_v2 timeline management.
//!
//! This module implements configurable retention defaults and budgeting for
//! environment context snapshots, deltas, and browser snapshots. It provides:
//! - Baseline immutable retention (keep most recent snapshot)
//! - Last N deltas retention
//! - Last K browser snapshots retention
//! - Byte cap budgeting across all retained items
//! - Telemetry for tracking bytes saved, delta counts, and deduplication

use code_protocol::models::ResponseItem;
use code_protocol::models::ContentItem;
use serde::{Deserialize, Serialize};

/// Retention policy configuration for env_ctx_v2 timeline items.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RetentionPolicy {
    /// Maximum number of environment context deltas to retain (default: 3)
    pub max_env_deltas: usize,
    /// Maximum number of browser snapshots to retain (default: 2)
    pub max_browser_snapshots: usize,
    /// Maximum total bytes for all retained env_ctx items (default: 100KB)
    pub max_total_bytes: usize,
    /// Always keep the most recent environment baseline snapshot
    pub keep_latest_baseline: bool,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_env_deltas: 3,
            max_browser_snapshots: 2,
            max_total_bytes: 100 * 1024, // 100 KB
            keep_latest_baseline: true,
        }
    }
}

/// Statistics tracked during retention pruning operations.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RetentionStats {
    /// Number of environment baseline snapshots removed
    pub removed_env_baselines: usize,
    /// Number of environment deltas removed
    pub removed_env_deltas: usize,
    /// Number of browser snapshots removed
    pub removed_browser_snapshots: usize,
    /// Number of screenshots removed
    pub removed_screenshots: usize,
    /// Number of status messages removed
    pub removed_status: usize,
    /// Number of environment deltas kept
    pub kept_env_deltas: usize,
    /// Number of browser snapshots kept
    pub kept_browser_snapshots: usize,
    /// Number of recent screenshots kept
    pub kept_recent_screenshots: usize,
    /// Total bytes of removed items
    pub bytes_removed: usize,
    /// Total bytes of kept items
    pub bytes_kept: usize,
    /// Number of items dropped due to byte budget constraints
    pub dropped_for_budget: usize,
}

impl RetentionStats {
    /// Returns true if any items were removed during pruning.
    pub fn any_removed(&self) -> bool {
        self.removed_screenshots > 0
            || self.removed_status > 0
            || self.removed_env_baselines > 0
            || self.removed_env_deltas > 0
            || self.removed_browser_snapshots > 0
    }

    /// Calculates bytes saved compared to legacy approach (no retention).
    pub fn bytes_saved(&self) -> usize {
        self.bytes_removed
    }

    /// Returns total delta count (kept + removed).
    pub fn total_delta_count(&self) -> usize {
        self.kept_env_deltas + self.removed_env_deltas
    }
}

/// Categorizes a response item for retention purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemCategory {
    /// Regular user message (real text input)
    RealUserMessage,
    /// System status message
    StatusMessage,
    /// Environment context baseline snapshot
    EnvBaseline,
    /// Environment context delta
    EnvDelta,
    /// Browser snapshot
    BrowserSnapshot,
    /// Screenshot image
    Screenshot,
    /// Other items (assistant messages, tool calls, etc.)
    Other,
}

/// Categorized item with size information.
#[derive(Debug)]
struct CategorizedItem {
    item: ResponseItem,
    category: ItemCategory,
    size_bytes: usize,
    index: usize,
}

impl CategorizedItem {
    fn new(item: ResponseItem, index: usize) -> Self {
        let category = categorize_item(&item);
        let size_bytes = estimate_item_size(&item);
        Self {
            item,
            category,
            size_bytes,
            index,
        }
    }
}

/// Estimates the byte size of a response item.
fn estimate_item_size(item: &ResponseItem) -> usize {
    match item {
        ResponseItem::Message { content, .. } => {
            content.iter().map(|c| match c {
                ContentItem::InputText { text } | ContentItem::OutputText { text } => text.len(),
                ContentItem::InputImage { image_url } => image_url.len(),
            }).sum()
        }
        ResponseItem::FunctionCall { arguments, .. } => arguments.len(),
        ResponseItem::FunctionCallOutput { output, .. } => output.content.len(),
        ResponseItem::CustomToolCall { input, .. } => input.len(),
        ResponseItem::CustomToolCallOutput { output, .. } => output.len(),
        ResponseItem::Reasoning { content, .. } => {
            content.as_ref().map(|c| c.len()).unwrap_or(0)
        }
        _ => 0,
    }
}

/// Categorizes a response item based on its content.
fn categorize_item(item: &ResponseItem) -> ItemCategory {
    const ENV_CTX_OPEN: &str = "<environment_context>";
    const ENV_CTX_DELTA_OPEN: &str = "<environment_context_delta>";
    const BROWSER_SNAPSHOT_OPEN: &str = "<browser_snapshot>";

    if let ResponseItem::Message { role, content, .. } = item {
        if role != "user" {
            return ItemCategory::Other;
        }

        let has_env_baseline = content.iter().any(|c| {
            if let ContentItem::InputText { text } = c {
                text.contains(ENV_CTX_OPEN) && !text.contains(ENV_CTX_DELTA_OPEN)
            } else {
                false
            }
        });

        if has_env_baseline {
            return ItemCategory::EnvBaseline;
        }

        let has_env_delta = content.iter().any(|c| {
            if let ContentItem::InputText { text } = c {
                text.contains(ENV_CTX_DELTA_OPEN)
            } else {
                false
            }
        });

        if has_env_delta {
            return ItemCategory::EnvDelta;
        }

        let has_browser_snapshot = content.iter().any(|c| {
            if let ContentItem::InputText { text } = c {
                text.contains(BROWSER_SNAPSHOT_OPEN)
            } else {
                false
            }
        });

        if has_browser_snapshot {
            return ItemCategory::BrowserSnapshot;
        }

        let has_status = content.iter().any(|c| {
            if let ContentItem::InputText { text } = c {
                text.contains("== System Status ==")
                    || text.contains("Current working directory:")
                    || text.contains("Git branch:")
            } else {
                false
            }
        });

        if has_status {
            return ItemCategory::StatusMessage;
        }

        let has_screenshot = content.iter().any(|c| matches!(c, ContentItem::InputImage { .. }));

        if has_screenshot {
            return ItemCategory::Screenshot;
        }

        let has_real_text = content.iter().any(|c| {
            if let ContentItem::InputText { text } = c {
                !text.contains("== System Status ==")
                    && !text.contains("Current working directory:")
                    && !text.contains("Git branch:")
                    && !text.trim().is_empty()
                    && !text.contains(ENV_CTX_OPEN)
                    && !text.contains(ENV_CTX_DELTA_OPEN)
                    && !text.contains(BROWSER_SNAPSHOT_OPEN)
            } else {
                false
            }
        });

        if has_real_text {
            ItemCategory::RealUserMessage
        } else {
            ItemCategory::StatusMessage
        }
    } else {
        ItemCategory::Other
    }
}

/// Applies retention policy to a list of history items.
///
/// Returns the pruned list of items and statistics about the operation.
pub fn apply_retention_policy(
    items: &[ResponseItem],
    policy: &RetentionPolicy,
) -> (Vec<ResponseItem>, RetentionStats) {
    let mut stats = RetentionStats::default();
    let categorized: Vec<CategorizedItem> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| CategorizedItem::new(item.clone(), idx))
        .collect();

    // Separate items by category
    let mut env_baselines = Vec::new();
    let mut env_deltas = Vec::new();
    let mut browser_snapshots = Vec::new();
    let mut screenshots = Vec::new();
    let mut status_messages = Vec::new();
    let mut other_items = Vec::new();

    for cat_item in categorized {
        match cat_item.category {
            ItemCategory::EnvBaseline => env_baselines.push(cat_item),
            ItemCategory::EnvDelta => env_deltas.push(cat_item),
            ItemCategory::BrowserSnapshot => browser_snapshots.push(cat_item),
            ItemCategory::Screenshot => screenshots.push(cat_item),
            ItemCategory::StatusMessage => status_messages.push(cat_item),
            _ => other_items.push(cat_item),
        }
    }

    // Apply retention rules
    let mut kept_items = Vec::new();

    // 1. Keep all non-categorized items (real messages, assistant, tool calls)
    for item in other_items {
        stats.bytes_kept += item.size_bytes;
        kept_items.push((item.index, item.item));
    }

    // 2. Keep only the latest baseline (immutable)
    if policy.keep_latest_baseline && !env_baselines.is_empty() {
        let latest = env_baselines.pop().unwrap();
        stats.bytes_kept += latest.size_bytes;
        kept_items.push((latest.index, latest.item));
    }

    // Remove older baselines
    for item in env_baselines {
        stats.removed_env_baselines += 1;
        stats.bytes_removed += item.size_bytes;
    }

    // 3. Keep last N deltas
    let deltas_to_keep = if env_deltas.len() <= policy.max_env_deltas {
        env_deltas
    } else {
        let to_drop = env_deltas.len() - policy.max_env_deltas;
        stats.removed_env_deltas += to_drop;
        for item in &env_deltas[..to_drop] {
            stats.bytes_removed += item.size_bytes;
        }
        env_deltas.into_iter().skip(to_drop).collect()
    };

    stats.kept_env_deltas = deltas_to_keep.len();
    for item in deltas_to_keep {
        stats.bytes_kept += item.size_bytes;
        kept_items.push((item.index, item.item));
    }

    // 4. Keep last K browser snapshots
    let snapshots_to_keep = if browser_snapshots.len() <= policy.max_browser_snapshots {
        browser_snapshots
    } else {
        let to_drop = browser_snapshots.len() - policy.max_browser_snapshots;
        stats.removed_browser_snapshots += to_drop;
        for item in &browser_snapshots[..to_drop] {
            stats.bytes_removed += item.size_bytes;
        }
        browser_snapshots.into_iter().skip(to_drop).collect()
    };

    stats.kept_browser_snapshots = snapshots_to_keep.len();
    for item in snapshots_to_keep {
        stats.bytes_kept += item.size_bytes;
        kept_items.push((item.index, item.item));
    }

    // 5. Remove all status messages and old screenshots (legacy behavior)
    for item in status_messages {
        stats.removed_status += 1;
        stats.bytes_removed += item.size_bytes;
    }

    // Keep only the most recent screenshot
    if !screenshots.is_empty() {
        let latest = screenshots.pop().unwrap();
        stats.kept_recent_screenshots = 1;
        stats.bytes_kept += latest.size_bytes;
        kept_items.push((latest.index, latest.item));

        for item in screenshots {
            stats.removed_screenshots += 1;
            stats.bytes_removed += item.size_bytes;
        }
    }

    // 6. Apply byte budget constraint
    if stats.bytes_kept > policy.max_total_bytes {
        // Sort kept items by index to maintain order
        kept_items.sort_by_key(|(idx, _)| *idx);

        // Drop items from the beginning until we're under budget
        // (keep most recent items)
        let mut cumulative_bytes = 0;
        let mut final_items = Vec::new();

        for (idx, item) in kept_items.iter().rev() {
            let item_size = estimate_item_size(item);
            if cumulative_bytes + item_size <= policy.max_total_bytes {
                cumulative_bytes += item_size;
                final_items.push((*idx, item.clone()));
            } else {
                stats.dropped_for_budget += 1;
                stats.bytes_removed += item_size;
                stats.bytes_kept -= item_size;
            }
        }

        kept_items = final_items;
        kept_items.sort_by_key(|(idx, _)| *idx);
    } else {
        // Sort by index to maintain chronological order
        kept_items.sort_by_key(|(idx, _)| *idx);
    }

    let result: Vec<ResponseItem> = kept_items.into_iter().map(|(_, item)| item).collect();
    (result, stats)
}

/// Owned variant of `apply_retention_policy` that avoids cloning large history items.
///
/// This is used by core-side cleanup paths where the history is already owned and we
/// want to prevent transient memory spikes from duplicating large payloads (e.g. base64
/// screenshots, environment context snapshots).
pub fn apply_retention_policy_owned(
    items: Vec<ResponseItem>,
    policy: &RetentionPolicy,
) -> (Vec<ResponseItem>, RetentionStats) {
    let mut stats = RetentionStats::default();

    let categorized: Vec<CategorizedItem> = items
        .into_iter()
        .enumerate()
        .map(|(idx, item)| CategorizedItem::new(item, idx))
        .collect();

    let mut env_baselines = Vec::new();
    let mut env_deltas = Vec::new();
    let mut browser_snapshots = Vec::new();
    let mut screenshots = Vec::new();
    let mut status_messages = Vec::new();
    let mut other_items = Vec::new();

    for cat_item in categorized {
        match cat_item.category {
            ItemCategory::EnvBaseline => env_baselines.push(cat_item),
            ItemCategory::EnvDelta => env_deltas.push(cat_item),
            ItemCategory::BrowserSnapshot => browser_snapshots.push(cat_item),
            ItemCategory::Screenshot => screenshots.push(cat_item),
            ItemCategory::StatusMessage => status_messages.push(cat_item),
            _ => other_items.push(cat_item),
        }
    }

    let mut kept: Vec<CategorizedItem> = Vec::new();

    // 1. Keep all non-categorized items (real messages, assistant, tool calls)
    for item in other_items {
        stats.bytes_kept += item.size_bytes;
        kept.push(item);
    }

    // 2. Keep only the latest baseline (immutable)
    if policy.keep_latest_baseline {
        if let Some(latest) = env_baselines.pop() {
            stats.bytes_kept += latest.size_bytes;
            kept.push(latest);
        }
    }

    for item in env_baselines {
        stats.removed_env_baselines += 1;
        stats.bytes_removed += item.size_bytes;
    }

    // 3. Keep last N deltas
    if env_deltas.len() > policy.max_env_deltas {
        let to_drop = env_deltas.len().saturating_sub(policy.max_env_deltas);
        stats.removed_env_deltas += to_drop;
        for item in env_deltas.drain(0..to_drop) {
            stats.bytes_removed += item.size_bytes;
        }
    }
    stats.kept_env_deltas = env_deltas.len();
    for item in env_deltas {
        stats.bytes_kept += item.size_bytes;
        kept.push(item);
    }

    // 4. Keep last K browser snapshots
    if browser_snapshots.len() > policy.max_browser_snapshots {
        let to_drop = browser_snapshots.len().saturating_sub(policy.max_browser_snapshots);
        stats.removed_browser_snapshots += to_drop;
        for item in browser_snapshots.drain(0..to_drop) {
            stats.bytes_removed += item.size_bytes;
        }
    }
    stats.kept_browser_snapshots = browser_snapshots.len();
    for item in browser_snapshots {
        stats.bytes_kept += item.size_bytes;
        kept.push(item);
    }

    // 5. Remove all status messages and old screenshots (legacy behavior)
    for item in status_messages {
        stats.removed_status += 1;
        stats.bytes_removed += item.size_bytes;
    }

    // Keep only the most recent screenshot
    if let Some(latest) = screenshots.pop() {
        stats.kept_recent_screenshots = 1;
        stats.bytes_kept += latest.size_bytes;
        kept.push(latest);
        for item in screenshots {
            stats.removed_screenshots += 1;
            stats.bytes_removed += item.size_bytes;
        }
    }

    // 6. Apply byte budget constraint
    kept.sort_by_key(|item| item.index);
    if stats.bytes_kept > policy.max_total_bytes {
        let mut cumulative_bytes = 0usize;
        let mut final_items: Vec<CategorizedItem> = Vec::new();
        let mut bytes_kept = stats.bytes_kept;

        for item in kept.into_iter().rev() {
            if cumulative_bytes.saturating_add(item.size_bytes) <= policy.max_total_bytes {
                cumulative_bytes = cumulative_bytes.saturating_add(item.size_bytes);
                final_items.push(item);
            } else {
                stats.dropped_for_budget += 1;
                stats.bytes_removed += item.size_bytes;
                bytes_kept = bytes_kept.saturating_sub(item.size_bytes);
            }
        }

        stats.bytes_kept = bytes_kept;
        final_items.sort_by_key(|item| item.index);
        kept = final_items;
    }

    let result = kept.into_iter().map(|item| item.item).collect();
    (result, stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_protocol::protocol::{
        BROWSER_SNAPSHOT_CLOSE_TAG, BROWSER_SNAPSHOT_OPEN_TAG,
        ENVIRONMENT_CONTEXT_CLOSE_TAG, ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG,
        ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG, ENVIRONMENT_CONTEXT_OPEN_TAG,
    };

    fn make_text_message(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
        }
    }

    fn make_screenshot_message(tag: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputImage {
                image_url: tag.to_string(),
            }],
        }
    }

    #[test]
    fn test_default_policy() {
        let policy = RetentionPolicy::default();
        assert_eq!(policy.max_env_deltas, 3);
        assert_eq!(policy.max_browser_snapshots, 2);
        assert_eq!(policy.max_total_bytes, 100 * 1024);
        assert!(policy.keep_latest_baseline);
    }

    #[test]
    fn test_retention_keeps_latest_baseline() {
        let policy = RetentionPolicy::default();

        let baseline1 = make_text_message(&format!(
            "{}\n{{\"version\":1}}\n{}",
            ENVIRONMENT_CONTEXT_OPEN_TAG, ENVIRONMENT_CONTEXT_CLOSE_TAG
        ));
        let baseline2 = make_text_message(&format!(
            "{}\n{{\"version\":2}}\n{}",
            ENVIRONMENT_CONTEXT_OPEN_TAG, ENVIRONMENT_CONTEXT_CLOSE_TAG
        ));

        let items = vec![baseline1.clone(), baseline2.clone()];
        let (pruned, stats) = apply_retention_policy(&items, &policy);

        assert_eq!(stats.removed_env_baselines, 1);
        assert!(pruned.contains(&baseline2));
        assert!(!pruned.contains(&baseline1));
    }

    #[test]
    fn test_retention_limits_deltas() {
        let policy = RetentionPolicy {
            max_env_deltas: 2,
            ..Default::default()
        };

        let delta1 = make_text_message(&format!(
            "{}\n{{\"seq\":1}}\n{}",
            ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG, ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG
        ));
        let delta2 = make_text_message(&format!(
            "{}\n{{\"seq\":2}}\n{}",
            ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG, ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG
        ));
        let delta3 = make_text_message(&format!(
            "{}\n{{\"seq\":3}}\n{}",
            ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG, ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG
        ));

        let items = vec![delta1.clone(), delta2.clone(), delta3.clone()];
        let (pruned, stats) = apply_retention_policy(&items, &policy);

        assert_eq!(stats.removed_env_deltas, 1);
        assert_eq!(stats.kept_env_deltas, 2);
        assert!(!pruned.contains(&delta1));
        assert!(pruned.contains(&delta2));
        assert!(pruned.contains(&delta3));
    }

    #[test]
    fn test_retention_limits_browser_snapshots() {
        let policy = RetentionPolicy {
            max_browser_snapshots: 1,
            ..Default::default()
        };

        let snap1 = make_text_message(&format!(
            "{}\n{{\"url\":\"https://first\"}}\n{}",
            BROWSER_SNAPSHOT_OPEN_TAG, BROWSER_SNAPSHOT_CLOSE_TAG
        ));
        let snap2 = make_text_message(&format!(
            "{}\n{{\"url\":\"https://second\"}}\n{}",
            BROWSER_SNAPSHOT_OPEN_TAG, BROWSER_SNAPSHOT_CLOSE_TAG
        ));

        let items = vec![snap1.clone(), snap2.clone()];
        let (pruned, stats) = apply_retention_policy(&items, &policy);

        assert_eq!(stats.removed_browser_snapshots, 1);
        assert_eq!(stats.kept_browser_snapshots, 1);
        assert!(!pruned.contains(&snap1));
        assert!(pruned.contains(&snap2));
    }

    #[test]
    fn test_retention_removes_old_screenshots() {
        let policy = RetentionPolicy::default();

        let screenshot1 = make_screenshot_message("data:image/png;base64,AAA");
        let screenshot2 = make_screenshot_message("data:image/png;base64,BBB");

        let items = vec![screenshot1.clone(), screenshot2.clone()];
        let (pruned, stats) = apply_retention_policy(&items, &policy);

        assert_eq!(stats.removed_screenshots, 1);
        assert_eq!(stats.kept_recent_screenshots, 1);
        assert!(!pruned.contains(&screenshot1));
        assert!(pruned.contains(&screenshot2));
    }

    #[test]
    fn test_retention_removes_status_messages() {
        let policy = RetentionPolicy::default();

        let status = make_text_message("== System Status ==\nSome info");
        let user_msg = make_text_message("Regular user message");

        let items = vec![status.clone(), user_msg.clone()];
        let (pruned, stats) = apply_retention_policy(&items, &policy);

        assert_eq!(stats.removed_status, 1);
        assert!(!pruned.contains(&status));
        assert!(pruned.contains(&user_msg));
    }

    #[test]
    fn test_bytes_saved_calculation() {
        let policy = RetentionPolicy::default();

        let large_delta = make_text_message(&format!(
            "{}\n{}\n{}",
            ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG,
            "X".repeat(1000),
            ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG
        ));

        let items = vec![large_delta; 5];
        let (_, stats) = apply_retention_policy(&items, &policy);

        // Should remove 2 deltas (keeping last 3)
        assert_eq!(stats.removed_env_deltas, 2);
        assert!(stats.bytes_saved() > 2000); // At least 2 * 1000 bytes
    }

    #[test]
    fn test_byte_budget_enforcement() {
        let policy = RetentionPolicy {
            max_total_bytes: 500,
            max_env_deltas: 10,
            ..Default::default()
        };

        let items: Vec<_> = (0..5)
            .map(|_| make_text_message(&format!(
                "{}\n{}\n{}",
                ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG,
                "X".repeat(200),
                ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG
            )))
            .collect();

        let (pruned, stats) = apply_retention_policy(&items, &policy);

        // Should drop some items due to byte budget
        assert!(stats.bytes_kept <= 500);
        assert!(stats.dropped_for_budget > 0 || pruned.len() < items.len());
    }

    #[test]
    fn test_maintains_chronological_order() {
        let policy = RetentionPolicy::default();

        let msg1 = make_text_message("First message");
        let delta1 = make_text_message(&format!(
            "{}\n{{\"seq\":1}}\n{}",
            ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG, ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG
        ));
        let msg2 = make_text_message("Second message");

        let items = vec![msg1.clone(), delta1.clone(), msg2.clone()];
        let (pruned, _) = apply_retention_policy(&items, &policy);

        // Order should be maintained
        assert_eq!(pruned[0], msg1);
        assert_eq!(pruned[1], delta1);
        assert_eq!(pruned[2], msg2);
    }
}
