use super::StreamCore;
use super::render_source;
use crate::history_cell::HistoryRenderMode;
use crate::markdown_render::preferences;
use codex_config::types::TuiRendering;
use pretty_assertions::assert_eq;

#[test]
fn empty_list_item_stays_mutable_until_finalization() {
    let cwd = std::env::temp_dir();
    for (marker, continuation) in [
        ("8.", ""),
        ("8.\n", "   answer\n"),
        ("> 8.\n", ">    answer\n"),
    ] {
        let mut stream = StreamCore::new(
            Some(24),
            &cwd,
            HistoryRenderMode::Rich,
            /*inline_visualization_context*/ None,
        );
        stream.push_delta(marker);
        let mut emitted = stream.tick_batch(usize::MAX);
        stream.push_delta(continuation);
        let (remaining, source) = stream.finalize_remaining();
        emitted.extend(remaining);
        assert_eq!(
            emitted,
            render_source(
                &source,
                Some(24),
                &cwd,
                HistoryRenderMode::Rich,
                /*inline_visualization_context*/ None,
            ),
            "marker={marker:?}"
        );
    }
}

#[test]
fn resizing_does_not_drop_held_list_marker() {
    let cwd = std::env::temp_dir();
    let mut stream = StreamCore::new(
        Some(10),
        &cwd,
        HistoryRenderMode::Rich,
        /*inline_visualization_context*/ None,
    );
    stream.push_delta("Long prose with enough words to wrap.\n\n");
    assert!(stream.tick_batch(usize::MAX).len() > 1);
    stream.push_delta("8.\n");
    stream.set_width(Some(40));
    let (remaining, _) = stream.finalize_remaining();
    assert_eq!(
        remaining.last().map(|line| line.line.to_string()),
        Some("8. ".to_string())
    );
}

#[test]
fn lists_match_incremental_and_emitted_responses() {
    let cwd = std::env::temp_dir();
    let source = concat!(
        "- [ ] First task with enough words to wrap\n",
        "  - [x] Nested task\n- [X] Done\n\n",
        "> 9. [ ] [link](https://example.com)\n> 10. [x] Done\n\n",
        "After.\n\nOne.\n\nTwo.\n\nThree.\n"
    );
    for lists in [true, false] {
        preferences::init(TuiRendering {
            lists,
            ..Default::default()
        });
        let mut stream = StreamCore::new(
            Some(24),
            &cwd,
            HistoryRenderMode::Rich,
            /*inline_visualization_context*/ None,
        );
        let render = |source: &str| {
            render_source(
                source,
                Some(24),
                &cwd,
                HistoryRenderMode::Rich,
                /*inline_visualization_context*/ None,
            )
        };
        let mut emitted = Vec::new();
        let mut committed = String::new();
        // Split inside markers as well as words to exercise partial task-list input.
        for character in source.chars() {
            committed.push(character);
            stream.push_delta(&character.to_string());
            emitted.extend(stream.tick_batch(usize::MAX));
            if character == '\n' {
                assert_eq!(stream.render.lines, render(&committed));
            }
        }
        let (remaining, _) = stream.finalize_remaining();
        emitted.extend(remaining);
        assert_eq!(emitted, render(source), "lists={lists}");
    }
}

#[test]
fn rendering_preferences_match_incremental_final_and_emitted_responses() {
    let cwd = std::env::temp_dir();
    let source = concat!(
        "Before.\n\n",
        "```markdown\n| A | B |\n|---|---|\n| `a` | \\(x_1\\) |\n```\n\n",
        "```mermaid\nflowchart LR\nA --> B\n```\n\n",
        "Inline \\(\\alpha_1\\) and $x^2$.\n\n\\[\n\\frac{a}{b}\n\\]\n\n",
        "| Name | Value |\n|---|---|\n| **a** | $x^2$ |\n\n",
        "Prose after the table.\n\nOne.\n\nTwo.\n\nThree.\n\nFour.\n\n",
        "| **Name** | Value |\n|---|---|\n| `a` | b |\n\nAfter.\n"
    );
    for mermaid in [true, false] {
        for math in [true, false] {
            for tables in [true, false] {
                preferences::init(TuiRendering {
                    mermaid,
                    math,
                    tables,
                    ..Default::default()
                });
                // A disabled table must retain even the prose preceding it inside its fence.
                let mut source = if tables {
                    source.to_owned()
                } else {
                    source.replacen(
                        "```markdown\n",
                        "```markdown\nIntro inside fence.\n\n",
                        /*count*/ 1,
                    )
                };
                if !math {
                    // A later protected code block can reject provisional math masking even
                    // without Unicode conversion, so the pending expression must stay mutable.
                    source.insert_str(
                        /*idx*/ 0,
                        "$$\n# heading\none\ntwo\nthree\nfour\nfive\n```rust\nx\n```\n$$\n\n",
                    );
                }
                let mut stream = StreamCore::new(
                    Some(80),
                    &cwd,
                    HistoryRenderMode::Rich,
                    /*inline_visualization_context*/ None,
                );
                let render = |source: &str| {
                    render_source(
                        source,
                        Some(80),
                        &cwd,
                        HistoryRenderMode::Rich,
                        /*inline_visualization_context*/ None,
                    )
                };
                let mut emitted = Vec::new();
                let mut committed = String::new();
                for chunk in source.split_inclusive('\n') {
                    committed.push_str(chunk);
                    stream.push_delta(chunk);
                    emitted.extend(stream.tick_batch(usize::MAX));
                    assert_eq!(
                        stream.render.lines,
                        render(&committed),
                        "{mermaid}/{math}/{tables}: {committed}"
                    );
                }
                if !tables {
                    assert!(
                        emitted
                            .iter()
                            .any(|line| line.line.to_string() == "Prose after the table.")
                    );
                }
                let (remaining, _) = stream.finalize_remaining();
                emitted.extend(remaining);
                assert_eq!(emitted, render(&source), "{mermaid}/{math}/{tables}");
            }
        }
    }
}
