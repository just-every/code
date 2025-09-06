use core::ffi::c_void;
use core_foundation::base::{CFComparisonResult, TCFType};
use core_foundation::{declare_TCFType, impl_CFComparison, impl_CFTypeDescription, impl_TCFType};

// sys equivalent stuff that must be declared

#[repr(C)]
pub struct __CFFooBar(c_void);

pub type CFFooBarRef = *const __CFFooBar;

extern "C" {
    pub fn CFFooBarGetTypeID() -> core_foundation::base::CFTypeID;
    pub fn fake_compare(
        this: CFFooBarRef,
        other: CFFooBarRef,
        context: *mut c_void,
    ) -> CFComparisonResult;
}

// Try to use the macros outside of the crate

declare_TCFType!(CFFooBar, CFFooBarRef);
impl_TCFType!(CFFooBar, CFFooBarRef, CFFooBarGetTypeID);
impl_CFTypeDescription!(CFFooBar);
impl_CFComparison!(CFFooBar, fake_compare);

declare_TCFType!(CFGenericFooBar<T: Clone>, CFFooBarRef);
impl_TCFType!(CFGenericFooBar<T: Clone>, CFFooBarRef, CFFooBarGetTypeID);
impl_CFTypeDescription!(CFGenericFooBar<T: Clone>);
impl_CFComparison!(CFGenericFooBar<T: Clone>, fake_compare);
