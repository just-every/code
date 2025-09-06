use super::{Captures, Regex};
use std::borrow::Cow;

/// Replacer describes types that can be used to replace matches in a string.
///
/// Implementations are provided for replacement using string literals
/// and `FnMut` callbacks. If this isn't enough for your replacement
/// needs a user-supplied `Replacer` implemenation can be
/// provided. For an example of a custom replacer implementation check
/// out `examples/dollar.rs` in the Onig crate.
pub trait Replacer {
    /// Returns a possibly owned string that is used to replace the match
    /// corresponding to the `caps` capture group.
    fn reg_replace(&mut self, caps: &Captures) -> Cow<str>;
}

/// Replacement using Literal Strings
impl<'t> Replacer for &'t str {
    fn reg_replace(&mut self, _: &Captures) -> Cow<str> {
        (*self).into()
    }
}

/// Replacement using `FnMut` Callbacks
impl<F> Replacer for F
where
    F: FnMut(&Captures) -> String,
{
    fn reg_replace<'a>(&'a mut self, caps: &Captures) -> Cow<'a, str> {
        (*self)(caps).into()
    }
}

impl Regex {
    /// Replaces the leftmost-first match with the replacement provided.
    /// The replacement can be a regular string or a function that takes
    /// the matches `Captures` and returns the replaced string.
    ///
    /// If no match is found, then a copy of the string is returned unchanged.
    ///
    /// # Examples
    ///
    /// Note that this function is polymorphic with respect to the replacement.
    /// In typical usage, this can just be a normal string:
    ///
    /// ```rust
    /// # use onig::Regex;
    /// # fn main() {
    /// let re = Regex::new("[^01]+").unwrap();
    /// assert_eq!(re.replace("1078910", ""), "1010");
    /// # }
    /// ```
    ///
    /// But anything satisfying the `Replacer` trait will work. For example,
    /// a closure of type `|&Captures| -> String` provides direct access to the
    /// captures corresponding to a match. This allows one to access
    /// submatches easily:
    ///
    /// ```rust
    /// # use onig::Regex;
    /// # use onig::Captures; fn main() {
    /// let re = Regex::new(r"([^,\s]+),\s+(\S+)").unwrap();
    /// let result = re.replace("Springsteen, Bruce", |caps: &Captures| {
    ///     format!("{} {}", caps.at(2).unwrap_or(""), caps.at(1).unwrap_or(""))
    /// });
    /// assert_eq!(result, "Bruce Springsteen");
    /// # }
    /// ```
    pub fn replace<R: Replacer>(&self, text: &str, rep: R) -> String {
        self.replacen(text, 1, rep)
    }

    /// Replaces all non-overlapping matches in `text` with the
    /// replacement provided. This is the same as calling `replacen` with
    /// `limit` set to `0`.
    ///
    /// See the documentation for `replace` for details on how to access
    /// submatches in the replacement string.
    pub fn replace_all<R: Replacer>(&self, text: &str, rep: R) -> String {
        self.replacen(text, 0, rep)
    }

    /// Replaces at most `limit` non-overlapping matches in `text` with the
    /// replacement provided. If `limit` is 0, then all non-overlapping matches
    /// are replaced.
    ///
    /// See the documentation for `replace` for details on how to access
    /// submatches in the replacement string.
    pub fn replacen<R: Replacer>(&self, text: &str, limit: usize, mut rep: R) -> String {
        let mut new = String::with_capacity(text.len());
        let mut last_match = 0;
        for (i, cap) in self.captures_iter(text).enumerate() {
            if limit > 0 && i >= limit {
                break;
            }
            // unwrap on 0 is OK because captures only reports matches
            let (s, e) = cap.pos(0).unwrap();
            new.push_str(&text[last_match..s]);
            new.push_str(&rep.reg_replace(&cap));
            last_match = e;
        }
        new.push_str(&text[last_match..]);
        new
    }
}
