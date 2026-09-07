//! Section identities survive consumer-specific transcript selection and rendering.

use crate::ConversationTranscriptEntry;
use crate::PlannedAction;

/// Ordered evidence with a stable section identity and source-specific content.
///
/// Variants preserve provenance: transcript entries carry their original roles,
/// root messages remain line-role-labeled, and answers are host-verified fragments.
/// All currently supported sections are delivered as user-role evidence. Source
/// attribution never promotes their contents to developer instructions.
#[derive(Clone, Debug, PartialEq)]
pub enum ContextSection<T = ConversationTranscriptEntry> {
    ConversationTranscript { items: Vec<T> },
    RootConversation { items: Vec<String> },
    TrustedUserAnswers { items: Vec<String> },
    RetainedUserInstructions { items: Vec<String> },
    PlannedAction(PlannedAction),
    PermissionContext { items: Vec<String> },
}
