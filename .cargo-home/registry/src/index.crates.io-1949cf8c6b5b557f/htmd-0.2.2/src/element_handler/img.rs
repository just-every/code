use crate::{
    text_util::{concat_strings, JoinOnStringIterator, TrimAsciiWhitespace},
    Element,
};

pub(super) fn img_handler(element: Element) -> Option<String> {
    let mut link: Option<String> = None;
    let mut alt: Option<String> = None;
    let mut title: Option<String> = None;
    for attr in element.attrs.iter() {
        let name = &attr.name.local;
        if name == "href" {
            link = Some(attr.value.to_string())
        } else if name == "src" {
            link = Some(attr.value.to_string());
        } else if name == "alt" {
            alt = Some(attr.value.to_string());
        } else if name == "title" {
            title = Some(attr.value.to_string());
        }
    }

    link.as_ref()?;

    let process_alt_title = |text: String| {
        text.lines()
            .map(|line| line.trim_ascii_whitespace().replace('"', "\\\""))
            .filter(|line| !line.is_empty())
            .join("\n")
    };

    // Handle new lines in alt
    let alt = alt.map(process_alt_title);

    // Handle new lines in title
    let title = title.map(process_alt_title);

    let link = link.map(|text| text.replace('(', "\\(").replace(')', "\\)"));

    let has_spaces_in_link = link.as_ref().is_some_and(|link| link.contains(' '));

    let md = concat_strings!(
        "![",
        alt.as_ref().unwrap_or(&String::new()),
        "](",
        if has_spaces_in_link { "<" } else { "" },
        link.as_ref().unwrap_or(&String::new()),
        title
            .as_ref()
            .map_or(String::new(), |t| concat_strings!(" \"", t, "\"")),
        if has_spaces_in_link { ">" } else { "" },
        ")"
    );
    Some(md)
}
