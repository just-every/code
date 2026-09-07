//! Group older history items at successful turn boundaries before projecting transcript cells.
//! A page splitting a turn only adds completion metadata when it contains that turn's last item.

use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnStatus;
use std::collections::HashMap;

pub(super) fn group_completed_turn_items(
    items: Vec<ThreadItem>,
    turns: &[Turn],
) -> Vec<(Vec<ThreadItem>, Option<&Turn>)> {
    let completed_boundaries: HashMap<_, _> = turns
        .iter()
        .filter(|turn| turn.status == TurnStatus::Completed)
        .filter_map(|turn| turn.items.last().map(|item| (item.id(), turn)))
        .collect();
    let mut groups = Vec::new();
    let mut pending = Vec::new();
    for item in items {
        let completed_turn = completed_boundaries.get(item.id()).copied();
        pending.push(item);
        if completed_turn.is_some() {
            groups.push((std::mem::take(&mut pending), completed_turn));
        }
    }
    if !pending.is_empty() {
        groups.push((pending, None));
    }
    groups
}

#[cfg(test)]
#[path = "history_completion_tests.rs"]
mod tests;
