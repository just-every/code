use super::*;
use codex_history::RetainedContextEvent;
use codex_history::RetainedInputSource;
use codex_history::RetainedUserMessage;
use codex_history::VerifiedAnswer;
use codex_history::VerifiedQuestionAnswer;
use pretty_assertions::assert_eq;

#[test]
fn instructions_preserve_source_order_and_whole_records() {
    let mut context = RetainedContext::default();
    // An answer at the existing 900-token byte limit still fits with order framing.
    let answer = "x".repeat(3_600 - "assistant: Publish?\nuser: \n".len());
    context.record(&RetainedContextEvent::VerifiedAnswer {
        answer: VerifiedAnswer {
            turn_id: "grant".to_owned(),
            call_id: "ask".to_owned(),
            questions: vec![VerifiedQuestionAnswer {
                question: "Publish?".to_owned(),
                answer: answer.clone(),
            }],
        },
        acceptance_order: None,
    });
    context.record_user_message(
        RetainedUserMessage {
            turn_id: "revocation".to_owned(),
            message_id: Some("msg_revoke".to_owned()),
            text: "Do not publish after all.".to_owned(),
            complete: true,
        },
        RetainedInputSource::Local(None),
    );
    let rendered = render_retained_instructions(&context);
    assert_eq!(
        rendered,
        vec!["Retained source order: 1\nuser: Do not publish after all.\n"]
    );
    let answers = crate::render_verified_answers(&context);
    assert_eq!(
        (answers.complete, answers.fragments),
        (
            true,
            vec![format!(
                "Retained source order: 0\nassistant: Publish?\nuser: {answer}\n"
            )]
        ),
    );
    context.record_user_message(
        RetainedUserMessage {
            turn_id: "oversized".to_owned(),
            message_id: Some("msg_large".to_owned()),
            text: "Permission is conditional. ".repeat(200),
            complete: true,
        },
        RetainedInputSource::Local(None),
    );
    let rendered = render_retained_instructions(&context);
    assert_eq!(rendered.len(), 2);
    assert!(rendered[0].starts_with("Host notice:"));
    assert!(
        !rendered
            .iter()
            .any(|fragment| fragment.contains("Permission is conditional"))
    );
}
