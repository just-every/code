//! A simple markdown renderer widget for Ratatui.
//!
//! This module provides a simple markdown renderer widget for Ratatui. It uses the `pulldown-cmark`
//! crate to parse markdown and convert it to a `Text` widget. The `Text` widget can then be
//! rendered to the terminal using the 'Ratatui' library.
//!
#![cfg_attr(feature = "document-features", doc = "\n# Features")]
#![cfg_attr(feature = "document-features", doc = document_features::document_features!())]
//! # Example
//!
//! ~~~
//! use ratatui::text::Text;
//! use tui_markdown::from_str;
//!
//! # fn draw(frame: &mut ratatui::Frame) {
//! let markdown = r#"
//! This is a simple markdown renderer for Ratatui.
//!
//! - List item 1
//! - List item 2
//!
//! ```rust
//! fn main() {
//!     println!("Hello, world!");
//! }
//! ```
//! "#;
//!
//! let text = from_str(markdown);
//! frame.render_widget(text, frame.area());
//! # }
//! ~~~
#![allow(
    unused,
    dead_code,
    unused_variables,
    unused_imports,
    unused_mut,
    unused_assignments
)]

use std::{sync::LazyLock, vec};

#[cfg(feature = "highlight-code")]
use ansi_to_tui::IntoText;
use itertools::{Itertools, Position};
use pulldown_cmark::{
    BlockQuoteKind, CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use ratatui::{
    style::{Style, Stylize},
    symbols::line,
    text::{Line, Span, Text},
};
#[cfg(feature = "highlight-code")]
use syntect::{
    easy::HighlightLines,
    highlighting::ThemeSet,
    parsing::SyntaxSet,
    util::{as_24_bit_terminal_escaped, LinesWithEndings},
};
use tracing::{debug, debug_span, info, info_span, instrument, span, warn};

pub fn from_str(input: &str) -> Text {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(input, options);
    let mut writer = TextWriter::new(parser);
    writer.run();
    writer.text
}

struct TextWriter<'a, I> {
    /// Iterator supplying events.
    iter: I,

    /// Text to write to.
    text: Text<'a>,

    /// Current style.
    ///
    /// This is a stack of styles, with the top style being the current style.
    inline_styles: Vec<Style>,

    /// Prefix to add to the start of the each line.
    line_prefixes: Vec<Span<'a>>,

    /// Stack of line styles.
    line_styles: Vec<Style>,

    /// Used to highlight code blocks, set when  a codeblock is encountered
    #[cfg(feature = "highlight-code")]
    code_highlighter: Option<HighlightLines<'a>>,

    /// Current list index as a stack of indices.
    list_indices: Vec<Option<u64>>,

    /// A link which will be appended to the current line when the link tag is closed.
    link: Option<CowStr<'a>>,

    needs_newline: bool,
}

#[cfg(feature = "highlight-code")]
static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
#[cfg(feature = "highlight-code")]
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

impl<'a, I> TextWriter<'a, I>
where
    I: Iterator<Item = Event<'a>>,
{
    fn new(iter: I) -> Self {
        Self {
            iter,
            text: Text::default(),
            inline_styles: vec![],
            line_styles: vec![],
            line_prefixes: vec![],
            list_indices: vec![],
            needs_newline: false,
            #[cfg(feature = "highlight-code")]
            code_highlighter: None,
            link: None,
        }
    }

    fn run(&mut self) {
        debug!("Running text writer");
        while let Some(event) = self.iter.next() {
            self.handle_event(event);
        }
    }

    #[instrument(level = "debug", skip(self))]
    fn handle_event(&mut self, event: Event<'a>) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => self.text(text),
            Event::Code(code) => self.code(code),
            Event::Html(html) => warn!("Html not yet supported"),
            Event::InlineHtml(html) => warn!("Inline html not yet supported"),
            Event::FootnoteReference(_) => warn!("Footnote reference not yet supported"),
            Event::SoftBreak => self.soft_break(),
            Event::HardBreak => self.hard_break(),
            Event::Rule => warn!("Rule not yet supported"),
            Event::TaskListMarker(_) => warn!("Task list marker not yet supported"),
            Event::InlineMath(_) => warn!("Inline math not yet supported"),
            Event::DisplayMath(_) => warn!("Display math not yet supported"),
        }
    }

    fn start_tag(&mut self, tag: Tag<'a>) {
        match tag {
            Tag::Paragraph => self.start_paragraph(),
            Tag::Heading { level, .. } => self.start_heading(level),
            Tag::BlockQuote(kind) => self.start_blockquote(kind),
            Tag::CodeBlock(kind) => self.start_codeblock(kind),
            Tag::HtmlBlock => warn!("Html block not yet supported"),
            Tag::List(start_index) => self.start_list(start_index),
            Tag::Item => self.start_item(),
            Tag::FootnoteDefinition(_) => warn!("Footnote definition not yet supported"),
            Tag::Table(_) => warn!("Table not yet supported"),
            Tag::TableHead => warn!("Table head not yet supported"),
            Tag::TableRow => warn!("Table row not yet supported"),
            Tag::TableCell => warn!("Table cell not yet supported"),
            Tag::Emphasis => self.push_inline_style(Style::new().italic()),
            Tag::Strong => self.push_inline_style(Style::new().bold()),
            Tag::Strikethrough => self.push_inline_style(Style::new().crossed_out()),
            Tag::Subscript => warn!("Subscript not yet supported"),
            Tag::Superscript => warn!("Superscript not yet supported"),
            Tag::Link { dest_url, .. } => self.push_link(dest_url),
            Tag::Image { .. } => warn!("Image not yet supported"),
            Tag::MetadataBlock(_) => warn!("Metadata block not yet supported"),
            Tag::DefinitionList => warn!("Definition list not yet supported"),
            Tag::DefinitionListTitle => warn!("Definition list title not yet supported"),
            Tag::DefinitionListDefinition => warn!("Definition list definition not yet supported"),
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.end_paragraph(),
            TagEnd::Heading(_) => self.end_heading(),
            TagEnd::BlockQuote(_) => self.end_blockquote(),
            TagEnd::CodeBlock => self.end_codeblock(),
            TagEnd::HtmlBlock => {}
            TagEnd::List(is_ordered) => self.end_list(),
            TagEnd::Item => {}
            TagEnd::FootnoteDefinition => {}
            TagEnd::Table => {}
            TagEnd::TableHead => {}
            TagEnd::TableRow => {}
            TagEnd::TableCell => {}
            TagEnd::Emphasis => self.pop_inline_style(),
            TagEnd::Strong => self.pop_inline_style(),
            TagEnd::Strikethrough => self.pop_inline_style(),
            TagEnd::Subscript => {}
            TagEnd::Superscript => {}
            TagEnd::Link => self.pop_link(),
            TagEnd::Image => {}
            TagEnd::MetadataBlock(_) => {}
            TagEnd::DefinitionList => {}
            TagEnd::DefinitionListTitle => {}
            TagEnd::DefinitionListDefinition => {}
        }
    }

    fn start_paragraph(&mut self) {
        // Insert an empty line between paragraphs if there is at least one line of text already.
        if self.needs_newline {
            self.push_line(Line::default());
        }
        self.push_line(Line::default());
        self.needs_newline = false;
    }

    fn end_paragraph(&mut self) {
        self.needs_newline = true
    }

    fn start_heading(&mut self, level: HeadingLevel) {
        if self.needs_newline {
            self.push_line(Line::default());
        }
        let style = match level {
            HeadingLevel::H1 => styles::H1,
            HeadingLevel::H2 => styles::H2,
            HeadingLevel::H3 => styles::H3,
            HeadingLevel::H4 => styles::H4,
            HeadingLevel::H5 => styles::H5,
            HeadingLevel::H6 => styles::H6,
        };
        let content = format!("{} ", "#".repeat(level as usize));
        self.push_line(Line::styled(content, style));
        self.needs_newline = false;
    }

    fn end_heading(&mut self) {
        self.needs_newline = true
    }

    fn start_blockquote(&mut self, kind: Option<BlockQuoteKind>) {
        if self.needs_newline {
            self.push_line(Line::default());
            self.needs_newline = false;
        }
        self.line_prefixes.push(Span::from(">"));
        self.line_styles.push(Style::new().green());
    }

    fn end_blockquote(&mut self) {
        self.line_prefixes.pop();
        self.line_styles.pop();
        self.needs_newline = true;
    }

    fn text(&mut self, text: CowStr<'a>) {
        #[cfg(feature = "highlight-code")]
        if let Some(highlighter) = &mut self.code_highlighter {
            let text: Text = LinesWithEndings::from(&text)
                .filter_map(|line| highlighter.highlight_line(line, &SYNTAX_SET).ok())
                .filter_map(|part| as_24_bit_terminal_escaped(&part, false).into_text().ok())
                .flatten()
                .collect();

            for line in text.lines {
                self.text.push_line(line);
            }
            self.needs_newline = false;
            return;
        }

        for (position, line) in text.lines().with_position() {
            if self.needs_newline {
                self.push_line(Line::default());
                self.needs_newline = false;
            }
            if matches!(position, Position::Middle | Position::Last) {
                self.push_line(Line::default());
            }

            let style = self.inline_styles.last().copied().unwrap_or_default();

            let span = Span::styled(line.to_owned(), style);

            self.push_span(span);
        }
        self.needs_newline = false;
    }

    fn code(&mut self, code: CowStr<'a>) {
        let span = Span::styled(code, styles::CODE);
        self.push_span(span);
    }

    fn hard_break(&mut self) {
        self.push_line(Line::default());
    }

    fn start_list(&mut self, index: Option<u64>) {
        if self.list_indices.is_empty() && self.needs_newline {
            self.push_line(Line::default());
        }
        self.list_indices.push(index);
    }

    fn end_list(&mut self) {
        self.list_indices.pop();
        self.needs_newline = true;
    }

    fn start_item(&mut self) {
        self.push_line(Line::default());
        let width = self.list_indices.len() * 4 - 3;
        if let Some(last_index) = self.list_indices.last_mut() {
            let span = match last_index {
                None => Span::from(" ".repeat(width - 1) + "- "),
                Some(index) => {
                    *index += 1;
                    format!("{:width$}. ", *index - 1).light_blue()
                }
            };
            self.push_span(span);
        }
        self.needs_newline = false;
    }

    fn soft_break(&mut self) {
        self.push_line(Line::default());
    }

    fn start_codeblock(&mut self, kind: CodeBlockKind<'_>) {
        if !self.text.lines.is_empty() {
            self.push_line(Line::default());
        }
        let lang = match kind {
            CodeBlockKind::Fenced(ref lang) => lang.as_ref(),
            CodeBlockKind::Indented => "",
        };

        #[cfg(not(feature = "highlight-code"))]
        self.line_styles.push(styles::CODE);

        #[cfg(feature = "highlight-code")]
        self.set_code_highlighter(lang);

        let span = Span::from(format!("```{lang}"));
        self.push_line(span.into());
        self.needs_newline = true;
    }

    fn end_codeblock(&mut self) {
        let span = Span::from("```");
        self.push_line(span.into());
        self.needs_newline = true;

        #[cfg(not(feature = "highlight-code"))]
        self.line_styles.pop();

        #[cfg(feature = "highlight-code")]
        self.clear_code_highlighter();
    }

    #[cfg(feature = "highlight-code")]
    #[instrument(level = "trace", skip(self))]
    fn set_code_highlighter(&mut self, lang: &str) {
        if let Some(syntax) = SYNTAX_SET.find_syntax_by_token(lang) {
            debug!("Starting code block with syntax: {:?}", lang);
            let theme = &THEME_SET.themes["base16-ocean.dark"];
            let highlighter = HighlightLines::new(syntax, theme);
            self.code_highlighter = Some(highlighter);
        } else {
            warn!("Could not find syntax for code block: {:?}", lang);
        }
    }

    #[cfg(feature = "highlight-code")]
    #[instrument(level = "trace", skip(self))]
    fn clear_code_highlighter(&mut self) {
        self.code_highlighter = None;
    }

    #[instrument(level = "trace", skip(self))]
    fn push_inline_style(&mut self, style: Style) {
        let current_style = self.inline_styles.last().copied().unwrap_or_default();
        let style = current_style.patch(style);
        self.inline_styles.push(style);
        debug!("Pushed inline style: {:?}", style);
        debug!("Current inline styles: {:?}", self.inline_styles);
    }

    #[instrument(level = "trace", skip(self))]
    fn pop_inline_style(&mut self) {
        self.inline_styles.pop();
    }

    #[instrument(level = "trace", skip(self))]
    fn push_line(&mut self, line: Line<'a>) {
        let style = self.line_styles.last().copied().unwrap_or_default();
        let mut line = line.patch_style(style);

        // Add line prefixes to the start of the line.
        let line_prefixes = self.line_prefixes.iter().cloned().collect_vec();
        let has_prefixes = !line_prefixes.is_empty();
        if has_prefixes {
            line.spans.insert(0, " ".into());
        }
        for prefix in line_prefixes.iter().rev().cloned() {
            line.spans.insert(0, prefix);
        }
        self.text.lines.push(line);
    }

    #[instrument(level = "trace", skip(self))]
    fn push_span(&mut self, span: Span<'a>) {
        if let Some(line) = self.text.lines.last_mut() {
            line.push_span(span);
        } else {
            self.push_line(Line::from(vec![span]));
        }
    }

    /// Store the link to be appended to the link text
    #[instrument(level = "trace", skip(self))]
    fn push_link(&mut self, dest_url: CowStr<'a>) {
        self.link = Some(dest_url);
    }

    /// Append the link to the current line
    #[instrument(level = "trace", skip(self))]
    fn pop_link(&mut self) {
        if let Some(link) = self.link.take() {
            self.push_span(" (".into());
            self.push_span(Span::styled(link, styles::LINK));
            self.push_span(")".into());
        }
    }
}

mod styles {
    use ratatui::style::{Color, Modifier, Style, Stylize};

    pub const H1: Style = Style::new()
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
        .add_modifier(Modifier::UNDERLINED);
    pub const H2: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    pub const H3: Style = Style::new()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
        .add_modifier(Modifier::ITALIC);
    pub const H4: Style = Style::new()
        .fg(Color::LightCyan)
        .add_modifier(Modifier::ITALIC);
    pub const H5: Style = Style::new()
        .fg(Color::LightCyan)
        .add_modifier(Modifier::ITALIC);
    pub const H6: Style = Style::new()
        .fg(Color::LightCyan)
        .add_modifier(Modifier::ITALIC);
    pub const BLOCKQUOTE: Style = Style::new().fg(Color::Green);
    pub const CODE: Style = Style::new().fg(Color::White).bg(Color::Black);
    pub const LINK: Style = Style::new()
        .fg(Color::Blue)
        .add_modifier(Modifier::UNDERLINED);
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use pretty_assertions::assert_eq;
    use rstest::{fixture, rstest};
    use tracing::{
        level_filters::LevelFilter,
        subscriber::{self, DefaultGuard},
    };
    use tracing_subscriber::fmt::{format::FmtSpan, time::Uptime};

    use super::*;

    #[fixture]
    fn with_tracing() -> DefaultGuard {
        let subscriber = tracing_subscriber::fmt()
            .with_test_writer()
            .with_timer(Uptime::default())
            .with_max_level(LevelFilter::TRACE)
            .with_span_events(FmtSpan::ENTER)
            .finish();
        subscriber::set_default(subscriber)
    }

    #[rstest]
    fn empty(with_tracing: DefaultGuard) {
        assert_eq!(from_str(""), Text::default());
    }

    #[rstest]
    fn paragraph_single(with_tracing: DefaultGuard) {
        assert_eq!(from_str("Hello, world!"), Text::from("Hello, world!"));
    }

    #[rstest]
    fn paragraph_soft_break(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(indoc! {"
                Hello
                World
            "}),
            Text::from_iter(["Hello", "World"])
        );
    }

    #[rstest]
    fn paragraph_multiple(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(indoc! {"
                Paragraph 1
                
                Paragraph 2
            "}),
            Text::from_iter(["Paragraph 1", "", "Paragraph 2",])
        );
    }

    #[rstest]
    fn headings(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(indoc! {"
                # Heading 1
                ## Heading 2
                ### Heading 3
                #### Heading 4
                ##### Heading 5
                ###### Heading 6
            "}),
            Text::from_iter([
                Line::from_iter(["# ", "Heading 1"]).style(styles::H1),
                Line::default(),
                Line::from_iter(["## ", "Heading 2"]).style(styles::H2),
                Line::default(),
                Line::from_iter(["### ", "Heading 3"]).style(styles::H3),
                Line::default(),
                Line::from_iter(["#### ", "Heading 4"]).style(styles::H4),
                Line::default(),
                Line::from_iter(["##### ", "Heading 5"]).style(styles::H5),
                Line::default(),
                Line::from_iter(["###### ", "Heading 6"]).style(styles::H6),
            ])
        );
    }

    /// I was having difficulty getting the right number of newlines between paragraphs, so this
    /// test is to help debug and ensure that.
    #[rstest]
    fn blockquote_after_paragraph(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(indoc! {"
                Hello, world!

                > Blockquote
            "}),
            Text::from_iter([
                Line::from("Hello, world!"),
                Line::default(),
                Line::from_iter([">", " ", "Blockquote"]).style(styles::BLOCKQUOTE),
            ])
        );
    }
    #[rstest]
    fn blockquote_single(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("> Blockquote"),
            Text::from(Line::from_iter([">", " ", "Blockquote"]).style(styles::BLOCKQUOTE))
        );
    }

    #[rstest]
    fn blockquote_soft_break(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(indoc! {"
                > Blockquote 1
                > Blockquote 2
            "}),
            Text::from_iter([
                Line::from_iter([">", " ", "Blockquote 1"]).style(styles::BLOCKQUOTE),
                Line::from_iter([">", " ", "Blockquote 2"]).style(styles::BLOCKQUOTE),
            ])
        );
    }

    #[rstest]
    fn blockquote_multiple(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(indoc! {"
                > Blockquote 1
                >
                > Blockquote 2
            "}),
            Text::from_iter([
                Line::from_iter([">", " ", "Blockquote 1"]).style(styles::BLOCKQUOTE),
                Line::from_iter([">", " "]).style(styles::BLOCKQUOTE),
                Line::from_iter([">", " ", "Blockquote 2"]).style(styles::BLOCKQUOTE),
            ])
        );
    }

    #[rstest]
    fn blockquote_multiple_with_break(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(indoc! {"
                > Blockquote 1

                > Blockquote 2
            "}),
            Text::from_iter([
                Line::from_iter([">", " ", "Blockquote 1"]).style(styles::BLOCKQUOTE),
                Line::default(),
                Line::from_iter([">", " ", "Blockquote 2"]).style(styles::BLOCKQUOTE),
            ])
        );
    }

    #[rstest]
    fn blockquote_nested(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(indoc! {"
                > Blockquote 1
                >> Nested Blockquote
            "}),
            Text::from_iter([
                Line::from_iter([">", " ", "Blockquote 1"]).style(styles::BLOCKQUOTE),
                Line::from_iter([">", " "]).style(styles::BLOCKQUOTE),
                Line::from_iter([">", ">", " ", "Nested Blockquote"]).style(styles::BLOCKQUOTE),
            ])
        );
    }

    #[rstest]
    fn list_single(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(indoc! {"
                - List item 1
            "}),
            Text::from_iter([Line::from_iter(["- ", "List item 1"])])
        );
    }

    #[rstest]
    fn list_multiple(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(indoc! {"
                - List item 1
                - List item 2
            "}),
            Text::from_iter([
                Line::from_iter(["- ", "List item 1"]),
                Line::from_iter(["- ", "List item 2"]),
            ])
        );
    }

    #[rstest]
    fn list_ordered(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(indoc! {"
                1. List item 1
                2. List item 2
            "}),
            Text::from_iter([
                Line::from_iter(["1. ".light_blue(), "List item 1".into()]),
                Line::from_iter(["2. ".light_blue(), "List item 2".into()]),
            ])
        );
    }

    #[rstest]
    fn list_nested(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str(indoc! {"
                - List item 1
                  - Nested list item 1
            "}),
            Text::from_iter([
                Line::from_iter(["- ", "List item 1"]),
                Line::from_iter(["    - ", "Nested list item 1"]),
            ])
        );
    }

    #[cfg_attr(not(feature = "highlight-code"), ignore)]
    #[rstest]
    fn highlighted_code(with_tracing: DefaultGuard) {
        // Assert no extra newlines are added
        let highlighted_code = from_str(indoc! {"
            ```rust
            fn main() {
                println!(\"Hello, highlighted code!\");
            }
            ```"});

        insta::assert_snapshot!(highlighted_code);
        insta::assert_debug_snapshot!(highlighted_code);
    }

    #[cfg_attr(not(feature = "highlight-code"), ignore)]
    #[rstest]
    fn highlighted_code_with_indentation(with_tracing: DefaultGuard) {
        // Assert no extra newlines are added
        let highlighted_code_indented = from_str(indoc! {"
            ```rust
            fn main() {
                // This is a comment
                HelloWorldBuilder::new()
                    .with_text(\"Hello, highlighted code!\")
                    .build()
                    .show();
                            
            }
            ```"});

        insta::assert_snapshot!(highlighted_code_indented);
        insta::assert_debug_snapshot!(highlighted_code_indented);
    }

    #[cfg_attr(feature = "highlight-code", ignore)]
    #[rstest]
    fn unhighlighted_code(with_tracing: DefaultGuard) {
        // Assert no extra newlines are added
        let unhiglighted_code = from_str(indoc! {"
            ```rust
            fn main() {
                println!(\"Hello, unhighlighted code!\");
            }
            ```"});

        insta::assert_snapshot!(unhiglighted_code);

        // Code highlighting is complex, assert on on the debug snapshot
        insta::assert_debug_snapshot!(unhiglighted_code);
    }

    #[rstest]
    fn inline_code(with_tracing: DefaultGuard) {
        let text = from_str("Example of `Inline code`");
        insta::assert_snapshot!(text);

        assert_eq!(
            text,
            Line::from_iter([
                Span::from("Example of "),
                Span::styled("Inline code", styles::CODE)
            ])
            .into()
        );
    }

    #[rstest]
    fn strong(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("**Strong**"),
            Text::from(Line::from("Strong".bold()))
        );
    }

    #[rstest]
    fn emphasis(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("*Emphasis*"),
            Text::from(Line::from("Emphasis".italic()))
        );
    }

    #[rstest]
    fn strikethrough(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("~~Strikethrough~~"),
            Text::from(Line::from("Strikethrough".crossed_out()))
        );
    }

    #[rstest]
    fn strong_emphasis(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("**Strong *emphasis***"),
            Text::from(Line::from_iter([
                "Strong ".bold(),
                "emphasis".bold().italic()
            ]))
        );
    }

    #[rstest]
    fn link(with_tracing: DefaultGuard) {
        assert_eq!(
            from_str("[Link](https://example.com)"),
            Text::from(Line::from_iter([
                Span::from("Link"),
                Span::from(" ("),
                Span::from("https://example.com").blue().underlined(),
                Span::from(")")
            ]))
        );
    }
}
