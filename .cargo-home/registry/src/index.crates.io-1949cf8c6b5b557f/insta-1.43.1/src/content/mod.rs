//! This module implements a generic `Content` type that can hold
//! runtime typed data.
//!
//! It's modelled after serde's data format but it's in fact possible to use
//! this independently of serde.  The `yaml` and `json` support implemented
//! here works without serde.  Only `yaml` has an implemented parser but since
//! YAML is a superset of JSON insta instead currently parses JSON via the
//! YAML implementation.

pub mod json;
#[cfg(feature = "serde")]
mod serialization;
pub mod yaml;

#[cfg(feature = "serde")]
pub use serialization::*;

use std::fmt;

/// An internal error type for content related errors.
#[derive(Debug)]
pub enum Error {
    FailedParsingYaml(std::path::PathBuf),
    UnexpectedDataType,
    MissingField,
    FileIo(std::io::Error, std::path::PathBuf),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::FailedParsingYaml(p) => {
                f.write_str(format!("Failed parsing the YAML from {:?}", p.display()).as_str())
            }
            Error::UnexpectedDataType => {
                f.write_str("The present data type wasn't what was expected")
            }
            Error::MissingField => f.write_str("A required field was missing"),
            Error::FileIo(e, p) => {
                f.write_str(format!("File error for {:?}: {}", p.display(), e).as_str())
            }
        }
    }
}

impl std::error::Error for Error {}

/// Represents variable typed content.
///
/// This is used for the serialization system to represent values
/// before the actual snapshots are written and is also exposed to
/// dynamic redaction functions.
///
/// Some enum variants are intentionally not exposed to user code.
/// It's generally recommended to construct content objects by
/// using the [`From`] trait and by using the
/// accessor methods to assert on it.
///
/// While matching on the content is possible in theory it is
/// recommended against.  The reason for this is that the content
/// enum holds variants that can "wrap" values where it's not
/// expected.  For instance if a field holds an `Option<String>`
/// you cannot use pattern matching to extract the string as it
/// will be contained in an internal [`Some`] variant that is not
/// exposed.  On the other hand the [`Content::as_str`] method will
/// automatically resolve such internal wrappers.
///
/// If you do need to pattern match you should use the
/// [`Content::resolve_inner`] method to resolve such internal wrappers.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Content {
    Bool(bool),

    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),

    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),

    F32(f32),
    F64(f64),

    Char(char),
    String(String),
    Bytes(Vec<u8>),

    #[doc(hidden)]
    None,
    #[doc(hidden)]
    Some(Box<Content>),

    #[doc(hidden)]
    Unit,
    #[doc(hidden)]
    UnitStruct(&'static str),
    #[doc(hidden)]
    UnitVariant(&'static str, u32, &'static str),
    #[doc(hidden)]
    NewtypeStruct(&'static str, Box<Content>),
    #[doc(hidden)]
    NewtypeVariant(&'static str, u32, &'static str, Box<Content>),

    Seq(Vec<Content>),
    #[doc(hidden)]
    Tuple(Vec<Content>),
    #[doc(hidden)]
    TupleStruct(&'static str, Vec<Content>),
    #[doc(hidden)]
    TupleVariant(&'static str, u32, &'static str, Vec<Content>),
    Map(Vec<(Content, Content)>),
    #[doc(hidden)]
    Struct(&'static str, Vec<(&'static str, Content)>),
    #[doc(hidden)]
    StructVariant(
        &'static str,
        u32,
        &'static str,
        Vec<(&'static str, Content)>,
    ),
}

macro_rules! impl_from {
    ($ty:ty, $newty:ident) => {
        impl From<$ty> for Content {
            fn from(value: $ty) -> Content {
                Content::$newty(value)
            }
        }
    };
}

impl_from!(bool, Bool);
impl_from!(u8, U8);
impl_from!(u16, U16);
impl_from!(u32, U32);
impl_from!(u64, U64);
impl_from!(u128, U128);
impl_from!(i8, I8);
impl_from!(i16, I16);
impl_from!(i32, I32);
impl_from!(i64, I64);
impl_from!(i128, I128);
impl_from!(f32, F32);
impl_from!(f64, F64);
impl_from!(char, Char);
impl_from!(String, String);
impl_from!(Vec<u8>, Bytes);

impl From<()> for Content {
    fn from(_value: ()) -> Content {
        Content::Unit
    }
}

impl<'a> From<&'a str> for Content {
    fn from(value: &'a str) -> Content {
        Content::String(value.to_string())
    }
}

impl<'a> From<&'a [u8]> for Content {
    fn from(value: &'a [u8]) -> Content {
        Content::Bytes(value.to_vec())
    }
}

impl Content {
    /// This resolves the innermost content in a chain of
    /// wrapped content.
    ///
    /// For instance if you encounter an `Option<Option<String>>`
    /// field the content will be wrapped twice in an internal
    /// option wrapper.  If you need to pattern match you will
    /// need in some situations to first resolve the inner value
    /// before such matching can take place as there is no exposed
    /// way to match on these wrappers.
    ///
    /// This method does not need to be called for the `as_`
    /// methods which resolve automatically.
    pub fn resolve_inner(&self) -> &Content {
        match *self {
            Content::Some(ref v)
            | Content::NewtypeStruct(_, ref v)
            | Content::NewtypeVariant(_, _, _, ref v) => v.resolve_inner(),
            ref other => other,
        }
    }

    /// Mutable version of [`Self::resolve_inner`].
    pub fn resolve_inner_mut(&mut self) -> &mut Content {
        match *self {
            Content::Some(ref mut v)
            | Content::NewtypeStruct(_, ref mut v)
            | Content::NewtypeVariant(_, _, _, ref mut v) => v.resolve_inner_mut(),
            ref mut other => other,
        }
    }

    /// Returns the value as string
    pub fn as_str(&self) -> Option<&str> {
        match self.resolve_inner() {
            Content::String(ref s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns the value as bytes
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self.resolve_inner() {
            Content::Bytes(ref b) => Some(b),
            _ => None,
        }
    }

    /// Returns the value as slice of content values.
    pub fn as_slice(&self) -> Option<&[Content]> {
        match self.resolve_inner() {
            Content::Seq(ref v) | Content::Tuple(ref v) | Content::TupleVariant(_, _, _, ref v) => {
                Some(&v[..])
            }
            _ => None,
        }
    }

    /// Returns true if the value is nil.
    pub fn is_nil(&self) -> bool {
        matches!(self.resolve_inner(), Content::None | Content::Unit)
    }

    /// Returns the value as bool
    pub fn as_bool(&self) -> Option<bool> {
        match *self.resolve_inner() {
            Content::Bool(val) => Some(val),
            _ => None,
        }
    }

    /// Returns the value as u64
    pub fn as_u64(&self) -> Option<u64> {
        match *self.resolve_inner() {
            Content::U8(v) => Some(u64::from(v)),
            Content::U16(v) => Some(u64::from(v)),
            Content::U32(v) => Some(u64::from(v)),
            Content::U64(v) => Some(v),
            Content::U128(v) => {
                let rv = v as u64;
                if rv as u128 == v {
                    Some(rv)
                } else {
                    None
                }
            }
            Content::I8(v) if v >= 0 => Some(v as u64),
            Content::I16(v) if v >= 0 => Some(v as u64),
            Content::I32(v) if v >= 0 => Some(v as u64),
            Content::I64(v) if v >= 0 => Some(v as u64),
            Content::I128(v) => {
                let rv = v as u64;
                if rv as i128 == v {
                    Some(rv)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Returns the value as u128
    pub fn as_u128(&self) -> Option<u128> {
        match *self.resolve_inner() {
            Content::U128(v) => Some(v),
            Content::I128(v) if v >= 0 => Some(v as u128),
            _ => self.as_u64().map(u128::from),
        }
    }

    /// Returns the value as i64
    pub fn as_i64(&self) -> Option<i64> {
        match *self.resolve_inner() {
            Content::U8(v) => Some(i64::from(v)),
            Content::U16(v) => Some(i64::from(v)),
            Content::U32(v) => Some(i64::from(v)),
            Content::U64(v) => {
                let rv = v as i64;
                if rv as u64 == v {
                    Some(rv)
                } else {
                    None
                }
            }
            Content::U128(v) => {
                let rv = v as i64;
                if rv as u128 == v {
                    Some(rv)
                } else {
                    None
                }
            }
            Content::I8(v) => Some(i64::from(v)),
            Content::I16(v) => Some(i64::from(v)),
            Content::I32(v) => Some(i64::from(v)),
            Content::I64(v) => Some(v),
            Content::I128(v) => {
                let rv = v as i64;
                if rv as i128 == v {
                    Some(rv)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Returns the value as i128
    pub fn as_i128(&self) -> Option<i128> {
        match *self.resolve_inner() {
            Content::U128(v) => {
                let rv = v as i128;
                if rv as u128 == v {
                    Some(rv)
                } else {
                    None
                }
            }
            Content::I128(v) => Some(v),
            _ => self.as_i64().map(i128::from),
        }
    }

    /// Returns the value as f64
    pub fn as_f64(&self) -> Option<f64> {
        match *self.resolve_inner() {
            Content::F32(v) => Some(f64::from(v)),
            Content::F64(v) => Some(v),
            _ => None,
        }
    }

    /// Recursively walks the content structure mutably.
    ///
    /// The callback is invoked for every content in the tree.
    pub fn walk<F: FnMut(&mut Content) -> bool>(&mut self, visit: &mut F) {
        if !visit(self) {
            return;
        }

        match *self {
            Content::Some(ref mut inner) => {
                Self::walk(&mut *inner, visit);
            }
            Content::NewtypeStruct(_, ref mut inner) => {
                Self::walk(&mut *inner, visit);
            }
            Content::NewtypeVariant(_, _, _, ref mut inner) => {
                Self::walk(&mut *inner, visit);
            }
            Content::Seq(ref mut vec) => {
                for inner in vec.iter_mut() {
                    Self::walk(inner, visit);
                }
            }
            Content::Map(ref mut vec) => {
                for inner in vec.iter_mut() {
                    Self::walk(&mut inner.0, visit);
                    Self::walk(&mut inner.1, visit);
                }
            }
            Content::Struct(_, ref mut vec) => {
                for inner in vec.iter_mut() {
                    Self::walk(&mut inner.1, visit);
                }
            }
            Content::StructVariant(_, _, _, ref mut vec) => {
                for inner in vec.iter_mut() {
                    Self::walk(&mut inner.1, visit);
                }
            }
            Content::Tuple(ref mut vec) => {
                for inner in vec.iter_mut() {
                    Self::walk(inner, visit);
                }
            }
            Content::TupleStruct(_, ref mut vec) => {
                for inner in vec.iter_mut() {
                    Self::walk(inner, visit);
                }
            }
            Content::TupleVariant(_, _, _, ref mut vec) => {
                for inner in vec.iter_mut() {
                    Self::walk(inner, visit);
                }
            }
            _ => {}
        }
    }
}
