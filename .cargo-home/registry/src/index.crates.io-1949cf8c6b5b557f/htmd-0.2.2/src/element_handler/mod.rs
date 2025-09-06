mod anchor;
mod blockquote;
mod br;
mod code;
mod emphasis;
mod headings;
mod hr;
mod img;
mod li;
mod list;
mod table;

use crate::text_util::concat_strings;

use super::{options::Options, Element};
use anchor::AnchorElementHandler;
use blockquote::blockquote_handler;
use br::br_handler;
use code::code_handler;
use emphasis::emphasis_handler;
use headings::headings_handler;
use hr::hr_handler;
use html5ever::Attribute;
use img::img_handler;
use li::list_item_handler;
use list::list_handler;
use markup5ever_rcdom::Node;
use std::{collections::HashSet, rc::Rc};
use table::table_handler;

/// The DOM element handler.
pub trait ElementHandler: Send + Sync {
    fn append(&self) -> Option<String> {
        None
    }

    fn on_visit(
        &self,
        node: &Rc<Node>,
        tag: &str,
        attrs: &[Attribute],
        content: &str,
        options: &Options,
    ) -> Option<String>;
}

pub(crate) struct HandlerRule {
    tags: HashSet<String>,
    pub(crate) handler: Box<dyn ElementHandler>,
}

impl<F> ElementHandler for F
where
    F: (Fn(Element) -> Option<String>) + Send + Sync,
{
    fn on_visit(
        &self,
        node: &Rc<Node>,
        tag: &str,
        attrs: &[Attribute],
        content: &str,
        options: &Options,
    ) -> Option<String> {
        self(Element {
            node,
            tag,
            attrs,
            content,
            options,
        })
    }
}

/// Builtin element handlers
pub(crate) struct ElementHandlers {
    pub(crate) rules: Vec<HandlerRule>,
}

impl ElementHandlers {
    pub fn new() -> Self {
        let mut handlers = Self { rules: Vec::new() };

        // img
        handlers.add_handler(vec!["img"], img_handler);

        // a
        handlers.add_handler(vec!["a"], AnchorElementHandler::new());

        // list
        handlers.add_handler(vec!["ol", "ul"], list_handler);

        // li
        handlers.add_handler(vec!["li"], list_item_handler);

        // quote
        handlers.add_handler(vec!["blockquote"], blockquote_handler);

        // code
        handlers.add_handler(vec!["code"], code_handler);

        // strong
        handlers.add_handler(vec!["strong", "b"], bold_handler);

        // italic
        handlers.add_handler(vec!["i", "em"], italic_handler);

        // headings
        handlers.add_handler(vec!["h1", "h2", "h3", "h4", "h5", "h6"], headings_handler);

        // br
        handlers.add_handler(vec!["br"], br_handler);

        // hr
        handlers.add_handler(vec!["hr"], hr_handler);

        // table
        handlers.add_handler(vec!["table"], table_handler);

        // other block elements
        handlers.add_handler(
            vec![
                "p", "pre", "body", "div", "tr", "td", "th", "thead", "tbody", "tfoot", "header",
                "footer", "nav", "section", "article", "aside", "main", "head", "script", "style",
                "caption",
            ],
            block_handler,
        );

        handlers
    }

    pub fn add_handler<Handler>(&mut self, tags: Vec<&str>, handler: Handler)
    where
        Handler: ElementHandler + 'static,
    {
        assert!(!tags.is_empty(), "tags cannot be empty.");
        let handler = HandlerRule {
            tags: HashSet::from_iter(tags.iter().map(|tag| tag.to_string())),
            handler: Box::new(handler),
        };
        self.rules.push(handler);
    }
}

impl ElementHandler for ElementHandlers {
    fn on_visit(
        &self,
        node: &Rc<Node>,
        tag: &str,
        attrs: &[Attribute],
        content: &str,
        options: &Options,
    ) -> Option<String> {
        match self.rules.iter().rev().find(|rule| rule.tags.contains(tag)) {
            Some(rule) => rule.handler.on_visit(node, tag, attrs, content, options),
            None => Some(content.to_string()),
        }
    }
}

fn block_handler(element: Element) -> Option<String> {
    Some(concat_strings!("\n\n", element.content, "\n\n"))
}

fn bold_handler(element: Element) -> Option<String> {
    emphasis_handler(element, "**")
}

fn italic_handler(element: Element) -> Option<String> {
    emphasis_handler(element, "_")
}
