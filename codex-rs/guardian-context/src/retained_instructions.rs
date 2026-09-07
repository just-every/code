//! Selects original user instructions independently of model compaction and transcript selection.
//! The host owns storage and lifecycle. Omitted records remain explicit; restrictions
//! are never truncated into partial permissions, and retained source order is preserved.
//! Section omissions do not change fast-approval eligibility.

use codex_history::RetainedContext;
use codex_history::RetainedContextEntry;
use codex_protocol::protocol::TruncationPolicy;

use crate::ContextSection;
use crate::GuardianRootMessage;
use crate::SectionContributor;
use crate::SectionError;
use crate::SectionInput;
use crate::SectionScope;

const MAX_INSTRUCTION_TOKENS: usize = 900;

/// Renders bounded originals even when transcript selection also includes their source messages.
/// Presence in parent history alone cannot prove complete delivery to a reviewer.
fn render_retained_instructions(context: &RetainedContext) -> Vec<String> {
    let mut complete = context.user_messages_complete();
    let mut fragments = Vec::new();
    for (order, (_, entry)) in context.ordered_entries().enumerate() {
        let RetainedContextEntry::UserMessage(message) = entry else {
            continue;
        };
        let text = format!(
            "Retained source order: {order}\n{}",
            GuardianRootMessage::User(message.text.clone()).render()
        );
        if message.complete
            && text.len() <= TruncationPolicy::Tokens(MAX_INSTRUCTION_TOKENS).byte_budget()
        {
            fragments.push(text);
        } else {
            complete = false;
        }
    }
    if !complete {
        fragments.insert(/*index*/ 0, "Host notice: some retained user instructions are unavailable within the evidence budget. Do not treat remaining grants as complete authorization.\n".to_owned());
    }
    fragments
}

pub(crate) struct RetainedUserInstructionsSection;

impl SectionContributor for RetainedUserInstructionsSection {
    fn scope(&self) -> SectionScope {
        SectionScope::Shared
    }

    fn contribute(&self, input: &SectionInput<'_>) -> Result<Option<ContextSection>, SectionError> {
        let Some(context) = input.history.retained_context() else {
            return Ok(None);
        };
        let rendered = render_retained_instructions(context);
        if rendered.is_empty() {
            return Ok(None);
        }
        let mut items = vec![">>> RETAINED USER INSTRUCTIONS START\nHost: Retained source order labels across instructions and verified answers reflect original acceptance, not section order. Later instructions may revoke earlier grants.\n".to_owned()];
        items.extend(rendered);
        items.push(">>> RETAINED USER INSTRUCTIONS END\n".to_owned());
        Ok(Some(ContextSection::RetainedUserInstructions { items }))
    }
}

#[cfg(test)]
#[path = "retained_instructions_tests.rs"]
mod tests;
