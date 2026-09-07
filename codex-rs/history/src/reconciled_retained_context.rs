//! Read-only recovery of retained instructions from eligible live messages.
//! Missing retained instructions use persisted acceptance order; unknown ordering stays incomplete.
//! Recovering surviving sources cannot clear a checkpoint's record of missing instructions.

use std::collections::HashSet;

use crate::RetainedContext;
use crate::RetainedContextEntry;
use crate::RetainedContextOrder;
use crate::RetainedUserMessage;

/// Retained evidence supplemented by surviving user messages without changing the checkpoint.
pub struct ReconciledRetainedContext<'a> {
    retained_context: Option<&'a RetainedContext>,
    recovered: Vec<(RetainedContextOrder, RetainedUserMessage)>,
    /// Instructions were already missing, or a recovered source could not be ordered.
    pub missing_user_messages: bool,
}

impl<'a> ReconciledRetainedContext<'a> {
    /// Reconciles eligible live user messages with retained sources in acceptance order.
    /// The caller supplies original source identities and filters out non-user context.
    /// Inherited sources must supply no local order; retained identities match before ordering.
    /// Live messages are consumed only when retained user instructions are incomplete.
    pub fn new(
        retained_context: Option<&'a RetainedContext>,
        live_messages: impl IntoIterator<Item = (Option<u64>, RetainedUserMessage)>,
    ) -> Self {
        let mut missing_user_messages =
            retained_context.is_none_or(RetainedContext::has_missing_user_messages);
        let retained_entries = retained_context
            .into_iter()
            .flat_map(RetainedContext::ordered_entries)
            .collect::<Vec<_>>();
        let mut recovered = Vec::new();
        if retained_context.is_none_or(|context| !context.user_messages_complete()) {
            let mut matched_entries = HashSet::new();
            let mut used_orders = retained_entries
                .iter()
                .map(|(order, _)| *order)
                .collect::<HashSet<_>>();
            for (order, message) in live_messages {
                if retained_entries
                    .iter()
                    .enumerate()
                    .any(|(index, (_, entry))| {
                        let RetainedContextEntry::UserMessage(retained) = entry else {
                            return false;
                        };
                        let matches_source = if let Some(id) = &retained.message_id {
                            message.message_id.as_ref() == Some(id)
                        } else {
                            message.turn_id == retained.turn_id && message.text == retained.text
                        };
                        matches_source && matched_entries.insert(index)
                    })
                {
                    continue;
                }
                // Queued steering can enter raw history after a later-accepted answer.
                // Compare their persisted sequence numbers, including checkpoint-only answers.
                // Parent counters cannot establish an adopted prefix position.
                let Some(order) = order
                    .map(RetainedContextOrder::Local)
                    .filter(|order| used_orders.insert(*order))
                else {
                    missing_user_messages = true;
                    continue;
                };
                recovered.push((order, message));
            }
        }
        Self {
            retained_context,
            recovered,
            missing_user_messages,
        }
    }

    /// Returns retained and recovered evidence in persisted acceptance order.
    pub fn ordered_entries(
        &self,
    ) -> impl DoubleEndedIterator<Item = (RetainedContextOrder, RetainedContextEntry<'_>)> {
        let mut entries = self
            .retained_context
            .into_iter()
            .flat_map(RetainedContext::ordered_entries)
            .collect::<Vec<_>>();
        entries.extend(
            self.recovered
                .iter()
                .map(|(order, message)| (*order, RetainedContextEntry::UserMessage(message))),
        );
        entries.sort_by_key(|(order, _)| *order);
        entries.into_iter()
    }
}

#[cfg(test)]
#[path = "reconciled_retained_context_tests.rs"]
mod tests;
