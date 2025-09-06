use std::fmt;

/// A [`Revision`] represents a version of CDP.
#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq)]
pub struct Revision(pub(crate) u32);

impl From<u32> for Revision {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl fmt::Display for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v0.0.{}", self.0)
    }
}
