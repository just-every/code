use chrono::Local;
use chrono::Timelike;

/// Build a time-aware placeholder like
/// "What can I code for you this morning?".
pub(crate) fn greeting_placeholder() -> String {
    let hour = Local::now().hour();
    // Custom mapping: show "today" for 10:00–13:59 local time.
    let when = if (10..=13).contains(&hour) {
        "today"
    } else if (5..=9).contains(&hour) {
        "this morning"
    } else if (14..=16).contains(&hour) {
        "this afternoon"
    } else if (17..=20).contains(&hour) {
        "this evening"
    } else {
        // Late night and very early hours
        "tonight"
    };
    format!("What can I code for you {when}?")
}
