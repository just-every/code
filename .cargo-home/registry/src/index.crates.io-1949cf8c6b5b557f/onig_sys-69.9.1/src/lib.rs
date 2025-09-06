#[cfg(feature = "generate")]
mod bindgened;

#[cfg(feature = "generate")]
pub use crate::bindgened::*;

#[cfg(not(feature = "generate"))]
#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(clippy::all)]
mod ffi;

#[cfg(not(feature = "generate"))]
pub use self::ffi::*;

// backfill types from the old hand-written bindings:

pub type OnigSyntaxBehavior = ::std::os::raw::c_uint;
pub type OnigSyntaxOp = ::std::os::raw::c_uint;
pub type OnigSyntaxOp2 = ::std::os::raw::c_uint;

#[test]
fn test_is_linked() {
    unsafe {
        assert!(!onig_copyright().is_null());
        assert!(!onig_version().is_null());
    }
}
