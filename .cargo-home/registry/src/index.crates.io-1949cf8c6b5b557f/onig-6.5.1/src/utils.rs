use std::ffi::{CStr, CString};
use std::mem;

/// Get Version
///
/// Returns the version information for the underlying Oniguruma
/// API. This is separate from the Rust Onig and onig_sys versions.
pub fn version() -> String {
    let raw_version = unsafe { CStr::from_ptr(onig_sys::onig_version()) };
    raw_version.to_string_lossy().into_owned()
}

/// Get Copyright
///
/// Returns copyright information for the underlying Oniguruma
/// API. Rust onig is licensed seperately. For more information see
/// LICENSE.md in the source distribution.
pub fn copyright() -> String {
    let raw_copy = unsafe { CStr::from_ptr(onig_sys::onig_copyright()) };
    raw_copy.to_string_lossy().into_owned()
}

pub type CodePointRange = (onig_sys::OnigCodePoint, onig_sys::OnigCodePoint);

/// Create a User Defined Proeprty
///
/// Creates a new user defined property from the given OnigCodePoint vlaues.
pub fn define_user_property(name: &str, ranges: &[CodePointRange]) -> i32 {
    let mut raw_ranges = vec![ranges.len() as onig_sys::OnigCodePoint];
    for &(start, end) in ranges {
        raw_ranges.push(start);
        raw_ranges.push(end);
    }
    let name = CString::new(name).unwrap();
    let r = unsafe {
        onig_sys::onig_unicode_define_user_property(name.as_ptr(), raw_ranges.as_mut_ptr())
    };

    // Deliberately leak the memory here as Onig expects to be able to
    // hang on to the pointer we just gave it. I'm not happy about it
    // but this does work and the amounts of memory leaked should be
    // trivial.
    mem::forget(raw_ranges);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn utils_get_copyright_is_not_emtpy() {
        let copyright = copyright();
        assert!(copyright.len() > 0);
    }
}
