//! This crate provides a safe wrapper around the
//! [Oniguruma](https://github.com/kkos/oniguruma) regular expression library.
//!
//! # Examples
//!
//! ```rust
//! use onig::Regex;
//!
//! let regex = Regex::new("e(l+)").unwrap();
//! for (i, pos) in regex.captures("hello").unwrap().iter_pos().enumerate() {
//!     match pos {
//!          Some((beg, end)) =>
//!              println!("Group {} captured in position {}:{}", i, beg, end),
//!          None =>
//!              println!("Group {} is not captured", i)
//!     }
//! }
//! ```
//!
//! # Match vs Search
//!
//! There are two basic things you can do with a `Regex` pattern; test
//! if the pattern matches the whole of a given string, and search for
//! occurences of the pattern within a string. Oniguruma exposes these
//! two concepts with the *match* and *search* APIs.
//!
//! In addition two these two base Onigurma APIs this crate exposes a
//! third *find* API, built on top of the *search* API.
//!
//! ```
//! # use onig::Regex;
//! let pattern = Regex::new("hello").unwrap();
//! assert_eq!(true, pattern.find("hello world").is_some());
//! assert_eq!(false, pattern.is_match("hello world"));
//! ```
//!
//! ## The *Match* API
//!
//! Functions in the match API check if a pattern matches the entire
//! string. The simplest of these is `Regex::is_match`. This retuns a
//! `true` if the pattern matches the string. For more complex useage
//! then `Regex::match_with_options` and `Regex::match_with_encoding`
//! can be used. These allow the capture groups to be inspected,
//! matching with different options, and matching sub-sections of a
//! given text.
//!
//! ## The *Search* API
//!
//! Function in the search API search for a pattern anywhere within a
//! string. The simplist of these is `Regex::find`. This returns the
//! offset of the first occurence of the pattern within the string.
//! For more complex useage `Regex::search_with_options` and
//! `Regex::search_with_encoding` can be used. These allow capture
//! groups to be inspected, searching with different options and
//! searching within subsections of a given text.
//!
//! ## The *Find* API
//!
//! The find API is built on top of the search API. Functions in this
//! API allow iteration across all matches of the pattern within a
//! string, not just the first one. The functions deal with some of
//! the complexities of this, such as zero-length matches.
//!
//! The simplest step-up from the basic search API `Regex::find` is
//! getting the captures relating to a match with the
//! `Regex::captures` method. To find capture information for all
//! matches within a string `Regex::find_iter` and
//! `Regex::captures_iter` can be used. The former exposes the start
//! and end of the match as `Regex::find` does, the latter exposes the
//! whole capture group information as `Regex::captures` does.
//!
//! # The `std::pattern` API
//!
//! In addition to the main Oniguruma API it is possible to use the
//! `Regex` object with the
//! [`std::pattern`](https://doc.rust-lang.org/std/str/pattern/)
//! API. To enable support compile with the `std-pattern` feature. If
//! you're using Cargo you can do this by adding the following to your
//! Cargo.toml:
//!
//! ```toml
//! [dependencies.onig]
//! version = "1.2"
//! features = ["std-pattern"]
//! ```

#![cfg_attr(feature = "std-pattern", feature(pattern))]
#![deny(missing_docs)]

use once_cell::sync::Lazy;

mod buffers;
mod find;
mod flags;
mod match_param;
mod names;
mod region;
mod replace;
mod syntax;
mod tree;
mod utils;

#[cfg(feature = "std-pattern")]
mod pattern;

// re-export the onig types publically
pub use crate::buffers::{EncodedBytes, EncodedChars};
pub use crate::find::{
    Captures, FindCaptures, FindMatches, RegexSplits, RegexSplitsN, SubCaptures, SubCapturesPos,
};
pub use crate::flags::*;
pub use crate::match_param::MatchParam;
pub use crate::region::Region;
pub use crate::replace::Replacer;
pub use crate::syntax::{MetaChar, Syntax};
pub use crate::tree::{CaptureTreeNode, CaptureTreeNodeIter};
pub use crate::utils::{copyright, define_user_property, version};

use std::os::raw::c_int;
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use std::{error, fmt, str};

#[derive(Debug)]
enum ErrorData {
    OnigError(c_int),
    Custom,
}

/// This struture represents an error from the underlying Oniguruma libray.
pub struct Error {
    data: ErrorData,
    description: String,
}

/// This struct is a wrapper around an Oniguruma regular expression
/// pointer. This represents a compiled regex which can be used in
/// search and match operations.
#[derive(Debug, Eq, PartialEq)]
pub struct Regex {
    raw: onig_sys::OnigRegex,
}

unsafe impl Send for Regex {}
unsafe impl Sync for Regex {}

impl Error {
    fn from_code_and_info(code: c_int, info: &onig_sys::OnigErrorInfo) -> Self {
        Error::new(code, info)
    }

    fn from_code(code: c_int) -> Self {
        Error::new(code, null())
    }

    fn custom<T: Into<String>>(message: T) -> Self {
        Error {
            data: ErrorData::Custom,
            description: message.into(),
        }
    }

    fn new(code: c_int, info: *const onig_sys::OnigErrorInfo) -> Self {
        let buff = &mut [0; onig_sys::ONIG_MAX_ERROR_MESSAGE_LEN as usize];
        let len = unsafe { onig_sys::onig_error_code_to_str(buff.as_mut_ptr(), code, info) };
        let description = if let Ok(description) = str::from_utf8(&buff[..len as usize]) {
            description
        } else {
            return Self::custom("Onig error string was invalid UTF-8");
        };
        Error {
            data: ErrorData::OnigError(code),
            description: description.to_owned(),
        }
    }

    /// Return Oniguruma engine error code.
    pub fn code(&self) -> i32 {
        match self.data {
            ErrorData::OnigError(code) => code,
            _ => -1,
        }
    }

    /// Return error description provided by Oniguruma engine.
    pub fn description(&self) -> &str {
        &self.description
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        &self.description
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Oniguruma error: {}", self.description())
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Error({:?}, {})", self.data, self.description())
    }
}

static REGEX_NEW_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

impl Regex {
    /// Create a Regex
    ///
    /// Simple regular expression constructor. Compiles a new regular
    /// expression with the default options using the ruby syntax.
    /// Once compiled, it can be used repeatedly to search in a string. If an
    /// invalid expression is given, then an error is returned.
    ///
    /// # Arguments
    ///
    /// * `pattern` - The regex pattern to compile
    ///
    /// # Examples
    ///
    /// ```
    /// use onig::Regex;
    /// let r = Regex::new(r#"hello (\w+)"#);
    /// assert!(r.is_ok());
    /// ```
    pub fn new(pattern: &str) -> Result<Self, Error> {
        Regex::with_encoding(pattern)
    }

    /// Create a Regex, Specifying an Encoding
    ///
    /// Attempts to compile `pattern` into a new `Regex`
    /// instance. Instead of assuming UTF-8 as the encoding scheme the
    /// encoding is inferred from the `pattern` buffer.
    ///
    /// # Arguments
    ///
    /// * `pattern` - The regex pattern to compile
    ///
    /// # Examples
    ///
    /// ```
    /// use onig::{Regex, EncodedBytes};
    /// let utf8 = Regex::with_encoding("hello");
    /// assert!(utf8.is_ok());
    /// let ascii = Regex::with_encoding(EncodedBytes::ascii(b"world"));
    /// assert!(ascii.is_ok());
    /// ```
    pub fn with_encoding<T>(pattern: T) -> Result<Regex, Error>
    where
        T: EncodedChars,
    {
        Regex::with_options_and_encoding(
            pattern,
            RegexOptions::REGEX_OPTION_NONE,
            Syntax::default(),
        )
    }

    /// Create a new Regex
    ///
    /// Attempts to compile a pattern into a new `Regex` instance.
    /// Once compiled, it can be used repeatedly to search in a string. If an
    /// invalid expression is given, then an error is returned.
    /// See [`onig_sys::onig_new`][regex_new] for more information.
    ///
    /// # Arguments
    ///
    ///  * `pattern` - The regex pattern to compile.
    ///  * `options` - The regex compilation options.
    ///  * `syntax`  - The syntax which the regex is written in.
    ///
    /// # Examples
    ///
    /// ```
    /// use onig::{Regex, Syntax, RegexOptions};
    /// let r = Regex::with_options("hello.*world",
    ///                             RegexOptions::REGEX_OPTION_NONE,
    ///                             Syntax::default());
    /// assert!(r.is_ok());
    /// ```
    ///
    /// [regex_new]: ./onig_sys/fn.onig_new.html
    pub fn with_options(
        pattern: &str,
        option: RegexOptions,
        syntax: &Syntax,
    ) -> Result<Regex, Error> {
        Regex::with_options_and_encoding(pattern, option, syntax)
    }

    /// Create a new Regex, Specifying Options and Ecoding
    ///
    /// Attempts to comile the given `pattern` into a new `Regex`
    /// instance. Instead of assuming UTF-8 as the encoding scheme the
    /// encoding is inferred from the `pattern` buffer. If the regex
    /// fails to compile the returned `Error` value from
    /// [`onig_new`][regex_new] contains more information.
    ///
    /// [regex_new]: ./onig_sys/fn.onig_new.html
    ///
    /// # Arguments
    ///
    ///  * `pattern` - The regex pattern to compile.
    ///  * `options` - The regex compilation options.
    ///  * `syntax`  - The syntax which the regex is written in.
    ///
    /// # Examples
    /// ```
    /// use onig::{Regex, Syntax, EncodedBytes, RegexOptions};
    /// let pattern = EncodedBytes::ascii(b"hello");
    /// let r = Regex::with_options_and_encoding(pattern,
    ///                                          RegexOptions::REGEX_OPTION_SINGLELINE,
    ///                                          Syntax::default());
    /// assert!(r.is_ok());
    /// ```
    pub fn with_options_and_encoding<T>(
        pattern: T,
        option: RegexOptions,
        syntax: &Syntax,
    ) -> Result<Self, Error>
    where
        T: EncodedChars,
    {
        // Convert the rust types to those required for the call to
        // `onig_new`.
        let mut reg: onig_sys::OnigRegex = null_mut();
        let reg_ptr = &mut reg as *mut onig_sys::OnigRegex;

        // We can use this later to get an error message to pass back
        // if regex creation fails.
        let mut error = onig_sys::OnigErrorInfo {
            enc: null_mut(),
            par: null_mut(),
            par_end: null_mut(),
        };

        let err = unsafe {
            // Grab a lock to make sure that `onig_new` isn't called by
            // more than one thread at a time.
            let _guard = REGEX_NEW_MUTEX.lock().unwrap();
            onig_sys::onig_new(
                reg_ptr,
                pattern.start_ptr(),
                pattern.limit_ptr(),
                option.bits(),
                pattern.encoding(),
                syntax as *const Syntax as *mut Syntax as *mut onig_sys::OnigSyntaxType,
                &mut error,
            )
        };

        if err == onig_sys::ONIG_NORMAL as i32 {
            Ok(Regex { raw: reg })
        } else {
            Err(Error::from_code_and_info(err, &error))
        }
    }

    /// Match String
    ///
    /// Try to match the regex against the given string slice,
    /// starting at a given offset. This method works the same way as
    /// `match_with_encoding`, but the encoding is always utf-8.
    ///
    /// For more information see [Match vs
    /// Search](index.html#match-vs-search)
    ///
    /// # Arguments
    ///
    /// * `str` - The string slice to match against.
    /// * `at` - The byte index in the passed slice to start matching
    /// * `options` - The regex match options.
    /// * `region` - The region for return group match range info
    ///
    /// # Returns
    ///
    /// `Some(len)` if the regex matched, with `len` being the number
    /// of bytes matched. `None` if the regex doesn't match.
    ///
    /// # Examples
    ///
    /// ```
    /// use onig::{Regex, SearchOptions};
    ///
    /// let r = Regex::new(".*").unwrap();
    /// let res = r.match_with_options("hello", 0, SearchOptions::SEARCH_OPTION_NONE, None);
    /// assert!(res.is_some()); // it matches
    /// assert!(res.unwrap() == 5); // 5 characters matched
    /// ```
    pub fn match_with_options(
        &self,
        str: &str,
        at: usize,
        options: SearchOptions,
        region: Option<&mut Region>,
    ) -> Option<usize> {
        self.match_with_encoding(str, at, options, region)
    }

    /// Match String with Encoding
    ///
    /// Match the regex against a string. This method will start at
    /// the offset `at` into the string and try and match the
    /// regex. If the regex matches then the return value is the
    /// number of characters which matched. If the regex doesn't match
    /// the return is `None`.
    ///
    /// For more information see [Match vs
    /// Search](index.html#match-vs-search)
    ///
    /// The contents of `chars` must have the same encoding that was
    /// used to construct the regex.
    ///
    /// # Arguments
    ///
    /// * `chars` - The buffer to match against.
    /// * `at` - The byte index in the passed buffer to start matching
    /// * `options` - The regex match options.
    /// * `region` - The region for return group match range info
    ///
    /// # Returns
    ///
    /// `Some(len)` if the regex matched, with `len` being the number
    /// of bytes matched. `None` if the regex doesn't match.
    ///
    /// # Examples
    ///
    /// ```
    /// use onig::{Regex, EncodedBytes, SearchOptions};
    ///
    /// let r = Regex::with_encoding(EncodedBytes::ascii(b".*")).unwrap();
    /// let res = r.match_with_encoding(EncodedBytes::ascii(b"world"),
    ///                                 0, SearchOptions::SEARCH_OPTION_NONE, None);
    /// assert!(res.is_some()); // it matches
    /// assert!(res.unwrap() == 5); // 5 characters matched
    /// ```
    pub fn match_with_encoding<T>(
        &self,
        chars: T,
        at: usize,
        options: SearchOptions,
        region: Option<&mut Region>,
    ) -> Option<usize>
    where
        T: EncodedChars,
    {
        let match_param = MatchParam::default();
        let result = self.match_with_param(chars, at, options, region, match_param);

        match result {
            Ok(r) => r,
            Err(e) => panic!("Onig: Regex match error: {}", e.description()),
        }
    }

    /// Match string with encoding and match param
    ///
    /// Match the regex against a string. This method will start at
    /// the offset `at` into the string and try and match the
    /// regex. If the regex matches then the return value is the
    /// number of characters which matched. If the regex doesn't match
    /// the return is `None`.
    ///
    /// For more information see [Match vs
    /// Search](index.html#match-vs-search)
    ///
    /// The contents of `chars` must have the same encoding that was
    /// used to construct the regex.
    ///
    /// # Arguments
    ///
    /// * `chars` - The buffer to match against.
    /// * `at` - The byte index in the passed buffer to start matching
    /// * `options` - The regex match options.
    /// * `region` - The region for return group match range info
    /// * `match_param` - The match parameters
    ///
    /// # Returns
    ///
    /// `Ok(Some(len))` if the regex matched, with `len` being the number
    /// of bytes matched. `Ok(None)` if the regex doesn't match. `Err` with an
    /// `Error` if an error occurred (e.g. retry-limit-in-match exceeded).
    ///
    /// # Examples
    ///
    /// ```
    /// use onig::{Regex, EncodedBytes, MatchParam, SearchOptions};
    ///
    /// let r = Regex::with_encoding(EncodedBytes::ascii(b".*")).unwrap();
    /// let res = r.match_with_param(EncodedBytes::ascii(b"world"),
    ///                              0, SearchOptions::SEARCH_OPTION_NONE,
    ///                              None, MatchParam::default());
    /// assert!(res.is_ok()); // matching did not error
    /// assert!(res.unwrap() == Some(5)); // 5 characters matched
    /// ```
    pub fn match_with_param<T>(
        &self,
        chars: T,
        at: usize,
        options: SearchOptions,
        region: Option<&mut Region>,
        match_param: MatchParam,
    ) -> Result<Option<usize>, Error>
    where
        T: EncodedChars,
    {
        if chars.encoding() != self.encoding() {
            return Err(Error::custom(format!(
                "Regex encoding does not match haystack encoding ({0:?}, {1:?})",
                chars.encoding(),
                self.encoding()
            )));
        }
        let r = unsafe {
            let offset = chars.start_ptr().add(at);
            if offset > chars.limit_ptr() {
                return Err(Error::custom(format!("Offset {} is too large", at)));
            }
            onig_sys::onig_match_with_param(
                self.raw,
                chars.start_ptr(),
                chars.limit_ptr(),
                offset,
                match region {
                    Some(region) => region as *mut Region as *mut onig_sys::OnigRegion,
                    None => std::ptr::null_mut(),
                },
                options.bits(),
                match_param.as_raw(),
            )
        };

        if r >= 0 {
            Ok(Some(r as usize))
        } else if r == onig_sys::ONIG_MISMATCH {
            Ok(None)
        } else {
            Err(Error::from_code(r))
        }
    }

    /// Search pattern in string
    ///
    /// Search for matches the regex in a string. This method will return the
    /// index of the first match of the regex within the string, if
    /// there is one. If `from` is less than `to`, then search is performed
    /// in forward order, otherwise – in backward order.
    ///
    /// For more information see [Match vs
    /// Search](index.html#match-vs-search)
    ///
    /// # Arguments
    ///
    ///  * `str` - The string to search in.
    ///  * `from` - The byte index in the passed slice to start search
    ///  * `to` - The byte index in the passed slice to finish search
    ///  * `options` - The options for the search.
    ///  * `region` - The region for return group match range info
    ///
    /// # Returns
    ///
    /// `Some(pos)` if the regex matches, where `pos` is the
    /// byte-position of the start of the match. `None` if the regex
    /// doesn't match anywhere in `str`.
    ///
    /// # Examples
    ///
    /// ```
    /// use onig::{Regex, SearchOptions};
    ///
    /// let r = Regex::new("l{1,2}").unwrap();
    /// let res = r.search_with_options("hello", 0, 5, SearchOptions::SEARCH_OPTION_NONE, None);
    /// assert!(res.is_some()); // it matches
    /// assert!(res.unwrap() == 2); // match starts at character 3
    /// ```
    pub fn search_with_options(
        &self,
        str: &str,
        from: usize,
        to: usize,
        options: SearchOptions,
        region: Option<&mut Region>,
    ) -> Option<usize> {
        self.search_with_encoding(str, from, to, options, region)
    }

    /// Search for a Pattern in a String with an Encoding
    ///
    /// Search for matches the regex in a string. This method will
    /// return the index of the first match of the regex within the
    /// string, if there is one. If `from` is less than `to`, then
    /// search is performed in forward order, otherwise – in backward
    /// order.
    ///
    /// For more information see [Match vs
    /// Search](index.html#match-vs-search)
    ///
    /// The encoding of the buffer passed to search in must match the
    /// encoding of the regex.
    ///
    /// # Arguments
    ///
    ///  * `chars` - The character buffer to search in.
    ///  * `from` - The byte index in the passed slice to start search
    ///  * `to` - The byte index in the passed slice to finish search
    ///  * `options` - The options for the search.
    ///  * `region` - The region for return group match range info
    ///
    /// # Returns
    ///
    /// `Some(pos)` if the regex matches, where `pos` is the
    /// byte-position of the start of the match. `None` if the regex
    /// doesn't match anywhere in `chars`.
    ///
    /// # Examples
    ///
    /// ```
    /// use onig::{Regex, EncodedBytes, SearchOptions};
    ///
    /// let r = Regex::with_encoding(EncodedBytes::ascii(b"l{1,2}")).unwrap();
    /// let res = r.search_with_encoding(EncodedBytes::ascii(b"hello"),
    ///                                  0, 5, SearchOptions::SEARCH_OPTION_NONE, None);
    /// assert!(res.is_some()); // it matches
    /// assert!(res.unwrap() == 2); // match starts at character 3
    /// ```
    pub fn search_with_encoding<T>(
        &self,
        chars: T,
        from: usize,
        to: usize,
        options: SearchOptions,
        region: Option<&mut Region>,
    ) -> Option<usize>
    where
        T: EncodedChars,
    {
        let match_param = MatchParam::default();
        let result = self.search_with_param(chars, from, to, options, region, match_param);

        match result {
            Ok(r) => r,
            Err(e) => panic!("Onig: Regex search error: {}", e.description()),
        }
    }

    /// Search pattern in string with encoding and match param
    ///
    /// Search for matches the regex in a string. This method will
    /// return the index of the first match of the regex within the
    /// string, if there is one. If `from` is less than `to`, then
    /// search is performed in forward order, otherwise – in backward
    /// order.
    ///
    /// For more information see [Match vs
    /// Search](index.html#match-vs-search)
    ///
    /// The encoding of the buffer passed to search in must match the
    /// encoding of the regex.
    ///
    /// # Arguments
    ///
    ///  * `chars` - The character buffer to search in.
    ///  * `from` - The byte index in the passed slice to start search
    ///  * `to` - The byte index in the passed slice to finish search
    ///  * `options` - The options for the search.
    ///  * `region` - The region for return group match range info
    ///  * `match_param` - The match parameters
    ///
    /// # Returns
    ///
    /// `Ok(Some(pos))` if the regex matches, where `pos` is the
    /// byte-position of the start of the match. `Ok(None)` if the regex
    /// doesn't match anywhere in `chars`. `Err` with an `Error` if an error
    /// occurred (e.g. retry-limit-in-match exceeded).
    ///
    /// # Examples
    ///
    /// ```
    /// use onig::{Regex, EncodedBytes, MatchParam, SearchOptions};
    ///
    /// let r = Regex::with_encoding(EncodedBytes::ascii(b"l{1,2}")).unwrap();
    /// let res = r.search_with_param(EncodedBytes::ascii(b"hello"),
    ///                               0, 5, SearchOptions::SEARCH_OPTION_NONE,
    ///                               None, MatchParam::default());
    /// assert!(res.is_ok()); // matching did not error
    /// assert!(res.unwrap() == Some(2)); // match starts at character 3
    /// ```
    pub fn search_with_param<T>(
        &self,
        chars: T,
        from: usize,
        to: usize,
        options: SearchOptions,
        region: Option<&mut Region>,
        match_param: MatchParam,
    ) -> Result<Option<usize>, Error>
    where
        T: EncodedChars,
    {
        let (beg, end) = (chars.start_ptr(), chars.limit_ptr());
        if chars.encoding() != self.encoding() {
            return Err(Error::custom(format!(
                "Regex encoding does not match haystack encoding ({0:?}, {1:?})",
                chars.encoding(),
                self.encoding()
            )));
        }
        let r = unsafe {
            let start = beg.add(from);
            let range = beg.add(to);
            if start > end {
                return Err(Error::custom("Start of match should be before end"));
            }
            if range > end {
                return Err(Error::custom("Limit of match should be before end"));
            }
            onig_sys::onig_search_with_param(
                self.raw,
                beg,
                end,
                start,
                range,
                match region {
                    Some(region) => region as *mut Region as *mut onig_sys::OnigRegion,
                    None => std::ptr::null_mut(),
                },
                options.bits(),
                match_param.as_raw(),
            )
        };

        if r >= 0 {
            Ok(Some(r as usize))
        } else if r == onig_sys::ONIG_MISMATCH {
            Ok(None)
        } else {
            Err(Error::from_code(r))
        }
    }

    /// Returns true if and only if the regex matches the string given.
    ///
    /// For more information see [Match vs
    /// Search](index.html#match-vs-search)
    ///
    /// # Arguments
    ///  * `text` - The string slice to test against the pattern.
    ///
    /// # Returns
    ///
    /// `true` if the pattern matches the whole of `text`, `false` otherwise.
    pub fn is_match(&self, text: &str) -> bool {
        self.match_with_options(text, 0, SearchOptions::SEARCH_OPTION_WHOLE_STRING, None)
            .map(|r| r == text.len())
            .unwrap_or(false)
    }

    /// Find a Match in a Buffer, With Encoding
    ///
    /// Finds the first match of the regular expression within the
    /// buffer.
    ///
    /// Note that this should only be used if you want to discover the
    /// position of the match within a string. Testing if a pattern
    /// matches the whole string is faster if you use `is_match`.  For
    /// more information see [Match vs
    /// Search](index.html#match-vs-search)
    ///
    /// # Arguments
    ///  * `text` - The text to search in.
    ///
    /// # Returns
    ///
    ///  The offset of the start and end of the first match. If no
    ///  match exists `None` is returned.
    pub fn find(&self, text: &str) -> Option<(usize, usize)> {
        self.find_with_encoding(text)
    }

    /// Find a Match in a Buffer, With Encoding
    ///
    /// Finds the first match of the regular expression within the
    /// buffer.
    ///
    /// For more information see [Match vs
    /// Search](index.html#match-vs-search)
    ///
    /// # Arguments
    ///  * `text` - The text to search in.
    ///
    /// # Returns
    ///
    ///  The offset of the start and end of the first match. If no
    ///  match exists `None` is returned.
    pub fn find_with_encoding<T>(&self, text: T) -> Option<(usize, usize)>
    where
        T: EncodedChars,
    {
        let mut region = Region::new();
        let len = text.len();
        self.search_with_encoding(
            text,
            0,
            len,
            SearchOptions::SEARCH_OPTION_NONE,
            Some(&mut region),
        )
        .and_then(|_| region.pos(0))
    }

    /// Get the Encoding of the Regex
    ///
    /// # Returns
    ///
    /// Returns a reference to an oniguruma encoding which was used
    /// when this regex was created.
    pub fn encoding(&self) -> onig_sys::OnigEncoding {
        unsafe { onig_sys::onig_get_encoding(self.raw) }
    }

    /// Get the Number of Capture Groups in this Pattern
    pub fn captures_len(&self) -> usize {
        unsafe { onig_sys::onig_number_of_captures(self.raw) as usize }
    }

    /// Get the Size of the Capture Histories for this Pattern
    pub fn capture_histories_len(&self) -> usize {
        unsafe { onig_sys::onig_number_of_capture_histories(self.raw) as usize }
    }
}

impl Drop for Regex {
    fn drop(&mut self) {
        unsafe {
            onig_sys::onig_free(self.raw);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic;

    #[test]
    fn test_regex_create() {
        Regex::with_options(".*", RegexOptions::REGEX_OPTION_NONE, Syntax::default()).unwrap();

        Regex::new(r#"a \w+ word"#).unwrap();
    }

    #[test]
    fn test_regex_invalid() {
        let e = Regex::new("\\p{foo}").unwrap_err();
        assert_eq!(e.code(), -223);
        assert_eq!(e.description(), "invalid character property name {foo}");
    }

    #[test]
    fn test_failed_match() {
        let regex = Regex::new("foo").unwrap();
        let res = regex.match_with_options("bar", 0, SearchOptions::SEARCH_OPTION_NONE, None);
        assert!(res.is_none());
    }

    #[test]
    fn test_regex_search_with_options() {
        let mut region = Region::new();
        let regex = Regex::new("e(l+)").unwrap();

        let r = regex.search_with_options(
            "hello",
            0,
            5,
            SearchOptions::SEARCH_OPTION_NONE,
            Some(&mut region),
        );

        assert!(region.tree().is_none());
        assert_eq!(r, Some(1));
        assert_eq!(region.len(), 2);
        let pos1 = region.pos(0).unwrap();
        let pos2 = region.pos(1).unwrap();
        assert_eq!(pos1, (1, 4));
        assert_eq!(pos2, (2, 4));

        // test cloning here since we already have a filled region
        let cloned_region = region.clone();
        let pos1_clone = cloned_region.pos(0).unwrap();
        assert_eq!(pos1_clone, pos1);
    }

    #[test]
    fn test_regex_match_with_options() {
        let mut region = Region::new();
        let regex = Regex::new("he(l+)").unwrap();

        let r = regex.match_with_options(
            "hello",
            0,
            SearchOptions::SEARCH_OPTION_NONE,
            Some(&mut region),
        );

        assert!(region.tree().is_none());
        assert_eq!(r, Some(4));
        assert_eq!(region.len(), 2);
        let pos1 = region.pos(0).unwrap();
        let pos2 = region.pos(1).unwrap();
        assert_eq!(pos1, (0, 4));
        assert_eq!(pos2, (2, 4));
    }

    #[test]
    fn test_regex_is_match() {
        let regex = Regex::new("he(l+)o").unwrap();
        assert!(regex.is_match("hello"));
        assert!(!regex.is_match("hello 2.0"));
    }

    #[test]
    fn test_is_match_chooses_longest_alternation() {
        let regex = Regex::new("Greater|GreaterOrEqual").unwrap();
        assert!(regex.is_match("Greater"));
        assert!(regex.is_match("GreaterOrEqual"));
    }

    #[test]
    fn test_regex_find() {
        let regex = Regex::new("he(l+)o").unwrap();
        assert_eq!(regex.find("hey, hello!"), Some((5, 10)));
        assert_eq!(regex.find("hey, honey!"), None);
    }

    #[test]
    fn test_regex_captures_len() {
        let regex = Regex::new("(he)(l+)(o)").unwrap();
        assert_eq!(regex.captures_len(), 3);
    }

    #[test]
    fn test_regex_error_is_match() {
        let regex = Regex::new("(a|b|ab)*bc").unwrap();
        let result = regex.match_with_param(
            "ababababababababababababababababababababababababababababacbc",
            0,
            SearchOptions::SEARCH_OPTION_NONE,
            None,
            MatchParam::default(),
        );

        let e = result.err().unwrap();
        assert_eq!("retry-limit-in-match over", e.description());
    }

    #[test]
    fn test_regex_panic_is_match() {
        let regex = Regex::new("(a|b|ab)*bc").unwrap();
        let result = panic::catch_unwind(|| {
            regex.is_match("ababababababababababababababababababababababababababababacbc")
        });
        let e = result.err().unwrap();
        let message = e.downcast_ref::<String>().unwrap();
        assert_eq!(
            message.as_str(),
            "Onig: Regex match error: retry-limit-in-match over"
        );
    }

    #[test]
    fn test_regex_error_find() {
        let regex = Regex::new("(a|b|ab)*bc").unwrap();
        let s = "ababababababababababababababababababababababababababababacbc";
        let result = regex.search_with_param(
            s,
            0,
            s.len(),
            SearchOptions::SEARCH_OPTION_NONE,
            None,
            MatchParam::default(),
        );

        let e = result.err().unwrap();
        assert_eq!("retry-limit-in-match over", e.description());
    }

    #[test]
    fn test_regex_panic_find() {
        let regex = Regex::new("(a|b|ab)*bc").unwrap();
        let result = panic::catch_unwind(|| {
            regex.find("ababababababababababababababababababababababababababababacbc")
        });
        let e = result.err().unwrap();
        let message = e.downcast_ref::<String>().unwrap();
        assert_eq!(
            message.as_str(),
            "Onig: Regex search error: retry-limit-in-match over"
        );
    }

    #[test]
    fn test_search_with_invalid_range() {
        let regex = Regex::with_options("R...", RegexOptions::REGEX_OPTION_NONE, Syntax::default())
            .expect("regex");
        let string = "Ruby";
        let is_match = regex.search_with_param(
            string,
            5,
            string.len(),
            SearchOptions::SEARCH_OPTION_NONE,
            None,
            MatchParam::default(),
        );
        assert!(is_match.is_err());

        let is_match = regex.search_with_param(
            string,
            2,
            string.len() + 1,
            SearchOptions::SEARCH_OPTION_NONE,
            None,
            MatchParam::default(),
        );
        assert!(is_match.is_err());
    }

    #[test]
    fn test_search_with_invalid_range_panic() {
        let regex = Regex::with_options("R...", RegexOptions::REGEX_OPTION_NONE, Syntax::default())
            .expect("regex");
        let string = "Ruby";
        let is_match = panic::catch_unwind(|| {
            regex.search_with_encoding(
                string,
                5,
                string.len(),
                SearchOptions::SEARCH_OPTION_NONE,
                None,
            )
        });
        assert!(is_match.is_err());
    }

    #[test]
    fn test_match_with_invalid_range() {
        let regex = Regex::with_options("R...", RegexOptions::REGEX_OPTION_NONE, Syntax::default())
            .expect("regex");
        let string = "Ruby";
        let is_match = regex.match_with_param(
            string,
            5,
            SearchOptions::SEARCH_OPTION_NONE,
            None,
            MatchParam::default(),
        );
        assert!(is_match.is_err());
    }

    #[test]
    fn test_match_with_invalid_range_panic() {
        let regex = Regex::with_options("R...", RegexOptions::REGEX_OPTION_NONE, Syntax::default())
            .expect("regex");
        let string = "Ruby";
        let is_match = panic::catch_unwind(|| {
            regex.match_with_encoding(string, 5, SearchOptions::SEARCH_OPTION_NONE, None)
        });
        assert!(is_match.is_err());
    }
}
