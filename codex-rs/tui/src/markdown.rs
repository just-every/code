#![allow(dead_code)]

use crate::citation_regex::CITATION_REGEX;
use crate::markdown_renderer::MarkdownRenderer;
use codex_core::config::Config;
use ratatui::style::Style;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;
use codex_core::config_types::UriBasedFileOpener;
use ratatui::text::Line;
use std::borrow::Cow;
use std::path::Path;

pub(crate) fn append_markdown(
    markdown_source: &str,
    lines: &mut Vec<Line<'static>>,
    config: &Config,
) {
    append_markdown_with_opener_and_cwd(markdown_source, lines, config.file_opener, &config.cwd);
}

pub(crate) fn append_markdown_with_bold_first(
    markdown_source: &str,
    lines: &mut Vec<Line<'static>>,
    config: &Config,
) {
    append_markdown_with_opener_and_cwd_and_bold(markdown_source, lines, config.file_opener, &config.cwd, true);
}

fn append_markdown_with_opener_and_cwd(
    markdown_source: &str,
    lines: &mut Vec<Line<'static>>,
    file_opener: UriBasedFileOpener,
    cwd: &Path,
) {
    append_markdown_with_opener_and_cwd_and_bold(markdown_source, lines, file_opener, cwd, false);
}

pub(crate) fn append_markdown_with_opener_and_cwd_and_bold(
    markdown_source: &str,
    lines: &mut Vec<Line<'static>>,
    file_opener: UriBasedFileOpener,
    cwd: &Path,
    bold_first_sentence: bool,
) {
    // Historically, we fed the entire `markdown_source` into the renderer in
    // one pass. However, fenced code blocks sometimes lost leading whitespace
    // when formatted by the markdown renderer/highlighter. To preserve code
    // block content exactly, split the source into "text" and "code" segments:
    // - Render non-code text through `tui_markdown` (with citation rewrite).
    // - Render code block content verbatim as plain lines without additional
    //   formatting, preserving leading spaces.
    for seg in split_text_and_fences(markdown_source) {
        match seg {
            Segment::Text(s) => {
                // Rewrite our special file citation tokens into markdown links
                // (e.g., vscode://file...). These will later be turned
                // into OSC8 hyperlinks by the markdown renderer.
                let processed = rewrite_file_citations(&s, file_opener, cwd);
                // Also rewrite web.run-style citation tokens of the form
                // "citeturn2search5" (or multiple ids like
                // "citeturn2search5turn2news1"). We do not have the
                // actual URL mapping in the TUI, so convert each ref id into
                // an inline markdown link whose target is a placeholder
                // identifier (e.g., ref:turn2search5). Our markdown renderer
                // will display these as underlined labels followed by the
                // target in parentheses, which is much nicer than leaking the
                // private-use delimiter glyphs.
                let processed = rewrite_web_citations(&processed);
                let rendered = if bold_first_sentence {
                    MarkdownRenderer::render_with_bold_first_sentence(&processed)
                } else {
                    MarkdownRenderer::render(&processed)
                };
                lines.extend(rendered);
            }
            Segment::Code { _lang, content, fenced } => {
                // Use syntect-based syntax highlighting when available, preserving exact text.
                let lang = _lang.as_deref();
                // Apply a solid background and pad trailing spaces so the block forms
                // a rectangle up to the longest line in this code section.
                let code_bg = crate::colors::code_block_bg();
                let mut highlighted = crate::syntax_highlight::highlight_code_block(&content, lang);
                // Compute max display width (in terminal cells) across lines
                let max_w: usize = highlighted
                    .iter()
                    .map(|l| l.spans.iter().map(|s| UnicodeWidthStr::width(s.content.as_ref())).sum::<usize>())
                    .max()
                    .unwrap_or(0);
                // No extra horizontal padding; use exact content width.
                let target_w = max_w;

                // When fenced and language is known, emit a hidden sentinel line so the
                // downstream renderer can surface a border + title without losing lang info.
                if fenced {
                    let label = _lang.clone().unwrap_or_else(|| "text".to_string());
                    let sentinel = format!("⟦LANG:{}⟧", label);
                    lines.push(Line::from(Span::styled(sentinel, Style::default().fg(code_bg).bg(code_bg))));
                }

                if fenced {
                    for l in highlighted.iter_mut() {
                        // Apply background to all existing spans instead of the line,
                        // so the painted region matches our explicit padding width.
                        for sp in l.spans.iter_mut() {
                            sp.style = sp.style.bg(code_bg);
                        }
                        let w: usize = l
                            .spans
                            .iter()
                            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                            .sum();
                        if target_w > w {
                            let pad = " ".repeat(target_w - w);
                            l.spans.push(Span::styled(pad, Style::default().bg(code_bg)));
                        } else if w == 0 {
                            // Defensive: paint at least one cell so background shows
                            l.spans.push(Span::styled(" ", Style::default().bg(code_bg)));
                        }
                    }
                    lines.extend(highlighted);
                } else {
                    // Non‑fenced (indented) blocks: do NOT convert to code cards.
                    // Preserve exact text and any syntax highlighting FG, but no background.
                    lines.extend(highlighted);
                }
            }
        }
    }
}

/// Rewrites file citations in `src` into markdown hyperlinks using the
/// provided `scheme` (`vscode`, `cursor`, etc.). The resulting URI follows the
/// format expected by VS Code-compatible file openers:
///
/// ```text
/// <scheme>://file<ABS_PATH>:<LINE>
/// ```
fn rewrite_file_citations<'a>(
    src: &'a str,
    file_opener: UriBasedFileOpener,
    cwd: &Path,
) -> Cow<'a, str> {
    // Map enum values to the corresponding URI scheme strings.
    let scheme: &str = match file_opener.get_scheme() {
        Some(scheme) => scheme,
        None => return Cow::Borrowed(src),
    };

    CITATION_REGEX.replace_all(src, |caps: &regex_lite::Captures<'_>| {
        let file = &caps[1];
        let start_line = &caps[2];

        // Resolve the path against `cwd` when it is relative.
        let absolute_path = {
            let p = Path::new(file);
            let absolute_path = if p.is_absolute() {
                path_clean::clean(p)
            } else {
                path_clean::clean(cwd.join(p))
            };
            // VS Code expects forward slashes even on Windows because URIs use
            // `/` as the path separator.
            absolute_path.to_string_lossy().replace('\\', "/")
        };

        // Render as a normal markdown link so the downstream renderer emits
        // the hyperlink escape sequence (when supported by the terminal).
        //
        // In practice, sometimes multiple citations for the same file, but with a
        // different line number, are shown sequentially, so we:
        // - include the line number in the label to disambiguate them
        // - add a space after the link to make it easier to read
        format!("[{file}:{start_line}]({scheme}://file{absolute_path}:{start_line}) ")
    })
}

// Convert web.run-style citations (private-use delimited) into inline markdown links.
// Examples:
//   "citeturn2search5"              -> "[turn2search5](ref:turn2search5) "
//   "citeturn2search5turn2news1" -> "[turn2search5](ref:turn2search5) [turn2news1](ref:turn2news1) "
fn rewrite_web_citations<'a>(src: &'a str) -> Cow<'a, str> {
    use once_cell::sync::OnceCell;
    use regex_lite::Regex;
    static WEB_CITE_RE: OnceCell<Regex> = OnceCell::new();
    let re = WEB_CITE_RE.get_or_init(|| Regex::new(r"cite([^]+)").expect("failed to compile web cite regex"));
    if !re.is_match(src) {
        return Cow::Borrowed(src);
    }
    Cow::Owned(re.replace_all(src, |caps: &regex_lite::Captures<'_>| {
        let inner = &caps[1];
        let parts = inner.split('').filter(|s| !s.is_empty());
        let mut out = String::new();
        for (i, id) in parts.enumerate() {
            if i > 0 { out.push(' '); }
            // Use a stable placeholder target. Our renderer will show "label (target)".
            out.push_str(&format!("[{id}](ref:{id})"));
        }
        // Add a trailing space for readability between adjacent citations and text
        out.push(' ');
        out
    }).into_owned())
}


// Minimal code block splitting.
// - Recognizes fenced blocks opened by ``` or ~~~ (allowing leading whitespace).
//   The opening fence may include a language string which we ignore.
//   The closing fence must be on its own line (ignoring surrounding whitespace).
// - Additionally recognizes indented code blocks that begin after a blank line
//   with a line starting with at least 4 spaces or a tab, and continue for
//   consecutive lines that are blank or also indented by >= 4 spaces or a tab.
enum Segment {
    Text(String),
    Code {
        _lang: Option<String>,
        content: String,
        // true when originated from ```/~~~ fenced block; false for indented
        fenced: bool,
    },
}

fn split_text_and_fences(src: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut curr_text = String::new();
    #[derive(Copy, Clone, PartialEq)]
    enum CodeMode {
        None,
        Fenced,
        Indented,
    }
    let mut code_mode = CodeMode::None;
    let mut fence_token = "";
    let mut code_lang: Option<String> = None;
    let mut code_content = String::new();
    // We intentionally do not require a preceding blank line for indented code blocks,
    // since streamed model output often omits it. This favors preserving indentation.

    for line in src.split_inclusive('\n') {
        let line_no_nl = line.strip_suffix('\n');
        let trimmed_start = match line_no_nl {
            Some(l) => l.trim_start(),
            None => line.trim_start(),
        };
        if code_mode == CodeMode::None {
            let open = if trimmed_start.starts_with("```") {
                Some("```")
            } else if trimmed_start.starts_with("~~~") {
                Some("~~~")
            } else {
                None
            };
            if let Some(tok) = open {
                // Flush pending text segment.
                if !curr_text.is_empty() {
                    segments.push(Segment::Text(curr_text.clone()));
                    curr_text.clear();
                }
                fence_token = tok;
                // Capture language after the token on this line (before newline).
                let after = &trimmed_start[tok.len()..];
                let lang = after.trim();
                code_lang = if lang.is_empty() {
                    None
                } else {
                    Some(lang.to_string())
                };
                code_mode = CodeMode::Fenced;
                code_content.clear();
                // Do not include the opening fence line in output.
                continue;
            }
            // Check for start of an indented code block: only after a blank line
            // (or at the beginning), and the line must start with >=4 spaces or a tab.
            let raw_line = match line_no_nl {
                Some(l) => l,
                None => line,
            };
            let leading_spaces = raw_line.chars().take_while(|c| *c == ' ').count();
            let starts_with_tab = raw_line.starts_with('\t');
            // Consider any line that begins with >=4 spaces or a tab to start an
            // indented code block. This favors preserving indentation even when a
            // preceding blank line is omitted (common in streamed model output).
            //
            // However, do NOT treat indented list items as code. Nested markdown lists
            // are commonly indented by 2+ spaces, and third-level bullets often cross
            // the 4‑space threshold. If the text after the indentation begins with a
            // list marker ("- ", "* ", "+ ", or an ordered list like "1. "), we
            // should render it as a list, not as an indented code block.
            let after_indent = if starts_with_tab {
                &raw_line[1..]
            } else {
                &raw_line[leading_spaces..]
            };
            let is_ordered_list = {
                // digits+". " or digits+") " are common ordered list markers
                let mut chars = after_indent.chars();
                let mut saw_digit = false;
                while let Some(c) = chars.next() {
                    if c.is_ascii_digit() { saw_digit = true; continue; }
                    if (c == '.' || c == ')') && chars.next().is_some_and(|n| n == ' ') {
                        break;
                    }
                    // Not an ordered list pattern
                    saw_digit = false; // ensure false when we break early
                    break;
                }
                saw_digit
            };
            let is_unordered_list = after_indent.starts_with("- ")
                || after_indent.starts_with("* ")
                || after_indent.starts_with("+ ");
            let looks_like_list = is_unordered_list || is_ordered_list;
            let starts_indented_code = ((leading_spaces >= 4) || starts_with_tab) && !looks_like_list;
            if starts_indented_code {
                // Flush pending text and begin an indented code block.
                if !curr_text.is_empty() {
                    segments.push(Segment::Text(curr_text.clone()));
                    curr_text.clear();
                }
                code_mode = CodeMode::Indented;
                code_content.clear();
                code_content.push_str(line);
                // Inside code now; do not treat this line as normal text.
                continue;
            }
            // Normal text line.
            curr_text.push_str(line);
        } else {
            match code_mode {
                CodeMode::Fenced => {
                    // inside fenced code: check for closing fence on its own line
                    let trimmed = match line_no_nl {
                        Some(l) => l.trim(),
                        None => line.trim(),
                    };
                    if trimmed == fence_token {
                        // End code block: emit segment without fences
                        segments.push(Segment::Code {
                            _lang: code_lang.take(),
                            content: code_content.clone(),
                            fenced: true,
                        });
                        code_content.clear();
                        code_mode = CodeMode::None;
                        fence_token = "";
                        continue;
                    }
                    // Accumulate code content exactly as-is.
                    code_content.push_str(line);
                }
                CodeMode::Indented => {
                    // Continue while the line is blank, or starts with >=4 spaces, or a tab.
                    let raw_line = match line_no_nl {
                        Some(l) => l,
                        None => line,
                    };
                    let is_blank = raw_line.trim().is_empty();
                    let leading_spaces = raw_line.chars().take_while(|c| *c == ' ').count();
                    let starts_with_tab = raw_line.starts_with('\t');
                    if is_blank || leading_spaces >= 4 || starts_with_tab {
                        code_content.push_str(line);
                    } else {
                        // Close the indented code block and reprocess this line as normal text.
                        segments.push(Segment::Code {
                            _lang: None,
                            content: code_content.clone(),
                            fenced: false,
                        });
                        code_content.clear();
                        code_mode = CodeMode::None;
                        // Now handle current line as text.
                        curr_text.push_str(line);
                    }
                }
                CodeMode::None => unreachable!(),
            }
        }
    }

    if code_mode != CodeMode::None {
        // Unterminated code fence: treat accumulated content as a code segment.
        segments.push(Segment::Code {
            _lang: code_lang.take(),
            content: code_content.clone(),
            fenced: matches!(code_mode, CodeMode::Fenced),
        });
    } else if !curr_text.is_empty() {
        segments.push(Segment::Text(curr_text.clone()));
    }

    segments
}

#[cfg(all(test, feature = "legacy_tests"))]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn citation_is_rewritten_with_absolute_path() {
        let markdown = "See 【F:/src/main.rs†L42-L50】 for details.";
        let cwd = Path::new("/workspace");
        let result = rewrite_file_citations(markdown, UriBasedFileOpener::VsCode, cwd);

        assert_eq!(
            "See [/src/main.rs:42](vscode://file/src/main.rs:42)  for details.",
            result
        );
    }

    #[test]
    fn citation_is_rewritten_with_relative_path() {
        let markdown = "Refer to 【F:lib/mod.rs†L5】 here.";
        let cwd = Path::new("/home/user/project");
        let result = rewrite_file_citations(markdown, UriBasedFileOpener::Windsurf, cwd);

        assert_eq!(
            "Refer to [lib/mod.rs:5](windsurf://file/home/user/project/lib/mod.rs:5)  here.",
            result
        );
    }

    #[test]
    fn citation_followed_by_space_so_they_do_not_run_together() {
        let markdown = "References on lines 【F:src/foo.rs†L24】【F:src/foo.rs†L42】";
        let cwd = Path::new("/home/user/project");
        let result = rewrite_file_citations(markdown, UriBasedFileOpener::VsCode, cwd);

        assert_eq!(
            "References on lines [src/foo.rs:24](vscode://file/home/user/project/src/foo.rs:24) [src/foo.rs:42](vscode://file/home/user/project/src/foo.rs:42) ",
            result
        );
    }

    #[test]
    fn citation_unchanged_without_file_opener() {
        let markdown = "Look at 【F:file.rs†L1】.";
        let cwd = Path::new("/");
        let unchanged = rewrite_file_citations(markdown, UriBasedFileOpener::VsCode, cwd);
        // The helper itself always rewrites – this test validates behaviour of
        // append_markdown when `file_opener` is None.
        let mut out = Vec::new();
        append_markdown_with_opener_and_cwd(markdown, &mut out, UriBasedFileOpener::None, cwd);
        // Convert lines back to string for comparison.
        let rendered: String = out
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.clone())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(markdown, rendered);
        // Ensure helper rewrites.
        assert_ne!(markdown, unchanged);
    }

    #[test]
    fn fenced_code_blocks_preserve_leading_whitespace() {
        let src = "```\n  indented\n\t\twith tabs\n    four spaces\n```\n";
        let cwd = Path::new("/");
        let mut out = Vec::new();
        append_markdown_with_opener_and_cwd(src, &mut out, UriBasedFileOpener::None, cwd);
        // Filter out the hidden language sentinel line
        let rendered: Vec<String> = out
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.clone())
                    .collect::<String>()
            })
            .filter(|s| !s.contains("⟦LANG:"))
            .collect();
        // Expect just the code lines (no fence markers), preserving leading whitespace.
        // We no longer inject visible padding rows here; borders/padding are applied at render time.
        let strip_line = |s: &str| s.strip_prefix(' ').unwrap_or(s).to_string();
        assert!(rendered.len() >= 3, "unexpected length: {:?}", rendered);
        assert_eq!(strip_line(&rendered[0]), "  indented");
        assert_eq!(strip_line(&rendered[1]), "\t\twith tabs");
        assert_eq!(strip_line(&rendered[2]), "    four spaces");
    }

    #[test]
    fn citations_not_rewritten_inside_code_blocks() {
        let src = "Before 【F:/x.rs†L1】\n```\nInside 【F:/x.rs†L2】\n```\nAfter 【F:/x.rs†L3】\n";
        let cwd = Path::new("/");
        let mut out = Vec::new();
        append_markdown_with_opener_and_cwd(src, &mut out, UriBasedFileOpener::VsCode, cwd);
        let rendered: Vec<String> = out
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.clone())
                    .collect::<String>()
            })
            .filter(|s| !s.contains("⟦LANG:"))
            .collect();
        // Expect first and last lines rewritten, and the interior fenced code line
        // unchanged (but wrapped with left/right padding rows).
        let strip_line = |s: &str| s.strip_prefix(' ').unwrap_or(s).trim_end().to_string();
        assert!(rendered.first().is_some_and(|s| s.contains("vscode://file")));
        assert!(rendered.iter().any(|s| strip_line(s) == "Inside 【F:/x.rs†L2】"));
        assert!(rendered.last().is_some_and(|s| s.contains("vscode://file")));
    }

    #[test]
    fn indented_code_blocks_preserve_leading_whitespace() {
        let src = "Before\n    code 1\n\tcode with tab\n        code 2\nAfter\n";
        let cwd = Path::new("/");
        let mut out = Vec::new();
        append_markdown_with_opener_and_cwd(src, &mut out, UriBasedFileOpener::None, cwd);
        let rendered: Vec<String> = out
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.clone())
                    .collect::<String>()
            })
            .filter(|s| !s.contains("⟦LANG:"))
            .collect();
        assert_eq!(
            rendered,
            vec![
                "Before".to_string(),
                "    code 1".to_string(),
                "\tcode with tab".to_string(),
                "        code 2".to_string(),
                "After".to_string()
            ]
        );
    }

    #[test]
    fn citations_not_rewritten_inside_indented_code_blocks() {
        let src = "Start 【F:/x.rs†L1】\n\n    Inside 【F:/x.rs†L2】\n\nEnd 【F:/x.rs†L3】\n";
        let cwd = Path::new("/");
        let mut out = Vec::new();
        append_markdown_with_opener_and_cwd(src, &mut out, UriBasedFileOpener::VsCode, cwd);
        let rendered: Vec<String> = out
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.clone())
                    .collect::<String>()
            })
            .filter(|s| !s.contains("⟦LANG:"))
            .collect();
        // Expect first and last lines rewritten, and the indented code line present
        // unchanged (citations inside not rewritten). We do not assert on blank
        // separator lines since the markdown renderer may normalize them.
        assert!(rendered.iter().any(|s| s.contains("vscode://file")));
        assert!(rendered.iter().any(|s| s == "    Inside 【F:/x.rs†L2】"));
    }

    #[test]
    fn append_markdown_preserves_full_text_line() {
        use codex_core::config_types::UriBasedFileOpener;
        use std::path::Path;
        let src = "Hi! How can I help with codex-rs today? Want me to explore the repo, run tests, or work on a specific change?\n";
        let cwd = Path::new("/");
        let mut out = Vec::new();
        append_markdown_with_opener_and_cwd(src, &mut out, UriBasedFileOpener::None, cwd);
        assert_eq!(
            out.len(),
            1,
            "expected a single rendered line for plain text"
        );
        let rendered: String = out
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.clone())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(
            rendered,
            "Hi! How can I help with codex-rs today? Want me to explore the repo, run tests, or work on a specific change?"
        );
    }

    #[test]
    fn fenced_code_block_with_internal_blank_line_is_one_contiguous_block() {
        // Repro from user: single fenced block with a blank line between two functions.
        // The blank line must render as part of the code block (with spaces), not as
        // a separate plain-text gap that would visually split the block.
        let src = "   ```rust\n   // Rust example\n   fn greet(name: &str) -> String {\n       format!(\"Hello, {}!\", name)\n   }\n\n   fn main() {\n       println!(\"{}\", greet(\"Codex\"));\n   }\n   ```\n";
        let cwd = Path::new("/");
        let mut out = Vec::new();
        append_markdown_with_opener_and_cwd(src, &mut out, UriBasedFileOpener::None, cwd);

        // Expect the lines to be exactly the code lines (no fence markers), including
        // one line that contains only spaces for the internal blank.
        // Extract plain strings for inspection.
        let rendered: Vec<String> = out
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.clone()).collect::<String>())
            .filter(|s| !s.contains("⟦LANG:"))
            .collect();

        // There should be at least 7 visible code lines (no border/padding rows emitted here).
        assert!(rendered.len() >= 7, "unexpected line count: {:?}", rendered);

        // Find the internal blank: it should be a line consisting only of spaces
        // (inserted by padding logic inside code block), not an actually empty string
        // produced by the non-code renderer.
        assert!(rendered.iter().any(|s| !s.is_empty() && s.trim().is_empty()),
            "expected a space-padded blank line inside the code block, got: {:?}", rendered);

        // Validate uniform rectangular width including padding rows
        use unicode_width::UnicodeWidthStr;
        let widths: Vec<usize> = out
            .iter()
            .map(|l| l.spans.iter().map(|s| UnicodeWidthStr::width(s.content.as_ref())).sum())
            .collect();
        let maxw = *widths.iter().max().unwrap_or(&0);
        assert!(widths.iter().all(|w| *w == maxw), "all lines must be padded to same width: {:?}", widths);
    }

    #[test]
    fn nested_list_items_not_treated_as_code_blocks() {
        // Repro: deeply indented third-level bullets starting with "- " should render
        // as list items, not as indented code blocks with a background.
        let src = "- What I changed\n  - data/model_data.ts\n      - Added model id gemini-2.5\n      - input_per_million: 0.30\n";
        let cwd = Path::new("/");
        let mut out = Vec::new();
        append_markdown_with_opener_and_cwd(src, &mut out, UriBasedFileOpener::None, cwd);

        // Convert to plain strings for inspection
        let rendered: Vec<String> = out
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.clone()).collect::<String>())
            .collect();

        // Expect at least three lines rendered
        assert!(rendered.len() >= 3, "unexpected rendered lines: {:?}", rendered);

        // The third-level bullets (indented by 6 spaces before "- ") should render
        // with a bullet glyph (level 4 uses '⋅') rather than the literal "- ".
        assert!(rendered.iter().any(|s| s.contains('⋅') && s.contains("Added model id")),
            "expected a rendered bullet glyph for third-level list item: {:?}", rendered);
        assert!(rendered.iter().any(|s| s.contains('⋅') && s.contains("input_per_million")),
            "expected a rendered bullet glyph for third-level list item: {:?}", rendered);
    }
}
