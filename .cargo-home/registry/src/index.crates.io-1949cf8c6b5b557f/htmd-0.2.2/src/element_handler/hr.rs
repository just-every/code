use crate::{options::HrStyle, Element};

pub(super) fn hr_handler(element: Element) -> Option<String> {
    match element.options.hr_style {
        HrStyle::Dashes => Some("\n\n- - -\n\n".to_string()),
        HrStyle::Asterisks => Some("\n\n* * *\n\n".to_string()),
        HrStyle::Underscores => Some("\n\n_ _ _\n\n".to_string()),
    }
}
