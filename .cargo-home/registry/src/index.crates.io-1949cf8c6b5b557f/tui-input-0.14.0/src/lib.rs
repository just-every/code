//! TUI input library supporting multiple backends.
//!
//! See examples in the [GitHub repo](https://github.com/sayanarijit/tui-input/tree/main/examples).

mod input;

pub mod backend;
pub use input::{Input, InputRequest, InputResponse, StateChanged};
