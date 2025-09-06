use std::{cell::RefCell, rc::Rc};

use html5ever::Attribute;
use markup5ever_rcdom::Node;

use crate::{
    options::{LinkReferenceStyle, LinkStyle, Options},
    text_util::{concat_strings, JoinOnStringIterator, StripWhitespace, TrimAsciiWhitespace},
};

use super::ElementHandler;

pub(super) struct AnchorElementHandler {}

impl AnchorElementHandler {
    thread_local! {
        static LINKS: RefCell<Vec<String>> = const { RefCell::new(vec![]) };
    }
}

impl ElementHandler for AnchorElementHandler {
    fn append(&self) -> Option<String> {
        AnchorElementHandler::LINKS.with(|links| {
            let mut links = links.borrow_mut();
            if links.is_empty() {
                return None;
            }
            let result = concat_strings!("\n\n", links.join("\n"), "\n\n");
            links.clear();
            Some(result)
        })
    }

    fn on_visit(
        &self,
        _node: &Rc<Node>,
        _tag: &str,
        attrs: &[Attribute],
        content: &str,
        options: &Options,
    ) -> Option<String> {
        let mut link: Option<String> = None;
        let mut title: Option<String> = None;
        for attr in attrs.iter() {
            let name = &attr.name.local;
            if name == "href" {
                link = Some(attr.value.to_string())
            } else if name == "title" {
                title = Some(attr.value.to_string());
            }
        }

        let Some(link) = link else {
            return Some(content.to_string());
        };

        let process_title = |text: String| {
            text.lines()
                .map(|line| line.trim_ascii_whitespace().replace('"', "\\\""))
                .filter(|line| !line.is_empty())
                .join("\n")
        };

        // Handle new lines in title
        let title = title.map(process_title);

        let link = link.replace('(', "\\(").replace(')', "\\)");

        let md = match options.link_style {
            LinkStyle::Inlined => self.build_inlined_anchor(content, link, title, false),
            LinkStyle::InlinedPreferAutolinks => {
                self.build_inlined_anchor(content, link, title, true)
            }
            LinkStyle::Referenced => {
                self.build_referenced_anchor(content, link, title, &options.link_reference_style)
            }
        };

        Some(md)
    }
}

impl AnchorElementHandler {
    pub(super) fn new() -> Self {
        Self {}
    }

    fn build_inlined_anchor(
        &self,
        content: &str,
        link: String,
        title: Option<String>,
        prefer_autolinks: bool,
    ) -> String {
        if prefer_autolinks && content == link {
            return concat_strings!("<", link, ">");
        }

        let has_spaces_in_link = link.contains(' ');
        let (content, _) = content.strip_leading_whitespace();
        let (content, trailing_whitespace) = content.strip_trailing_whitespace();
        concat_strings!(
            "[",
            content,
            "](",
            if has_spaces_in_link { "<" } else { "" },
            link,
            title
                .as_ref()
                .map_or(String::new(), |t| concat_strings!(" \"", t, "\"")),
            if has_spaces_in_link { ">" } else { "" },
            ")",
            trailing_whitespace.unwrap_or("")
        )
    }

    fn build_referenced_anchor(
        &self,
        content: &str,
        link: String,
        title: Option<String>,
        style: &LinkReferenceStyle,
    ) -> String {
        AnchorElementHandler::LINKS.with(|links| {
            let title = title.map_or(String::new(), |t| concat_strings!(" \"", t, "\""));
            let (current, append) = match style {
                LinkReferenceStyle::Full => {
                    let index = links.borrow().len() + 1;
                    (
                        concat_strings!("[", content, "][", index.to_string(), "]"),
                        concat_strings!("[", index.to_string(), "]: ", link, title),
                    )
                }
                LinkReferenceStyle::Collapsed => (
                    concat_strings!("[", content, "][]"),
                    concat_strings!("[", content, "]: ", link, title),
                ),
                LinkReferenceStyle::Shortcut => (
                    concat_strings!("[", content, "]"),
                    concat_strings!("[", content, "]: ", link, title),
                ),
            };
            links.borrow_mut().push(append);
            current
        })
    }
}
