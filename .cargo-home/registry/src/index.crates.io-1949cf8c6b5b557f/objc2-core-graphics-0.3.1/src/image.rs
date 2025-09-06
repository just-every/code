use crate::CGBitmapInfo;

#[allow(non_upper_case_globals)]
impl CGBitmapInfo {
    #[doc(alias = "kCGBitmapByteOrder16Host")]
    pub const ByteOrder16Host: Self = if cfg!(target_endian = "big") {
        Self::ByteOrder16Big
    } else {
        Self::ByteOrder16Little
    };

    #[doc(alias = "kCGBitmapByteOrder32Host")]
    pub const ByteOrder32Host: Self = if cfg!(target_endian = "big") {
        Self::ByteOrder32Big
    } else {
        Self::ByteOrder32Little
    };
}
