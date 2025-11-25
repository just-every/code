use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::layout::Margin;
use ratatui::widgets::WidgetRef;

use super::popup_consts::MAX_POPUP_ROWS;
use super::scroll_state::ScrollState;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::render_rows;
use crate::slash_command::SlashCommand;
use crate::slash_command::built_in_slash_commands;
use code_common::fuzzy_match::fuzzy_match;
use code_protocol::custom_prompts::CustomPrompt;
use code_protocol::custom_prompts::PROMPTS_CMD_PREFIX;
use std::collections::HashSet;

/// A selectable item in the popup: either a built-in command or a user prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandItem {
    Builtin(SlashCommand),
    // Index into `prompts`
    UserPrompt(usize),
    // Index into `subagents`
    Subagent(usize),
}

pub(crate) struct CommandPopup {
    command_filter: String,
    builtins: Vec<(&'static str, SlashCommand)>,
    prompts: Vec<CustomPrompt>,
    state: ScrollState,
    subagents: Vec<String>,
}

impl CommandPopup {
    pub(crate) fn new_with_filter(hide_verbosity: bool) -> Self {
        let mut commands = built_in_slash_commands();
        if hide_verbosity {
            // Filter out the verbosity command when using ChatGPT auth
            commands.retain(|(_, cmd)| *cmd != SlashCommand::Verbosity);
        }
        Self {
            command_filter: String::new(),
            builtins: commands,
            prompts: Vec::new(),
            state: ScrollState::new(),
            subagents: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn set_prompts(&mut self, mut prompts: Vec<CustomPrompt>) {
        let exclude: HashSet<String> = self
            .builtins
            .iter()
            .map(|(n, _)| (*n).to_string())
            .collect();
        prompts.retain(|p| !exclude.contains(&p.name));
        prompts.sort_by(|a, b| a.name.cmp(&b.name));
        self.prompts = prompts;
    }

    pub(crate) fn prompt(&self, idx: usize) -> Option<&CustomPrompt> {
        self.prompts.get(idx)
    }

    pub(crate) fn subagent_name(&self, idx: usize) -> Option<&str> {
        self.subagents.get(idx).map(|s| s.as_str())
    }

    /// Supply custom subagent command names (e.g., ["demo", "ship"]) to include in the
    /// slash popup. Built-ins should already be excluded by the caller.
    pub(crate) fn set_subagent_commands(&mut self, mut names: Vec<String>) {
        // Normalize: drop duplicates, keep stable order
        let mut seen = HashSet::new();
        names.retain(|n| seen.insert(n.to_ascii_lowercase()));
        self.subagents = names;
        // Clamp selection relative to new item count
        self.state.clamp_selection(self.filtered_items().len());
    }

    /// Update the filter string based on the current composer text. The text
    /// passed in is expected to start with a leading '/'. Everything after the
    /// *first* '/" on the *first* line becomes the active filter that is used
    /// to narrow down the list of available commands.
    pub(crate) fn on_composer_text_change(&mut self, text: String) {
        let first_line = text.lines().next().unwrap_or("");

        if let Some(stripped) = first_line.strip_prefix('/') {
            // Extract the *first* token (sequence of non-whitespace
            // characters) after the slash so that `/clear something` still
            // shows the help for `/clear`.
            let token = stripped.trim_start();
            let cmd_token = token.split_whitespace().next().unwrap_or("");

            // Update the filter keeping the original case (commands are all
            // lower-case for now but this may change in the future).
            self.command_filter = cmd_token.to_string();
        } else {
            // The composer no longer starts with '/'. Reset the filter so the
            // popup shows the *full* command list if it is still displayed
            // for some reason.
            self.command_filter.clear();
        }

        // Reset or clamp selected index based on new filtered list.
        let matches_len = self.filtered_items().len();
        self.state.clamp_selection(matches_len);
        self.state
            .ensure_visible(matches_len, MAX_POPUP_ROWS.min(matches_len));
    }

    /// Determine the preferred height of the popup. This is the number of
    /// rows required to show at most MAX_POPUP_ROWS commands.
    pub(crate) fn calculate_required_height(&self) -> u16 {
        self.filtered_items().len().clamp(1, MAX_POPUP_ROWS) as u16
    }

    /// Compute fuzzy-filtered matches over built-in commands and user prompts,
    /// paired with optional highlight indices and score. Sorted by ascending
    /// score, then by name for stability.
    fn filtered(&self) -> Vec<(CommandItem, Option<Vec<usize>>, i32)> {
        let filter = self.command_filter.trim();
        let mut out: Vec<(CommandItem, Option<Vec<usize>>, i32)> = Vec::new();
        if filter.is_empty() {
            // Built-ins first, in presentation order.
            for (_, cmd) in self.builtins.iter() {
                out.push((CommandItem::Builtin(*cmd), None, 0));
            }
            // Then subagent commands
            for (idx, _) in self.subagents.iter().enumerate() {
                out.push((CommandItem::Subagent(idx), None, 0));
            }
            // Then prompts, already sorted by name.
            for idx in 0..self.prompts.len() {
                out.push((CommandItem::UserPrompt(idx), None, 0));
            }
            return out;
        }

        for (_, cmd) in self.builtins.iter() {
            if let Some((indices, score)) = fuzzy_match(cmd.command(), filter) {
                out.push((CommandItem::Builtin(*cmd), Some(indices), score));
            }
        }
        for (idx, name) in self.subagents.iter().enumerate() {
            if let Some((indices, score)) = fuzzy_match(name, filter) {
                out.push((CommandItem::Subagent(idx), Some(indices), score));
            }
        }
        for (idx, p) in self.prompts.iter().enumerate() {
            let prefixed = format!("{PROMPTS_CMD_PREFIX}:{}", p.name);
            let mut best: Option<(Vec<usize>, i32)> = None;
            if let Some((indices, score)) = fuzzy_match(&prefixed, filter) {
                best = Some((indices, score));
            }
            if let Some((indices, score)) = fuzzy_match(&p.name, filter) {
                match best {
                    Some((_, s)) if score < s => best = Some((indices, score)),
                    Some((_, s)) if score == s => best = Some((indices, score)),
                    None => best = Some((indices, score)),
                    _ => {}
                }
            }
            if let Some((indices, score)) = best {
                out.push((CommandItem::UserPrompt(idx), Some(indices), score));
            }
        }
        // When filtering, sort by ascending score, then prefer built-ins over
        // subagents over prompts to keep core commands easy to reach on exact
        // matches (e.g., `/prompts` should not auto-select a custom prompt),
        // and finally fall back to name for stability.
        out.sort_by(|a, b| {
            use std::cmp::Ordering;

            let score_cmp = a.2.cmp(&b.2);
            if score_cmp != Ordering::Equal {
                return score_cmp;
            }

            let rank = |item: &CommandItem| match item {
                CommandItem::Builtin(_) => 0,
                CommandItem::Subagent(_) => 1,
                CommandItem::UserPrompt(_) => 2,
            };
            let rank_cmp = rank(&a.0).cmp(&rank(&b.0));
            if rank_cmp != Ordering::Equal {
                return rank_cmp;
            }

            let an = match a.0 {
                CommandItem::Builtin(c) => c.command(),
                CommandItem::UserPrompt(i) => &self.prompts[i].name,
                CommandItem::Subagent(i) => &self.subagents[i],
            };
            let bn = match b.0 {
                CommandItem::Builtin(c) => c.command(),
                CommandItem::UserPrompt(i) => &self.prompts[i].name,
                CommandItem::Subagent(i) => &self.subagents[i],
            };
            an.cmp(bn)
        });
        out
    }

    fn filtered_items(&self) -> Vec<CommandItem> {
        self.filtered().into_iter().map(|(c, _, _)| c).collect()
    }

    /// Return the current number of selectable commands under the active filter.
    pub(crate) fn match_count(&self) -> usize {
        self.filtered_items().len()
    }

    /// Move the selection cursor one step up.
    pub(crate) fn move_up(&mut self) {
        let len = self.filtered_items().len();
        self.state.move_up_wrap(len);
        self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len));
    }

    /// Move the selection cursor one step down.
    pub(crate) fn move_down(&mut self) {
        let matches_len = self.filtered_items().len();
        self.state.move_down_wrap(matches_len);
        self.state
            .ensure_visible(matches_len, MAX_POPUP_ROWS.min(matches_len));
    }

    /// Return currently selected command, if any.
    pub(crate) fn selected_item(&self) -> Option<CommandItem> {
        let matches = self.filtered_items();
        self.state
            .selected_idx
            .and_then(|idx| matches.get(idx).copied())
    }
}

impl WidgetRef for CommandPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        // Add two spaces of left padding so suggestions align with the
        // slash command typed inside the composer (which has 1px border +
        // 1 space inner padding). This keeps the popup visually lined up
        // with the input text.
        let indented_area = area.inner(Margin::new(2, 0));
        let matches = self.filtered();
        let rows_all: Vec<GenericDisplayRow> = if matches.is_empty() {
            Vec::new()
        } else {
            matches
                .into_iter()
                .map(|(cmd_item, indices, _)| {
                    let (name, desc) = match cmd_item {
                        CommandItem::Builtin(cmd) => (
                            format!("/{}", cmd.command()),
                            Some(cmd.description().to_string()),
                        ),
                        CommandItem::UserPrompt(i) => {
                            let prompt = &self.prompts[i];
                            let preview = prompt
                                .content
                                .lines()
                                .next()
                                .unwrap_or("")
                                .trim();
                            let desc = if preview.is_empty() {
                                None
                            } else {
                                Some(format!("[custom] {preview}"))
                            };
                            (format!("/{}", prompt.name), desc)
                        }
                        CommandItem::Subagent(i) => (
                            format!("/{}", self.subagents[i]),
                            Some("custom subagent".to_string()),
                        ),
                    };
                    GenericDisplayRow {
                        name,
                        match_indices: indices
                            .map(|v| v.into_iter().map(|i| i + 1).collect()),
                        is_current: false,
                        description: desc,
                        // Slash command names should use theme primary color
                        name_color: Some(crate::colors::primary()),
                    }
                })
                .collect()
        };
        render_rows(
            indented_area,
            buf,
            &rows_all,
            &self.state,
            MAX_POPUP_ROWS,
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn prompt(name: &str, content: &str) -> CustomPrompt {
        CustomPrompt {
            name: name.to_string(),
            path: PathBuf::new(),
            content: content.to_string(),
            description: None,
            argument_hint: None,
        }
    }

    #[test]
    fn prefers_builtin_over_prompt_with_identical_score() {
        let mut popup = CommandPopup::new_with_filter(false);
        popup.set_prompts(vec![prompt("issues", "do something")]);

        // Filter to the prompts command; both the built-in "/prompts" and the
        // custom prompt (via "prompts:issues") are perfect prefix matches.
        popup.on_composer_text_change("/prompts".to_string());

        let first = popup.filtered_items().first().copied();
        assert!(matches!(first, Some(CommandItem::Builtin(SlashCommand::Prompts))));
    }
}
