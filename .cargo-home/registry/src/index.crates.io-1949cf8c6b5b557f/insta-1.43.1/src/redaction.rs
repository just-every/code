use pest::Parser;
use pest_derive::Parser;
use std::borrow::Cow;
use std::fmt;

use crate::content::Content;

#[derive(Debug)]
pub struct SelectorParseError(Box<pest::error::Error<Rule>>);

impl SelectorParseError {
    /// Return the column of where the error occurred.
    pub fn column(&self) -> usize {
        match self.0.line_col {
            pest::error::LineColLocation::Pos((_, col)) => col,
            pest::error::LineColLocation::Span((_, col), _) => col,
        }
    }
}

/// Represents a path for a callback function.
///
/// This can be converted into a string with `to_string` to see a stringified
/// path that the selector matched.
#[derive(Clone, Debug)]
#[cfg_attr(docsrs, doc(cfg(feature = "redactions")))]
pub struct ContentPath<'a>(&'a [PathItem]);

impl fmt::Display for ContentPath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for item in self.0.iter() {
            write!(f, ".")?;
            match *item {
                PathItem::Content(ref ctx) => {
                    if let Some(s) = ctx.as_str() {
                        write!(f, "{}", s)?;
                    } else {
                        write!(f, "<content>")?;
                    }
                }
                PathItem::Field(name) => write!(f, "{}", name)?,
                PathItem::Index(idx, _) => write!(f, "{}", idx)?,
            }
        }
        Ok(())
    }
}

/// Replaces a value with another one.
///
/// Represents a redaction.
#[cfg_attr(docsrs, doc(cfg(feature = "redactions")))]
pub enum Redaction {
    /// Static redaction with new content.
    Static(Content),
    /// Redaction with new content.
    Dynamic(Box<dyn Fn(Content, ContentPath<'_>) -> Content + Sync + Send>),
}

macro_rules! impl_from {
    ($ty:ty) => {
        impl From<$ty> for Redaction {
            fn from(value: $ty) -> Redaction {
                Redaction::Static(Content::from(value))
            }
        }
    };
}

impl_from!(());
impl_from!(bool);
impl_from!(u8);
impl_from!(u16);
impl_from!(u32);
impl_from!(u64);
impl_from!(i8);
impl_from!(i16);
impl_from!(i32);
impl_from!(i64);
impl_from!(f32);
impl_from!(f64);
impl_from!(char);
impl_from!(String);
impl_from!(Vec<u8>);

impl<'a> From<&'a str> for Redaction {
    fn from(value: &'a str) -> Redaction {
        Redaction::Static(Content::from(value))
    }
}

impl<'a> From<&'a [u8]> for Redaction {
    fn from(value: &'a [u8]) -> Redaction {
        Redaction::Static(Content::from(value))
    }
}

/// Creates a dynamic redaction.
///
/// This can be used to redact a value with a different value but instead of
/// statically declaring it a dynamic value can be computed.  This can also
/// be used to perform assertions before replacing the value.
///
/// The closure is passed two arguments: the value as [`Content`]
/// and the path that was selected (as [`ContentPath`])
///
/// Example:
///
/// ```rust
/// # use insta::{Settings, dynamic_redaction};
/// # let mut settings = Settings::new();
/// settings.add_redaction(".id", dynamic_redaction(|value, path| {
///     assert_eq!(path.to_string(), ".id");
///     assert_eq!(
///         value
///             .as_str()
///             .unwrap()
///             .chars()
///             .filter(|&c| c == '-')
///             .count(),
///         4
///     );
///     "[uuid]"
/// }));
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "redactions")))]
pub fn dynamic_redaction<I, F>(func: F) -> Redaction
where
    I: Into<Content>,
    F: Fn(Content, ContentPath<'_>) -> I + Send + Sync + 'static,
{
    Redaction::Dynamic(Box::new(move |c, p| func(c, p).into()))
}

/// Creates a dynamic redaction that sorts the value at the selector.
///
/// This is useful to force something like a set or map to be ordered to make
/// it deterministic.  This is necessary as insta's serialization support is
/// based on [`serde`] which does not have native set support.  As a result vectors
/// (which need to retain order) and sets (which should be given a stable order)
/// look the same.
///
/// ```rust
/// # use insta::{Settings, sorted_redaction};
/// # let mut settings = Settings::new();
/// settings.add_redaction(".flags", sorted_redaction());
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "redactions")))]
pub fn sorted_redaction() -> Redaction {
    fn sort(mut value: Content, _path: ContentPath) -> Content {
        match value.resolve_inner_mut() {
            Content::Seq(ref mut val) => {
                val.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            }
            Content::Map(ref mut val) => {
                val.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            }
            Content::Struct(_, ref mut fields)
            | Content::StructVariant(_, _, _, ref mut fields) => {
                fields.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            }
            _ => {}
        }
        value
    }
    dynamic_redaction(sort)
}

/// Creates a redaction that rounds floating point numbers to a given
/// number of decimal places.
///
/// ```rust
/// # use insta::{Settings, rounded_redaction};
/// # let mut settings = Settings::new();
/// settings.add_redaction(".sum", rounded_redaction(2));
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "redactions")))]
pub fn rounded_redaction(decimals: usize) -> Redaction {
    dynamic_redaction(move |value: Content, _path: ContentPath| -> Content {
        let f = match value.resolve_inner() {
            Content::F32(f) => *f as f64,
            Content::F64(f) => *f,
            _ => return value,
        };
        let x = 10f64.powf(decimals as f64);
        Content::F64((f * x).round() / x)
    })
}

impl Redaction {
    /// Performs the redaction of the value at the given path.
    fn redact(&self, value: Content, path: &[PathItem]) -> Content {
        match *self {
            Redaction::Static(ref new_val) => new_val.clone(),
            Redaction::Dynamic(ref callback) => callback(value, ContentPath(path)),
        }
    }
}

#[derive(Parser)]
#[grammar = "select_grammar.pest"]
pub struct SelectParser;

#[derive(Debug)]
pub enum PathItem {
    Content(Content),
    Field(&'static str),
    Index(u64, u64),
}

impl PathItem {
    fn as_str(&self) -> Option<&str> {
        match *self {
            PathItem::Content(ref content) => content.as_str(),
            PathItem::Field(s) => Some(s),
            PathItem::Index(..) => None,
        }
    }

    fn as_u64(&self) -> Option<u64> {
        match *self {
            PathItem::Content(ref content) => content.as_u64(),
            PathItem::Field(_) => None,
            PathItem::Index(idx, _) => Some(idx),
        }
    }

    fn range_check(&self, start: Option<i64>, end: Option<i64>) -> bool {
        fn expand_range(sel: i64, len: i64) -> i64 {
            if sel < 0 {
                (len + sel).max(0)
            } else {
                sel
            }
        }
        let (idx, len) = match *self {
            PathItem::Index(idx, len) => (idx as i64, len as i64),
            _ => return false,
        };
        match (start, end) {
            (None, None) => true,
            (None, Some(end)) => idx < expand_range(end, len),
            (Some(start), None) => idx >= expand_range(start, len),
            (Some(start), Some(end)) => {
                idx >= expand_range(start, len) && idx < expand_range(end, len)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment<'a> {
    DeepWildcard,
    Wildcard,
    Key(Cow<'a, str>),
    Index(u64),
    Range(Option<i64>, Option<i64>),
}

#[derive(Debug, Clone)]
pub struct Selector<'a> {
    selectors: Vec<Vec<Segment<'a>>>,
}

impl<'a> Selector<'a> {
    pub fn parse(selector: &'a str) -> Result<Selector<'a>, SelectorParseError> {
        let pair = SelectParser::parse(Rule::selectors, selector)
            .map_err(Box::new)
            .map_err(SelectorParseError)?
            .next()
            .unwrap();
        let mut rv = vec![];

        for selector_pair in pair.into_inner() {
            match selector_pair.as_rule() {
                Rule::EOI => break,
                other => assert_eq!(other, Rule::selector),
            }
            let mut segments = vec![];
            let mut have_deep_wildcard = false;
            for segment_pair in selector_pair.into_inner() {
                segments.push(match segment_pair.as_rule() {
                    Rule::identity => continue,
                    Rule::wildcard => Segment::Wildcard,
                    Rule::deep_wildcard => {
                        if have_deep_wildcard {
                            return Err(SelectorParseError(Box::new(
                                pest::error::Error::new_from_span(
                                    pest::error::ErrorVariant::CustomError {
                                        message: "deep wildcard used twice".into(),
                                    },
                                    segment_pair.as_span(),
                                ),
                            )));
                        }
                        have_deep_wildcard = true;
                        Segment::DeepWildcard
                    }
                    Rule::key => Segment::Key(Cow::Borrowed(&segment_pair.as_str()[1..])),
                    Rule::subscript => {
                        let subscript_rule = segment_pair.into_inner().next().unwrap();
                        match subscript_rule.as_rule() {
                            Rule::int => Segment::Index(subscript_rule.as_str().parse().unwrap()),
                            Rule::string => {
                                let sq = subscript_rule.as_str();
                                let s = &sq[1..sq.len() - 1];
                                let mut was_backslash = false;
                                Segment::Key(if s.bytes().any(|x| x == b'\\') {
                                    Cow::Owned(
                                        s.chars()
                                            .filter_map(|c| {
                                                let rv = match c {
                                                    '\\' if !was_backslash => {
                                                        was_backslash = true;
                                                        return None;
                                                    }
                                                    other => other,
                                                };
                                                was_backslash = false;
                                                Some(rv)
                                            })
                                            .collect(),
                                    )
                                } else {
                                    Cow::Borrowed(s)
                                })
                            }
                            _ => unreachable!(),
                        }
                    }
                    Rule::full_range => Segment::Range(None, None),
                    Rule::range => {
                        let mut int_rule = segment_pair
                            .into_inner()
                            .map(|x| x.as_str().parse().unwrap());
                        Segment::Range(int_rule.next(), int_rule.next())
                    }
                    Rule::range_to => {
                        let int_rule = segment_pair.into_inner().next().unwrap();
                        Segment::Range(None, int_rule.as_str().parse().ok())
                    }
                    Rule::range_from => {
                        let int_rule = segment_pair.into_inner().next().unwrap();
                        Segment::Range(int_rule.as_str().parse().ok(), None)
                    }
                    _ => unreachable!(),
                });
            }
            rv.push(segments);
        }

        Ok(Selector { selectors: rv })
    }

    pub fn make_static(self) -> Selector<'static> {
        Selector {
            selectors: self
                .selectors
                .into_iter()
                .map(|parts| {
                    parts
                        .into_iter()
                        .map(|x| match x {
                            Segment::Key(x) => Segment::Key(Cow::Owned(x.into_owned())),
                            Segment::Index(x) => Segment::Index(x),
                            Segment::Wildcard => Segment::Wildcard,
                            Segment::DeepWildcard => Segment::DeepWildcard,
                            Segment::Range(a, b) => Segment::Range(a, b),
                        })
                        .collect()
                })
                .collect(),
        }
    }

    fn segment_is_match(&self, segment: &Segment, element: &PathItem) -> bool {
        match *segment {
            Segment::Wildcard => true,
            Segment::DeepWildcard => true,
            Segment::Key(ref k) => element.as_str() == Some(k),
            Segment::Index(i) => element.as_u64() == Some(i),
            Segment::Range(start, end) => element.range_check(start, end),
        }
    }

    fn selector_is_match(&self, selector: &[Segment], path: &[PathItem]) -> bool {
        if let Some(idx) = selector.iter().position(|x| *x == Segment::DeepWildcard) {
            let forward_sel = &selector[..idx];
            let backward_sel = &selector[idx + 1..];

            if path.len() <= idx {
                return false;
            }

            for (segment, element) in forward_sel.iter().zip(path.iter()) {
                if !self.segment_is_match(segment, element) {
                    return false;
                }
            }

            for (segment, element) in backward_sel.iter().rev().zip(path.iter().rev()) {
                if !self.segment_is_match(segment, element) {
                    return false;
                }
            }

            true
        } else {
            if selector.len() != path.len() {
                return false;
            }
            for (segment, element) in selector.iter().zip(path.iter()) {
                if !self.segment_is_match(segment, element) {
                    return false;
                }
            }
            true
        }
    }

    pub fn is_match(&self, path: &[PathItem]) -> bool {
        for selector in &self.selectors {
            if self.selector_is_match(selector, path) {
                return true;
            }
        }
        false
    }

    pub fn redact(&self, value: Content, redaction: &Redaction) -> Content {
        self.redact_impl(value, redaction, &mut vec![])
    }

    fn redact_seq(
        &self,
        seq: Vec<Content>,
        redaction: &Redaction,
        path: &mut Vec<PathItem>,
    ) -> Vec<Content> {
        let len = seq.len();
        seq.into_iter()
            .enumerate()
            .map(|(idx, value)| {
                path.push(PathItem::Index(idx as u64, len as u64));
                let new_value = self.redact_impl(value, redaction, path);
                path.pop();
                new_value
            })
            .collect()
    }

    fn redact_struct(
        &self,
        seq: Vec<(&'static str, Content)>,
        redaction: &Redaction,
        path: &mut Vec<PathItem>,
    ) -> Vec<(&'static str, Content)> {
        seq.into_iter()
            .map(|(key, value)| {
                path.push(PathItem::Field(key));
                let new_value = self.redact_impl(value, redaction, path);
                path.pop();
                (key, new_value)
            })
            .collect()
    }

    fn redact_impl(
        &self,
        value: Content,
        redaction: &Redaction,
        path: &mut Vec<PathItem>,
    ) -> Content {
        if self.is_match(path) {
            redaction.redact(value, path)
        } else {
            match value {
                Content::Map(map) => Content::Map(
                    map.into_iter()
                        .map(|(key, value)| {
                            path.push(PathItem::Field("$key"));
                            let new_key = self.redact_impl(key.clone(), redaction, path);
                            path.pop();

                            path.push(PathItem::Content(key));
                            let new_value = self.redact_impl(value, redaction, path);
                            path.pop();

                            (new_key, new_value)
                        })
                        .collect(),
                ),
                Content::Seq(seq) => Content::Seq(self.redact_seq(seq, redaction, path)),
                Content::Tuple(seq) => Content::Tuple(self.redact_seq(seq, redaction, path)),
                Content::TupleStruct(name, seq) => {
                    Content::TupleStruct(name, self.redact_seq(seq, redaction, path))
                }
                Content::TupleVariant(name, variant_index, variant, seq) => Content::TupleVariant(
                    name,
                    variant_index,
                    variant,
                    self.redact_seq(seq, redaction, path),
                ),
                Content::Struct(name, seq) => {
                    Content::Struct(name, self.redact_struct(seq, redaction, path))
                }
                Content::StructVariant(name, variant_index, variant, seq) => {
                    Content::StructVariant(
                        name,
                        variant_index,
                        variant,
                        self.redact_struct(seq, redaction, path),
                    )
                }
                Content::NewtypeStruct(name, inner) => Content::NewtypeStruct(
                    name,
                    Box::new(self.redact_impl(*inner, redaction, path)),
                ),
                Content::NewtypeVariant(name, index, variant_name, inner) => {
                    Content::NewtypeVariant(
                        name,
                        index,
                        variant_name,
                        Box::new(self.redact_impl(*inner, redaction, path)),
                    )
                }
                Content::Some(contents) => {
                    Content::Some(Box::new(self.redact_impl(*contents, redaction, path)))
                }
                other => other,
            }
        }
    }
}

#[test]
fn test_range_checks() {
    use similar_asserts::assert_eq;
    assert_eq!(PathItem::Index(0, 10).range_check(None, Some(-1)), true);
    assert_eq!(PathItem::Index(9, 10).range_check(None, Some(-1)), false);
    assert_eq!(PathItem::Index(0, 10).range_check(Some(1), Some(-1)), false);
    assert_eq!(PathItem::Index(1, 10).range_check(Some(1), Some(-1)), true);
    assert_eq!(PathItem::Index(9, 10).range_check(Some(1), Some(-1)), false);
    assert_eq!(PathItem::Index(0, 10).range_check(Some(1), None), false);
    assert_eq!(PathItem::Index(1, 10).range_check(Some(1), None), true);
    assert_eq!(PathItem::Index(9, 10).range_check(Some(1), None), true);
}
