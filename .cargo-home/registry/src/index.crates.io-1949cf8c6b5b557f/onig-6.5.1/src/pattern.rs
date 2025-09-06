use super::{FindMatches, Regex};
use std::str::pattern::{Pattern, SearchStep, Searcher};

/// Regex Searcher Type
///
/// Represents the state of an ongoing search over a given string
/// slice.
pub struct RegexSearcher<'r, 'a> {
    iter: FindMatches<'r, 'a>,
    pos: usize,
    hay: &'a str,
    cached_match: Option<(usize, usize)>,
}

impl<'r> Pattern for &'r Regex {
    /// Searcher Type
    ///
    /// The searcher is the type responsible for returning an iterator
    /// of matches in a given string
    type Searcher<'a> = RegexSearcher<'r, 'a>;

    /// Into Searcher
    ///
    /// Creates a new searcher instance from this `Regex` pattern
    fn into_searcher<'a>(self, haystack: &'a str) -> Self::Searcher<'a> {
        RegexSearcher::new(self, haystack)
    }
}

impl<'r, 'a> RegexSearcher<'r, 'a> {
    /// New
    ///
    /// Create a regex searcher which uses the given regex to search a
    /// given pattern.
    pub fn new(reg: &'r Regex, haystack: &'a str) -> Self {
        RegexSearcher::<'r, 'a> {
            iter: reg.find_iter(haystack),
            pos: 0,
            hay: haystack,
            cached_match: None,
        }
    }
}

unsafe impl<'r, 'a> Searcher<'a> for RegexSearcher<'r, 'a> {
    /// Haystack Accessor
    ///
    /// Return the contained reference to the haystack being searched.
    fn haystack(&self) -> &'a str {
        self.hay
    }

    /// Next
    ///
    /// Returns the indexes of the next `Match` or `Reject` of the
    /// pattern within the haystack.
    fn next(&mut self) -> SearchStep {
        // if we have a cached match then return it straight away
        if let Some((start, end)) = self.cached_match {
            self.cached_match = None;
            self.pos = end;
            return SearchStep::Match(start, end);
        }

        // If we have no more haystack to search, we are done
        if self.pos >= self.hay.len() {
            return SearchStep::Done;
        }

        // Search based on the current position
        let next = self.iter.next();

        match next {
            // we found a new match at the beginning of our slice, so
            // just return it straight away
            Some((start, end)) if start == self.pos => {
                self.pos = end;
                SearchStep::Match(start, end)
            }
            // We found a match later on in the slice. So cache it for
            // now and return a rejection up to the start of the
            // match
            Some((start, _)) => {
                self.cached_match = next;
                SearchStep::Reject(self.pos, start)
            }
            // We didn't find anything in the remainder of the
            // slice. So issue a rejection for the remaining buffer
            None => {
                let old_pos = self.pos;
                self.pos = self.hay.len();
                SearchStep::Reject(old_pos, self.pos)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::Regex;
    use std::str::pattern::{Pattern, SearchStep, Searcher};

    #[test]
    pub fn pattern_matches_in_str_returns_all_matches() {
        {
            let pattern = Regex::new("abc").unwrap();
            let v: Vec<&str> = "abcXXXabcYYYabc".matches(&pattern).collect();
            assert_eq!(v, ["abc", "abc", "abc"]);
        }
        {
            let pattern = Regex::new("a+").unwrap();
            let v: Vec<&str> = ".a..aaa.a".matches(&pattern).collect();
            assert_eq!(v, ["a", "aaa", "a"]);
        }
    }

    #[test]
    pub fn pattern_matches_with_index_returns_all_matches() {
        let pattern = Regex::new("[0-9]+").unwrap();
        let v: Vec<(usize, &str)> = "hello 1234 12.34 3".match_indices(&pattern).collect();
        assert_eq!(v, [(6, "1234"), (11, "12"), (14, "34"), (17, "3")]);
    }

    #[test]
    pub fn pattern_trim_matches_removes_matches() {
        {
            let pattern = Regex::new("a+").unwrap();
            let trimmed = "aaaaworld".trim_start_matches(&pattern);
            assert_eq!(trimmed, "world");
        }
        {
            let pattern = Regex::new("[ab]").unwrap();
            let trimmed = "aabbbababtbaest".trim_start_matches(&pattern);
            assert_eq!(trimmed, "tbaest");
        }
        {
            let pattern = Regex::new(r#"[ \t]"#).unwrap();
            let trimmed = "   \t".trim_start_matches(&pattern);
            assert_eq!(trimmed, "");
        }
    }

    #[test]
    pub fn pattern_as_searcher_returns_expected_rejections() {
        {
            let reg = Regex::new("[ab]").unwrap();
            let mut searcher = reg.into_searcher("a.b");
            assert_eq!(searcher.next(), SearchStep::Match(0, 1));
            assert_eq!(searcher.next(), SearchStep::Reject(1, 2));
            assert_eq!(searcher.next(), SearchStep::Match(2, 3));
            assert_eq!(searcher.next(), SearchStep::Done);
        }
        {
            let reg = Regex::new("test").unwrap();
            let mut searcher = reg.into_searcher("this test string");
            assert_eq!(searcher.next(), SearchStep::Reject(0, 5));
            assert_eq!(searcher.next(), SearchStep::Match(5, 9));
            assert_eq!(searcher.next(), SearchStep::Reject(9, 16));
            assert_eq!(searcher.next(), SearchStep::Done);
        }
    }

    #[test]
    pub fn pattern_match_with_empty_matches() {
        let reg = Regex::new(r"\b").unwrap();
        let matches: Vec<(usize, &str)> = "hello world".match_indices(&reg).collect();
        assert_eq!(matches, [(0, ""), (5, ""), (6, ""), (11, "")]);
    }

    #[test]
    pub fn pattern_split_with_empty_matches() {
        let reg = Regex::new(r"e?").unwrap();
        let split: Vec<&str> = "test".split(&reg).collect();
        assert_eq!(split, ["", "t", "s", "t", ""]);
    }

    #[test]
    pub fn pattern_match_prefix_returns_true_when_regex_is_prefix() {
        let pattern = Regex::new("a+").unwrap();
        assert!(pattern.is_prefix_of("aaaaaworld"));
    }
}
