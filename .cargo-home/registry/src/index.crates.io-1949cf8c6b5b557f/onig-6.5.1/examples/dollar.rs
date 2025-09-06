use onig::{Captures, Regex, Replacer};
use std::borrow::Cow;

/// A string, with `$1` refering to the first capture group.
struct Dollarified<'a>(&'a str);

/// Capture Reference to Captured String
///
/// Tries to convert a refernece to a capture to the captured text. If
/// the reference isn't a valid numeric capture group then no text is
/// returned.
fn capture_str<'t>(caps: &'t Captures, cap_ref: &str) -> Option<&'t str> {
    cap_ref.parse::<usize>().ok().and_then(|p| caps.at(p))
}

impl<'a> Replacer for Dollarified<'a> {
    fn reg_replace(&mut self, caps: &Captures) -> Cow<str> {
        let mut replacement = String::new();
        let mut pattern = self.0;
        while !pattern.is_empty() {
            if let Some(position) = pattern.find('$') {
                // push up to the replacement
                replacement.push_str(&pattern[..position]);
                pattern = &pattern[position + 1..];

                // find the end of the capture reference
                let ref_end = pattern
                    .find(|c| !char::is_numeric(c))
                    .unwrap_or(pattern.len());

                // push the capture from this capture reference
                if let Some(cap) = capture_str(caps, &pattern[..ref_end]) {
                    replacement.push_str(cap);
                    pattern = &pattern[ref_end..];
                } else {
                    replacement.push('$');
                }
            } else {
                // no replacements left
                replacement.push_str(pattern);
                break;
            }
        }
        replacement.into()
    }
}

fn test_with(replacement: &str) {
    let re = Regex::new(r"(\w+) (\w+)").unwrap();
    let hay = "well (hello world) to you!";
    println!(
        "/{}/{}/ -> {}",
        &hay,
        &replacement,
        re.replace(hay, Dollarified(replacement))
    );
}

fn main() {
    test_with("$2 $1");
    test_with("($2 $1)");
    test_with("|$2|$1|");
    test_with("|$0|$2$1");
    test_with("$$$");
    test_with("$$$3");
    test_with("$$2$3");
    test_with("Literal replacement");
}
