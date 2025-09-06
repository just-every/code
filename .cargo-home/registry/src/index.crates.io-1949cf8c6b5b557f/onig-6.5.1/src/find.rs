use super::{Regex, Region, SearchOptions};
use std::iter::FusedIterator;

impl Regex {
    /// Returns the capture groups corresponding to the leftmost-first match
    /// in text. Capture group `0` always corresponds to the entire match.
    /// If no match is found, then `None` is returned.
    pub fn captures<'t>(&self, text: &'t str) -> Option<Captures<'t>> {
        let mut region = Region::new();
        self.search_with_options(
            text,
            0,
            text.len(),
            SearchOptions::SEARCH_OPTION_NONE,
            Some(&mut region),
        )
        .map(|pos| Captures {
            text,
            region,
            offset: pos,
        })
    }

    /// Returns an iterator for each successive non-overlapping match in `text`,
    /// returning the start and end byte indices with respect to `text`.
    ///
    /// # Example
    ///
    /// Find the start and end location of every word with exactly 13
    /// characters:
    ///
    /// ```rust
    /// # use onig::Regex;
    /// # fn main() {
    /// let text = "Retroactively relinquishing remunerations is reprehensible.";
    /// for pos in Regex::new(r"\b\w{13}\b").unwrap().find_iter(text) {
    ///     println!("{:?}", pos);
    /// }
    /// // Output:
    /// // (0, 13)
    /// // (14, 27)
    /// // (28, 41)
    /// // (45, 58)
    /// # }
    /// ```
    pub fn find_iter<'r, 't>(&'r self, text: &'t str) -> FindMatches<'r, 't> {
        FindMatches {
            regex: self,
            region: Region::new(),
            text,
            last_end: 0,
            last_match_end: None,
        }
    }

    /// Returns an iterator over all the non-overlapping capture groups matched
    /// in `text`. This is operationally the same as `find_iter` (except it
    /// yields information about submatches).
    ///
    /// # Example
    ///
    /// We can use this to find all movie titles and their release years in
    /// some text, where the movie is formatted like "'Title' (xxxx)":
    ///
    /// ```rust
    /// # use onig::Regex;
    /// # fn main() {
    /// let re = Regex::new(r"'([^']+)'\s+\((\d{4})\)")
    ///                .unwrap();
    /// let text = "'Citizen Kane' (1941), 'The Wizard of Oz' (1939), 'M' (1931).";
    /// for caps in re.captures_iter(text) {
    ///     println!("Movie: {:?}, Released: {:?}", caps.at(1), caps.at(2));
    /// }
    /// // Output:
    /// // Movie: Citizen Kane, Released: 1941
    /// // Movie: The Wizard of Oz, Released: 1939
    /// // Movie: M, Released: 1931
    /// # }
    /// ```
    pub fn captures_iter<'r, 't>(&'r self, text: &'t str) -> FindCaptures<'r, 't> {
        FindCaptures {
            regex: self,
            text,
            last_end: 0,
            last_match_end: None,
        }
    }

    /// Returns an iterator of substrings of `text` delimited by a match
    /// of the regular expression.
    /// Namely, each element of the iterator corresponds to text that *isn't*
    /// matched by the regular expression.
    ///
    /// This method will *not* copy the text given.
    ///
    /// # Example
    ///
    /// To split a string delimited by arbitrary amounts of spaces or tabs:
    ///
    /// ```rust
    /// # use onig::Regex;
    /// # fn main() {
    /// let re = Regex::new(r"[ \t]+").unwrap();
    /// let fields: Vec<&str> = re.split("a b \t  c\td    e").collect();
    /// assert_eq!(fields, vec!("a", "b", "c", "d", "e"));
    /// # }
    /// ```
    pub fn split<'r, 't>(&'r self, text: &'t str) -> RegexSplits<'r, 't> {
        RegexSplits {
            finder: self.find_iter(text),
            last: 0,
        }
    }

    /// Returns an iterator of at most `limit` substrings of `text` delimited
    /// by a match of the regular expression. (A `limit` of `0` will return no
    /// substrings.)
    /// Namely, each element of the iterator corresponds to text that *isn't*
    /// matched by the regular expression.
    /// The remainder of the string that is not split will be the last element
    /// in the iterator.
    ///
    /// This method will *not* copy the text given.
    ///
    /// # Example
    ///
    /// Get the first two words in some text:
    ///
    /// ```rust
    /// # use onig::Regex;
    /// # fn main() {
    /// let re = Regex::new(r"\W+").unwrap();
    /// let fields: Vec<&str> = re.splitn("Hey! How are you?", 3).collect();
    /// assert_eq!(fields, vec!("Hey", "How", "are you?"));
    /// # }
    /// ```
    pub fn splitn<'r, 't>(&'r self, text: &'t str, limit: usize) -> RegexSplitsN<'r, 't> {
        RegexSplitsN {
            splits: self.split(text),
            n: limit,
        }
    }

    /// Scan the given slice, capturing into the given region and
    /// executing a callback for each match.
    pub fn scan_with_region<F>(
        &self,
        to_search: &str,
        region: &mut Region,
        options: SearchOptions,
        mut callback: F,
    ) -> i32
    where
        F: Fn(i32, i32, &Region) -> bool,
    {
        use onig_sys::{onig_scan, OnigRegion};
        use std::os::raw::{c_int, c_void};

        // Find the bounds of the string we're searching
        let start = to_search.as_ptr();
        let end = to_search[to_search.len()..].as_ptr();

        unsafe extern "C" fn scan_cb<F>(
            i: c_int,
            j: c_int,
            r: *mut OnigRegion,
            ud: *mut c_void,
        ) -> c_int
        where
            F: Fn(i32, i32, &Region) -> bool,
        {
            let region = Region::clone_from_raw(r);
            let callback = &*(ud as *mut F);
            if callback(i, j, &region) {
                0
            } else {
                -1
            }
        }

        unsafe {
            onig_scan(
                self.raw,
                start,
                end,
                (&mut region.raw) as *mut ::onig_sys::OnigRegion,
                options.bits(),
                Some(scan_cb::<F>),
                &mut callback as *mut F as *mut c_void,
            )
        }
    }

    /// Scan a Pattern and Observe Captures
    ///
    /// The scan function takes a haystack `to_search` and invokes the
    /// given `callback` for each capture of this expression.
    pub fn scan<'t, CB>(&self, to_search: &'t str, callback: CB)
    where
        CB: Fn(i32, Captures<'t>) -> bool,
    {
        let mut region = Region::new();
        self.scan_with_region(
            to_search,
            &mut region,
            SearchOptions::SEARCH_OPTION_NONE,
            |n, s, region| {
                let captures = Captures {
                    text: to_search,
                    region: region.clone(),
                    offset: s as usize,
                };
                callback(n, captures)
            },
        );
    }
}

/// Captures represents a group of captured strings for a single match.
///
/// The 0th capture always corresponds to the entire match. Each subsequent
/// index corresponds to the next capture group in the regex. Positions
/// returned from a capture group are always byte indices.
///
/// `'t` is the lifetime of the matched text.
#[derive(Debug)]
pub struct Captures<'t> {
    text: &'t str,
    region: Region,
    offset: usize,
}

impl<'t> Captures<'t> {
    /// Returns the start and end positions of the Nth capture group. Returns
    /// `None` if i is not a valid capture group or if the capture group did
    /// not match anything. The positions returned are always byte indices with
    /// respect to the original string matched.
    pub fn pos(&self, pos: usize) -> Option<(usize, usize)> {
        self.region.pos(pos)
    }

    /// Returns the matched string for the capture group `i`. If `i` isn't
    /// a valid capture group or didn't match anything, then `None` is returned.
    pub fn at(&self, pos: usize) -> Option<&'t str> {
        self.pos(pos).map(|(beg, end)| &self.text[beg..end])
    }

    /// Returns the number of captured groups.
    pub fn len(&self) -> usize {
        self.region.len()
    }

    /// Returns true if and only if there are no captured groups.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Creates an iterator of all the capture groups in order of appearance in
    /// the regular expression.
    pub fn iter(&'t self) -> SubCaptures<'t> {
        SubCaptures { idx: 0, caps: self }
    }

    /// Creates an iterator of all the capture group positions in order of
    /// appearance in the regular expression. Positions are byte indices in
    /// terms of the original string matched.
    pub fn iter_pos(&'t self) -> SubCapturesPos<'t> {
        SubCapturesPos { idx: 0, caps: self }
    }

    /// Offset of the captures within the given string slice.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

/// An iterator over capture groups for a particular match of a regular
/// expression.
///
/// `'t` is the lifetime of the matched text.
pub struct SubCaptures<'t> {
    idx: usize,
    caps: &'t Captures<'t>,
}

impl<'t> Iterator for SubCaptures<'t> {
    type Item = Option<&'t str>;

    fn next(&mut self) -> Option<Option<&'t str>> {
        if self.idx < self.caps.len() {
            self.idx += 1;
            Some(self.caps.at(self.idx - 1))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.caps.len();
        (size, Some(size))
    }

    fn count(self) -> usize {
        self.caps.len()
    }
}

impl<'t> FusedIterator for SubCaptures<'t> {}

impl<'t> ExactSizeIterator for SubCaptures<'t> {}

/// An iterator over capture group positions for a particular match of
/// a regular expression.
///
/// Positions are byte indices in terms of the original
/// string matched. `'t` is the lifetime of the matched text.
pub struct SubCapturesPos<'t> {
    idx: usize,
    caps: &'t Captures<'t>,
}

impl<'t> Iterator for SubCapturesPos<'t> {
    type Item = Option<(usize, usize)>;

    fn next(&mut self) -> Option<Option<(usize, usize)>> {
        if self.idx < self.caps.len() {
            self.idx += 1;
            Some(self.caps.pos(self.idx - 1))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.caps.len();
        (size, Some(size))
    }

    fn count(self) -> usize {
        self.caps.len()
    }
}

impl<'t> FusedIterator for SubCapturesPos<'t> {}

impl<'t> ExactSizeIterator for SubCapturesPos<'t> {}

/// An iterator over all non-overlapping matches for a particular string.
///
/// The iterator yields a tuple of integers corresponding to the start and end
/// of the match. The indices are byte offsets. The iterator stops when no more
/// matches can be found.
///
/// `'r` is the lifetime of the `Regex` struct and `'t` is the lifetime
/// of the matched string.
pub struct FindMatches<'r, 't> {
    regex: &'r Regex,
    region: Region,
    text: &'t str,
    last_end: usize,
    last_match_end: Option<usize>,
}

impl<'r, 't> Iterator for FindMatches<'r, 't> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<(usize, usize)> {
        if self.last_end > self.text.len() {
            return None;
        }
        self.region.clear();
        self.regex.search_with_options(
            self.text,
            self.last_end,
            self.text.len(),
            SearchOptions::SEARCH_OPTION_NONE,
            Some(&mut self.region),
        )?;
        let (s, e) = self.region.pos(0).unwrap();

        // Don't accept empty matches immediately following the last match.
        // i.e., no infinite loops please.
        if e == s && self.last_match_end.map_or(false, |l| l == e) {
            self.last_end += self.text[self.last_end..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
            return self.next();
        } else {
            self.last_end = e;
            self.last_match_end = Some(e);
        }

        Some((s, e))
    }
}

impl<'r, 't> FusedIterator for FindMatches<'r, 't> {}

/// An iterator that yields all non-overlapping capture groups matching a
/// particular regular expression.
///
/// The iterator stops when no more matches can be found.
///
/// `'r` is the lifetime of the `Regex` struct and `'t` is the lifetime
/// of the matched string.
pub struct FindCaptures<'r, 't> {
    regex: &'r Regex,
    text: &'t str,
    last_end: usize,
    last_match_end: Option<usize>,
}

impl<'r, 't> Iterator for FindCaptures<'r, 't> {
    type Item = Captures<'t>;

    fn next(&mut self) -> Option<Captures<'t>> {
        if self.last_end > self.text.len() {
            return None;
        }

        let mut region = Region::new();
        let r = self.regex.search_with_options(
            self.text,
            self.last_end,
            self.text.len(),
            SearchOptions::SEARCH_OPTION_NONE,
            Some(&mut region),
        )?;
        let (s, e) = region.pos(0).unwrap();

        // Don't accept empty matches immediately following the last match.
        // i.e., no infinite loops please.
        if e == s && self.last_match_end.map_or(false, |l| l == e) {
            self.last_end += self.text[self.last_end..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
            return self.next();
        } else {
            self.last_end = e;
            self.last_match_end = Some(e);
        }
        Some(Captures {
            text: self.text,
            region,
            offset: r,
        })
    }
}

impl<'r, 't> FusedIterator for FindCaptures<'r, 't> {}

/// Yields all substrings delimited by a regular expression match.
///
/// `'r` is the lifetime of the compiled expression and `'t` is the lifetime
/// of the string being split.
pub struct RegexSplits<'r, 't> {
    finder: FindMatches<'r, 't>,
    last: usize,
}

impl<'r, 't> Iterator for RegexSplits<'r, 't> {
    type Item = &'t str;

    fn next(&mut self) -> Option<&'t str> {
        let text = self.finder.text;
        match self.finder.next() {
            None => {
                if self.last >= text.len() {
                    None
                } else {
                    let s = &text[self.last..];
                    self.last = text.len();
                    Some(s)
                }
            }
            Some((s, e)) => {
                let matched = &text[self.last..s];
                self.last = e;
                Some(matched)
            }
        }
    }
}

impl<'r, 't> FusedIterator for RegexSplits<'r, 't> {}

/// Yields at most `N` substrings delimited by a regular expression match.
///
/// The last substring will be whatever remains after splitting.
///
/// `'r` is the lifetime of the compiled expression and `'t` is the lifetime
/// of the string being split.
pub struct RegexSplitsN<'r, 't> {
    splits: RegexSplits<'r, 't>,
    n: usize,
}

impl<'r, 't> Iterator for RegexSplitsN<'r, 't> {
    type Item = &'t str;

    fn next(&mut self) -> Option<&'t str> {
        if self.n == 0 {
            return None;
        }
        self.n -= 1;
        if self.n == 0 {
            let text = self.splits.finder.text;
            Some(&text[self.splits.last..])
        } else {
            self.splits.next()
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.n))
    }
}

impl<'r, 't> FusedIterator for RegexSplitsN<'r, 't> {}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn test_regex_captures() {
        let regex = Regex::new("e(l+)|(r+)").unwrap();
        let captures = regex.captures("hello").unwrap();
        assert_eq!(captures.len(), 3);
        assert_eq!(captures.is_empty(), false);
        let pos1 = captures.pos(0).unwrap();
        let pos2 = captures.pos(1).unwrap();
        let pos3 = captures.pos(2);
        assert_eq!(pos1, (1, 4));
        assert_eq!(pos2, (2, 4));
        assert_eq!(pos3, None);
        let str1 = captures.at(0).unwrap();
        let str2 = captures.at(1).unwrap();
        let str3 = captures.at(2);
        assert_eq!(str1, "ell");
        assert_eq!(str2, "ll");
        assert_eq!(str3, None);
    }

    #[test]
    fn test_regex_subcaptures() {
        let regex = Regex::new("e(l+)").unwrap();
        let captures = regex.captures("hello").unwrap();
        let caps = captures.iter().collect::<Vec<_>>();
        assert_eq!(caps[0], Some("ell"));
        assert_eq!(caps[1], Some("ll"));
        assert_eq!(caps.len(), 2);
    }

    #[test]
    fn test_regex_subcapturespos() {
        let regex = Regex::new("e(l+)").unwrap();
        let captures = regex.captures("hello").unwrap();
        let caps = captures.iter_pos().collect::<Vec<_>>();
        assert_eq!(caps[0], Some((1, 4)));
        assert_eq!(caps[1], Some((2, 4)));
        assert_eq!(caps.len(), 2);
    }

    #[test]
    fn test_find_iter() {
        let re = Regex::new(r"\d+").unwrap();
        let ms = re.find_iter("a12b2").collect::<Vec<_>>();
        assert_eq!(ms, vec![(1, 3), (4, 5)]);
    }

    #[test]
    fn test_find_iter_one_zero_length() {
        let re = Regex::new(r"\d*").unwrap();
        let ms = re.find_iter("a1b2").collect::<Vec<_>>();
        assert_eq!(ms, vec![(0, 0), (1, 2), (3, 4)]);
    }

    #[test]
    fn test_find_iter_many_zero_length() {
        let re = Regex::new(r"\d*").unwrap();
        let ms = re.find_iter("a1bbb2").collect::<Vec<_>>();
        assert_eq!(ms, vec![(0, 0), (1, 2), (3, 3), (4, 4), (5, 6)]);
    }

    #[test]
    fn test_find_iter_empty_after_match() {
        let re = Regex::new(r"b|(?=,)").unwrap();
        let ms = re.find_iter("ba,").collect::<Vec<_>>();
        assert_eq!(ms, vec![(0, 1), (2, 2)]);
    }

    #[test]
    fn test_zero_length_matches_jumps_past_match_location() {
        let re = Regex::new(r"\b").unwrap();
        let matches = re.find_iter("test string").collect::<Vec<_>>();
        assert_eq!(matches, [(0, 0), (4, 4), (5, 5), (11, 11)]);
    }

    #[test]
    fn test_captures_iter() {
        let re = Regex::new(r"\d+").unwrap();
        let ms = re.captures_iter("a12b2").collect::<Vec<_>>();
        assert_eq!(ms[0].pos(0).unwrap(), (1, 3));
        assert_eq!(ms[1].pos(0).unwrap(), (4, 5));
    }

    #[test]
    fn test_captures_stores_match_offset() {
        let reg = Regex::new(r"\d+\.(\d+)").unwrap();
        let captures = reg.captures("100 - 3.1415 / 2.0").unwrap();
        assert_eq!(6, captures.offset());
        let all_caps = reg
            .captures_iter("1 - 3234.3 * 123.2 - 100")
            .map(|cap| cap.offset())
            .collect::<Vec<_>>();
        assert_eq!(vec![4, 13], all_caps);
    }
}
