use crate::{
    text_util::{concat_strings, StripWhitespace},
    Element,
};

pub(super) fn emphasis_handler(element: Element, marker: &str) -> Option<String> {
    let content = element.content;
    if content.is_empty() {
        return None;
    }
    let (content, leading_whitespace) = content.strip_leading_whitespace();
    let (content, trailing_whitespace) = content.strip_trailing_whitespace();
    if content.is_empty() {
        return None;
    }
    Some(concat_strings!(
        leading_whitespace.unwrap_or(""),
        marker,
        content,
        marker,
        trailing_whitespace.unwrap_or("")
    ))
}
