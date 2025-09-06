//! Everything about parsing text into text annotated with scopes.
//!
//! The most important struct here is [`SyntaxSet`], check out the docs for that.
//!
//! [`SyntaxSet`]: struct.SyntaxSet.html

#[cfg(feature = "metadata")]
pub mod metadata;
#[cfg(feature = "parsing")]
mod parser;
#[cfg(feature = "parsing")]
pub mod syntax_definition;
#[cfg(feature = "parsing")]
mod syntax_set;
#[cfg(all(feature = "parsing", feature = "yaml-load"))]
mod yaml_load;

mod scope;
#[cfg(any(feature = "parsing", feature = "yaml-load", feature = "metadata"))]
mod regex;

#[cfg(feature = "parsing")]
pub use self::syntax_definition::SyntaxDefinition;
#[cfg(all(feature = "parsing", feature = "yaml-load"))]
pub use self::yaml_load::*;
#[cfg(feature = "parsing")]
pub use self::syntax_set::*;
#[cfg(feature = "parsing")]
pub use self::parser::*;
#[cfg(feature = "metadata")]
pub use self::metadata::*;

#[cfg(any(feature = "parsing", feature = "yaml-load", feature = "metadata"))]
pub use self::regex::*;

pub use self::scope::*;
