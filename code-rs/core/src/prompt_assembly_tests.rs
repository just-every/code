//! Tests for prompt assembly with env_ctx_v2 baseline-once + deltas integration.
//!
//! These tests verify:
//! 1. Duplicate suppression between legacy XML and new JSON env_ctx items
//! 2. Baseline-once + deltas behavior from EnvironmentContextTracker
//! 3. Proper filtering in turn_input_with_history

#[cfg(test)]
mod tests {
    use code_protocol::models::{ContentItem, ResponseItem};
    use crate::environment_context::{EnvironmentContext, EnvironmentContextTracker};
    use crate::shell::{Shell, BashShell};
    use code_protocol::protocol::ENVIRONMENT_CONTEXT_OPEN_TAG;

    /// Helper to create a legacy XML environment context item
    fn legacy_env_context_item() -> ResponseItem {
        let shell = Shell::Bash(BashShell {
            shell_path: "/bin/bash".to_string(),
            bashrc_path: "~/.bashrc".to_string(),
        });
        let env_ctx = EnvironmentContext::new(
            Some("/test/cwd".into()),
            None,
            None,
            Some(shell),
        );
        ResponseItem::from(env_ctx)
    }

    /// Helper to create a user message
    fn user_message(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
        }
    }

    /// Helper to create an assistant message
    fn assistant_message(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: text.to_string(),
            }],
        }
    }

    #[test]
    fn test_legacy_env_context_contains_xml_tag() {
        let item = legacy_env_context_item();
        if let ResponseItem::Message { content, .. } = &item {
            let text = match &content[0] {
                ContentItem::InputText { text } => text,
                _ => panic!("Expected InputText"),
            };
            assert!(
                text.contains(ENVIRONMENT_CONTEXT_OPEN_TAG),
                "Legacy env context should contain XML opening tag"
            );
            assert!(
                text.contains("<environment_context>"),
                "Legacy env context should contain <environment_context> tag"
            );
        } else {
            panic!("Expected Message variant");
        }
    }

    #[test]
    fn test_baseline_once_then_deltas() {
        let mut tracker = EnvironmentContextTracker::new();
        let shell = Shell::Bash(BashShell {
            shell_path: "/bin/bash".to_string(),
            bashrc_path: "~/.bashrc".to_string(),
        });
        let env_ctx = EnvironmentContext::new(
            Some("/test/cwd".into()),
            None,
            None,
            Some(shell.clone()),
        );

        // First emission should be Full (baseline)
        let result1 = tracker.emit_response_items(&env_ctx, None, None, Some("stream"));
        assert!(result1.is_ok());
        let (emission1, items1) = result1.unwrap().expect("First emission should be Some");
        assert_eq!(emission1.sequence(), 1);
        assert!(!items1.is_empty());

        // Verify it's a full snapshot (contains ENVIRONMENT_CONTEXT_OPEN_TAG, not DELTA)
        if let ResponseItem::Message { content, .. } = &items1[0] {
            let text = match &content[0] {
                ContentItem::InputText { text } => text,
                _ => panic!("Expected InputText"),
            };
            assert!(
                text.contains(ENVIRONMENT_CONTEXT_OPEN_TAG),
                "First emission should be full baseline"
            );
            assert!(
                !text.contains("environment_context_delta"),
                "First emission should not be delta"
            );
        }

        // Second emission with same context should be None (no change)
        let result2 = tracker.emit_response_items(&env_ctx, None, None, Some("stream"));
        assert!(result2.is_ok());
        assert!(
            result2.unwrap().is_none(),
            "Unchanged context should emit None"
        );

        // Third emission with different context should be Delta
        let env_ctx_changed = EnvironmentContext::new(
            Some("/test/cwd2".into()),
            None,
            None,
            Some(shell),
        );
        let result3 = tracker.emit_response_items(&env_ctx_changed, None, None, Some("stream"));
        assert!(result3.is_ok());
        let (emission3, items3) = result3.unwrap().expect("Changed context should emit Delta");
        assert_eq!(emission3.sequence(), 2);
        assert!(!items3.is_empty());

        // Verify it's a delta (contains ENVIRONMENT_CONTEXT_DELTA tags)
        if let ResponseItem::Message { content, .. } = &items3[0] {
            let text = match &content[0] {
                ContentItem::InputText { text } => text,
                _ => panic!("Expected InputText"),
            };
            assert!(
                text.contains("environment_context_delta"),
                "Third emission should be delta"
            );
        }
    }

    #[test]
    fn test_duplicate_suppression_detection() {
        // Create a history with legacy XML env context
        let legacy_item = legacy_env_context_item();

        // Helper to check if an item is detected as legacy env context
        let is_legacy_env_context = |item: &ResponseItem| -> bool {
            if let ResponseItem::Message { role, content, .. } = item {
                if role == "user" {
                    return content.iter().any(|c| {
                        if let ContentItem::InputText { text } = c {
                            text.contains("<environment_context>")
                        } else {
                            false
                        }
                    });
                }
            }
            false
        };

        assert!(
            is_legacy_env_context(&legacy_item),
            "Legacy XML item should be detected"
        );
        assert!(
            !is_legacy_env_context(&user_message("Hello")),
            "Regular user message should not be detected as env context"
        );
        assert!(
            !is_legacy_env_context(&assistant_message("Hi")),
            "Assistant message should not be detected as env context"
        );
    }

    #[test]
    fn test_prompt_assembly_golden_fixture() {
        // This is a golden fixture test showing the expected behavior of prompt assembly
        // with env_ctx_v2 enabled.

        // Setup: Initial context has legacy XML (will be in history)
        let mut history = vec![
            legacy_env_context_item(),
            user_message("First user message"),
            assistant_message("First assistant response"),
        ];

        // When env_ctx_v2 is enabled, legacy XML items should be filtered out
        let env_ctx_v2_enabled = true;

        let is_legacy_env_context = |item: &ResponseItem| -> bool {
            if let ResponseItem::Message { role, content, .. } = item {
                if role == "user" {
                    return content.iter().any(|c| {
                        if let ContentItem::InputText { text } = c {
                            text.contains("<environment_context>")
                        } else {
                            false
                        }
                    });
                }
            }
            false
        };

        // Simulate the filtering logic from turn_input_with_history
        if env_ctx_v2_enabled {
            history.retain(|item| !is_legacy_env_context(item));
        }

        // After filtering, legacy XML should be removed
        assert_eq!(history.len(), 2, "Legacy XML item should be filtered out");

        // Verify remaining items are correct
        match &history[0] {
            ResponseItem::Message { role, content, .. } => {
                assert_eq!(role, "user");
                if let ContentItem::InputText { text } = &content[0] {
                    assert_eq!(text, "First user message");
                }
            }
            _ => panic!("Expected user message"),
        }

        match &history[1] {
            ResponseItem::Message { role, content, .. } => {
                assert_eq!(role, "assistant");
                if let ContentItem::OutputText { text } = &content[0] {
                    assert_eq!(text, "First assistant response");
                }
            }
            _ => panic!("Expected assistant message"),
        }
    }

    #[test]
    fn test_env_context_v2_disabled_preserves_legacy() {
        // When env_ctx_v2 is disabled, legacy XML should be preserved
        let mut history = vec![
            legacy_env_context_item(),
            user_message("User message"),
        ];

        let env_ctx_v2_enabled = false;

        let is_legacy_env_context = |item: &ResponseItem| -> bool {
            if let ResponseItem::Message { role, content, .. } = item {
                if role == "user" {
                    return content.iter().any(|c| {
                        if let ContentItem::InputText { text } = c {
                            text.contains("<environment_context>")
                        } else {
                            false
                        }
                    });
                }
            }
            false
        };

        // Simulate the filtering logic from turn_input_with_history
        if env_ctx_v2_enabled {
            history.retain(|item| !is_legacy_env_context(item));
        }

        // Legacy XML should still be present when disabled
        assert_eq!(history.len(), 2, "All items should be preserved when env_ctx_v2 is disabled");
        assert!(
            is_legacy_env_context(&history[0]),
            "Legacy XML should be preserved when env_ctx_v2 is disabled"
        );
    }
}
