use super::*;
use pretty_assertions::assert_eq;

#[test]
fn delivery_preserves_arbitrary_message_boundaries_and_rejects_them_for_sync() {
    let message = crate::PreviousReviews::try_from_fragments(vec!["host-attested review".into()])
        .unwrap()
        .into_message();
    let sections = || {
        vec![
            SectionOutput {
                id: "new_user_section",
                delivery: text_content(vec!["first".into(), "second".into()]),
            },
            SectionOutput {
                id: "new_message_section",
                delivery: SectionDelivery::Message(Box::new(message.clone())),
            },
            SectionOutput {
                id: "another_user_section",
                delivery: text_content(vec!["third".into()]),
            },
        ]
    };
    assert_eq!(
        ComposedContext {
            sections: sections(),
            truncations: Vec::new()
        }
        .into_messages(),
        vec![
            user_message(vec![
                ContentItem::InputText {
                    text: "first".into()
                },
                ContentItem::InputText {
                    text: "second".into()
                }
            ]),
            message.clone(),
            user_message(vec![ContentItem::InputText {
                text: "third".into()
            }]),
        ]
    );
    assert_eq!(
        ComposedContext {
            sections: sections(),
            truncations: Vec::new()
        }
        .into_user_inputs(),
        Err(SectionError::UnsupportedDelivery {
            section: "new_message_section"
        })
    );
}
