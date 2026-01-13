use crate::history::state::{
    HistoryId,
    InlineSpan,
    MessageHeader,
    MessageLine,
    MessageLineKind,
    PlainMessageKind,
    PlainMessageRole,
    PlainMessageState,
    TextEmphasis,
    TextTone,
};
use crate::history_cell::formatting::normalize_overwrite_sequences;
use crate::sanitize::Mode as SanitizeMode;
use crate::sanitize::Options as SanitizeOptions;
use crate::sanitize::sanitize_for_tui;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;

fn span(text: impl Into<String>, tone: TextTone, bold: bool) -> InlineSpan {
    InlineSpan {
        text: text.into(),
        tone,
        emphasis: TextEmphasis {
            bold,
            ..TextEmphasis::default()
        },
        entity: None,
    }
}

fn message_lines_from_text(text: &str) -> Vec<MessageLine> {
    let normalized = normalize_overwrite_sequences(text);
    let sanitized = sanitize_for_tui(
        &normalized,
        SanitizeMode::Plain,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );

    if sanitized.is_empty() {
        return Vec::new();
    }

    sanitized
        .lines()
        .map(|line| {
            let is_blank = line.trim().is_empty();
            MessageLine {
                kind: if is_blank {
                    MessageLineKind::Blank
                } else {
                    MessageLineKind::Paragraph
                },
                spans: vec![InlineSpan {
                    text: line.to_string(),
                    tone: TextTone::Default,
                    emphasis: TextEmphasis::default(),
                    entity: None,
                }],
            }
        })
        .collect()
}

static WEAVE_ACCENT_OVERRIDES: OnceLock<Mutex<HashMap<String, u8>>> = OnceLock::new();

pub(crate) fn set_weave_agent_accent_override(agent_id: String, accent: u8) {
    let normalized = agent_id.trim();
    if normalized.is_empty() {
        return;
    }
    let map = WEAVE_ACCENT_OVERRIDES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut locked) = map.lock() {
        locked.insert(normalized.to_string(), accent % 8);
    }
}

pub(crate) fn clear_weave_agent_accent_override(agent_id: &str) {
    let normalized = agent_id.trim();
    if normalized.is_empty() {
        return;
    }
    let Some(map) = WEAVE_ACCENT_OVERRIDES.get() else {
        return;
    };
    if let Ok(mut locked) = map.lock() {
        locked.remove(normalized);
    }
}

fn weave_agent_tone_for_id(agent_id: &str) -> TextTone {
    if let Some(map) = WEAVE_ACCENT_OVERRIDES.get()
        && let Ok(locked) = map.lock()
        && let Some(accent) = locked.get(agent_id.trim())
    {
        return TextTone::Accent(*accent);
    }
    TextTone::Accent(weave_agent_accent_index(agent_id))
}

fn weave_agent_accent_index(agent_id: &str) -> u8 {
    const ACCENT_COUNT: u64 = 8;
    let normalized = agent_id.trim();
    if normalized.is_empty() {
        return 0;
    }
    let hash = fnv1a64(normalized);
    u8::try_from(hash % ACCENT_COUNT).unwrap_or(0)
}

fn fnv1a64(input: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub(crate) fn new_weave_inbound(
    sender_id: String,
    sender_label: String,
    _receiver_id: String,
    _receiver_label: String,
    message: String,
) -> PlainMessageState {
    let mut lines: Vec<MessageLine> = Vec::new();

    let mut message_lines = message_lines_from_text(&message);
    let first_line = message_lines
        .first()
        .and_then(|line| line.spans.first())
        .map(|span| span.text.clone())
        .filter(|text| !text.trim().is_empty());
    if first_line.is_some() && !message_lines.is_empty() {
        message_lines.remove(0);
    }

    let mut header_spans: Vec<InlineSpan> = Vec::new();
    header_spans.push(span(
        sender_label,
        weave_agent_tone_for_id(&sender_id),
        true,
    ));
    header_spans.push(span(": ", TextTone::Dim, false));
    if let Some(first_line) = first_line {
        header_spans.push(span(first_line, TextTone::Default, false));
    }
    lines.push(MessageLine {
        kind: MessageLineKind::Paragraph,
        spans: header_spans,
    });

    lines.extend(message_lines);

    PlainMessageState {
        id: HistoryId::ZERO,
        role: PlainMessageRole::System,
        kind: PlainMessageKind::Notice,
        header: Some(MessageHeader {
            label: "weave".to_string(),
            badge: None,
        }),
        lines,
        metadata: None,
    }
}

pub(crate) fn new_weave_outbound(
    sender_id: String,
    sender_label: String,
    recipients: Vec<(String, String)>,
    message: String,
) -> PlainMessageState {
    let mut lines: Vec<MessageLine> = Vec::new();

    let mut message_lines = message_lines_from_text(&message);
    let first_line = message_lines
        .first()
        .and_then(|line| line.spans.first())
        .map(|span| span.text.clone())
        .filter(|text| !text.trim().is_empty());
    if first_line.is_some() && !message_lines.is_empty() {
        message_lines.remove(0);
    }

    let mut header_spans: Vec<InlineSpan> = Vec::new();
    header_spans.push(span(
        sender_label,
        weave_agent_tone_for_id(&sender_id),
        true,
    ));
    header_spans.push(span(" → ", TextTone::Dim, false));
    if recipients.is_empty() {
        header_spans.push(span("(no recipients)", TextTone::Dim, false));
    } else {
        for (idx, (recipient_id, recipient_label)) in recipients.iter().enumerate() {
            if idx > 0 {
                header_spans.push(span(", ", TextTone::Dim, false));
            }
            header_spans.push(span(
                recipient_label.clone(),
                weave_agent_tone_for_id(recipient_id.as_str()),
                true,
            ));
        }
    }
    header_spans.push(span(": ", TextTone::Dim, false));
    if let Some(first_line) = first_line {
        header_spans.push(span(first_line, TextTone::Default, false));
    }

    lines.push(MessageLine {
        kind: MessageLineKind::Paragraph,
        spans: header_spans,
    });

    lines.extend(message_lines);

    PlainMessageState {
        id: HistoryId::ZERO,
        role: PlainMessageRole::System,
        kind: PlainMessageKind::Notice,
        header: Some(MessageHeader {
            label: "weave".to_string(),
            badge: None,
        }),
        lines,
        metadata: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone_for_label(state: &PlainMessageState, label: &str) -> Option<TextTone> {
        state.lines.iter().find_map(|line| {
            line.spans
                .iter()
                .find(|span| span.text == label)
                .map(|span| span.tone)
        })
    }

    #[test]
    fn weave_agent_tone_is_stable() {
        clear_weave_agent_accent_override("alice");
        clear_weave_agent_accent_override("bob");
        assert_eq!(weave_agent_tone_for_id("alice"), TextTone::Accent(7));
        assert_eq!(weave_agent_tone_for_id("bob"), TextTone::Accent(4));
    }

    #[test]
    fn weave_inbound_uses_agent_tones() {
        let state = new_weave_inbound(
            "alice".to_string(),
            "Alice".to_string(),
            "local".to_string(),
            "Local".to_string(),
            "hello".to_string(),
        );

        assert_eq!(tone_for_label(&state, "Alice"), Some(TextTone::Accent(7)));
        assert_eq!(tone_for_label(&state, "Local"), None);
    }

    #[test]
    fn weave_agent_tone_can_be_overridden() {
        set_weave_agent_accent_override("override-id".to_string(), 3);
        assert_eq!(
            weave_agent_tone_for_id("override-id"),
            TextTone::Accent(3)
        );
    }

    #[test]
    fn weave_outbound_colors_each_recipient() {
        let state = new_weave_outbound(
            "local".to_string(),
            "Local".to_string(),
            vec![
                ("alice".to_string(), "Alice".to_string()),
                ("bob".to_string(), "Bob".to_string()),
            ],
            "ping".to_string(),
        );

        assert_eq!(tone_for_label(&state, "Local"), Some(TextTone::Accent(0)));
        assert_eq!(tone_for_label(&state, "Alice"), Some(TextTone::Accent(7)));
        assert_eq!(tone_for_label(&state, "Bob"), Some(TextTone::Accent(4)));
    }
}
