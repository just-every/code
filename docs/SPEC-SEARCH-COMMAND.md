# Slash Command: `/search`

This document outlines the design for a new `/search` slash command in the TUI, allowing users to find text within the conversation history.

## 1. Problem Statement & Success Criteria

*   **Problem:** Users with long conversation histories lack an efficient way to find specific information within the TUI, relying on manual scrolling.
*   **Success Criteria:**
    *   Users can execute `/search <query>` to find all occurrences of `<query>`.
    *   Matching text is highlighted in the history view.
    *   Users can navigate between matches.
    *   The search is performant on long histories.
    *   The feature is intuitive and easy to use.

## 2. Proposed UX / Command Flow

1.  **Initiating Search:**
    *   The user types `/search <query>` in the input box and presses Enter.
    *   The TUI enters a "search mode."

2.  **Displaying Results:**
    *   Matching text within the history cells is highlighted with a distinct background color.
    *   The history view automatically scrolls to the first match.
    *   A status line appears in the bottom pane, indicating the number of matches (e.g., "Search: 1 of 10 matches for 'query'").

3.  **Navigating Matches:**
    *   Pressing `n` (next) and `p` (previous) jumps between matches, with the history view scrolling accordingly.
    *   The status line updates to reflect the current match number (e.g., "Search: 2 of 10...").

4.  **Exiting Search:**
    *   Pressing `Esc` or `q` exits search mode.
    *   Highlights are removed, and the TUI returns to its normal state.

5.  **Edge Cases & Failure Handling:**
    *   **No Matches:** A message "No results found for '<query>'" is displayed in the status area.
    *   **Empty Query:** `/search` with no arguments will be ignored.
    *   **Search During Streaming:** Search will be disabled while the assistant is generating a response.

## 3. Implementation Plan

1.  **`slash_command.rs`:**
    *   Add `Search` to the `SlashCommand` enum with a user-facing description.
    *   Update `process_slash_command_message` to parse `/search` as a `RegularCommand`.

2.  **`chatwidget/mod.rs` (`ChatWidget`):**
    *   Introduce a `SearchState` struct in `ChatWidget` to manage the search mode, query, match locations, and current match index.
        ```rust
        struct SearchState {
            active: bool,
            query: String,
            matches: Vec<MatchLocation>,
            current_match: usize,
        }

        struct MatchLocation {
            cell_index: usize,
            // Location details for highlighting (e.g., line, char offset)
        }
        ```
    *   Implement a `perform_search(&mut self, query: &str)` method to iterate through `history_cells`, find matches using a new `get_text_for_search` method on `HistoryCell`, and populate `SearchState`.
    *   Update the `ChatWidget` event loop to handle the `/search` command, triggering the search and scrolling to the first result.
    *   Modify the `render` method to display a search status line when `SearchState.active` is true.
    *   Add key event handlers for `n`, `p`, and `Esc`/`q` to manage navigation and exit from search mode.

3.  **`history_cell/mod.rs` (`HistoryCell` trait):**
    *   Add a `get_text_for_search(&self) -> Cow<'_, str>` method to the `HistoryCell` trait to provide the searchable text content for each cell.
    *   Update rendering methods (`render_with_skip` or `custom_render_with_skip`) to accept search highlighting information.
    *   Implement highlighting logic. A simple initial approach will be to highlight the entire line containing a match. A more advanced implementation will highlight the specific substring.

4.  **Highlighting:**
    *   Define a new color for search result highlighting in `colors.rs`.
    *   Implement the highlighting logic carefully to ensure it composites correctly with existing text styles (e.g., syntax highlighting). A background color change is the preferred approach.

## 4. Risks & Open Questions

*   **Performance:** For very long histories, a simple linear scan on search activation is a reasonable starting point. Future optimizations could involve pre-indexing the conversation history.
*   **Highlighting Complexity:** Applying highlights to pre-styled and wrapped text is complex. The initial implementation may opt for line-level highlighting if substring-level highlighting proves too difficult to implement robustly within the existing rendering pipeline.
*   **Searchable Content:** The `get_text_for_search` method will provide control over what is indexed. The initial implementation will focus on user prompts, assistant messages, and reasoning text.
*   **UX for Navigation:** Simple `n`/`p` keybindings are sufficient for the initial version. Visual cues in the scrollbar could be added in a future iteration.
