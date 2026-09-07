//! Focused visual coverage and shared setup for voice behavior tests.

use super::RealtimeConversationPhase;
use crate::app_event::AppEvent;
use crate::chatwidget::ChatWidget;
use crate::chatwidget::tests::make_chatwidget_manual_with_sender;
use crate::chatwidget::tests::render_bottom_popup;
use codex_protocol::ThreadId;

pub(crate) fn activate_voice_for_thread(chat: &mut ChatWidget, thread_id: ThreadId) {
    chat.thread_id = Some(thread_id);
    chat.realtime_conversation.phase = RealtimeConversationPhase::Active;
    chat.realtime_conversation.thread_id = Some(thread_id);
    chat.realtime_conversation.backend_started = true;
    chat.realtime_conversation.latest_input_was_voice = true;
}

#[tokio::test]
async fn enabling_voice_on_an_open_thread_snapshots_the_new_thread_notice() {
    let (mut chat, _sender, mut events, _ops) = make_chatwidget_manual_with_sender().await;
    chat.set_feature_enabled(
        codex_features::Feature::RealtimeConversation,
        /*enabled*/ true,
    );
    let rendered = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            AppEvent::InsertHistoryCell(cell) => Some(
                cell.display_lines(/*width*/ 80)
                    .into_iter()
                    .map(|line| line.to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(rendered, @"• Voice conversations will be available in new threads.");
}

#[tokio::test]
async fn voice_footer_renders_the_main_conversation_states() {
    let (mut chat, _sender, _events, _ops) = make_chatwidget_manual_with_sender().await;
    let mut states = Vec::new();

    for (label, phase, muted, level, speaker_level, role, transcript) in [
        (
            "connecting",
            RealtimeConversationPhase::Starting,
            false,
            0,
            0,
            None,
            "",
        ),
        (
            "listening",
            RealtimeConversationPhase::Active,
            false,
            0,
            0,
            None,
            "",
        ),
        (
            "speaking",
            RealtimeConversationPhase::Active,
            false,
            4,
            5,
            None,
            "",
        ),
        (
            "muted",
            RealtimeConversationPhase::Active,
            true,
            4,
            4,
            None,
            "",
        ),
        (
            "transcript",
            RealtimeConversationPhase::Active,
            false,
            2,
            2,
            Some("assistant"),
            "Hello there",
        ),
    ] {
        chat.realtime_conversation.phase = phase;
        chat.realtime_conversation.microphone_muted = muted;
        chat.realtime_conversation.microphone_level = level;
        chat.realtime_conversation.speaker_level = speaker_level;
        chat.realtime_conversation.transcript_role = role.map(str::to_string);
        chat.realtime_conversation.transcript = transcript.to_string();
        chat.update_realtime_footer();
        states.push(format!(
            "{label}:\n{}",
            render_bottom_popup(&chat, /*width*/ 80)
        ));
    }

    insta::assert_snapshot!(states.join("\n\n"));
}
