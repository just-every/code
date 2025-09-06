use crate::{options::BrStyle, Element};

pub(super) fn br_handler(element: Element) -> Option<String> {
    match element.options.br_style {
        BrStyle::TwoSpaces => Some("  \n".to_string()),
        BrStyle::Backslash => Some("\\\n".to_string()),
    }
}
