use crate::{
    node_util::{get_node_tag_name, get_parent_node},
    text_util::concat_strings,
    Element,
};

pub(super) fn list_handler(element: Element) -> Option<String> {
    let parent = get_parent_node(element.node);
    let is_parent_li = parent
        .map(|p| get_node_tag_name(&p).is_some_and(|tag| tag == "li"))
        .unwrap_or(false);
    if is_parent_li {
        Some(concat_strings!(
            "\n",
            element.content.trim_matches(|ch| ch == '\n'),
            "\n"
        ))
    } else {
        Some(concat_strings!(
            "\n\n",
            element.content.trim_matches(|ch| ch == '\n'),
            "\n\n"
        ))
    }
}
