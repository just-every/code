//! RustDCT is a pure-Rust signal processing library that computes the most common Discrete Cosine Transforms
//!
//! * Discrete Cosine Transform (DCT) Types 1, 2, 3, 4
//! * Discrete Sine Transform (DST) Types 1, 2, 3, 4
//! * Modified Discrete Cosine Transform (MDCT)
//!
//! The recommended way to use RustDCT is to create a [`DCTplanner`](struct.DCTplanner.html) instance, then call its
//! `plan_dct1` or `plan_dct2` or etc methods. Each transform type has its own `plan_*` method which will choose the best algorithm
//! for the given size.
//!
//! ```rust
//! // Compute a DCT Type 2 of size 1234
//! use std::sync::Arc;
//! use rustdct::DCTplanner;
//!
//! let mut input:  Vec<f32> = vec![0f32; 1234];
//! let mut output: Vec<f32> = vec![0f32; 1234];
//!
//! let mut planner = DCTplanner::new();
//! let dct = planner.plan_dct2(1234);
//! dct.process_dct2(&mut input, &mut output);
//!
//! // The DCT instance returned by the planner is stored behind an `Arc`, so it's cheap to clone
//! let dct_clone = Arc::clone(&dct);
//! ```
//! 
//! RustDCT also exposes individual DCT algorithms. For example, if you're writing a JPEG compression library, it's
//! safe to assume you want a DCT2 and DCT3 of size 8. Instead of going through the planner, you can directly create
//! hardcoded DCT instances of size 8.
//! 
//! ```rust
//! // Compute a DCT type 2 of size 8, and then compute a DCT type 3 of size 8 on the output.
//! use rustdct::{DCT2, DCT3};
//! use rustdct::algorithm::type2and3_butterflies::Type2And3Butterfly8;
//! 
//! let mut input = [0f32; 8];
//! let mut intermediate = [0f32; 8];
//! let mut output = [0f32; 8];
//! 
//! let dct = Type2And3Butterfly8::new();
//! 
//! dct.process_dct2(&mut input, &mut intermediate);
//! dct.process_dct3(&mut intermediate, &mut output);
//! ```

pub extern crate rustfft;

pub use rustfft::num_complex;
pub use rustfft::num_traits;

/// Algorithms for computing the Modified Discrete Cosine Transform
pub mod mdct;

pub mod algorithm;

mod plan;
mod twiddles;
mod common;
pub use common::DCTnum;

pub use self::plan::DCTplanner;

#[cfg(test)]
mod test_utils;

/// An umbrella trait for algorithms which compute the Discrete Cosine Transform Type 1 (DCT1)
pub trait DCT1<T: common::DCTnum>: rustfft::Length {
    /// Computes the DCT Type 1 on the `input` buffer and places the result in the `output` buffer.
    ///
    /// This method uses the `input` buffer as scratch space, so the contents of `input` should be considered garbage
    /// after calling
    fn process_dct1(&self, input: &mut [T], output: &mut [T]);
}

/// An umbrella trait for algorithms which compute the Discrete Cosine Transform Type 2 (DCT2)
pub trait DCT2<T: common::DCTnum>: rustfft::Length {
    /// Computes the DCT Type 2 on the `input` buffer and places the result in the `output` buffer.
    ///
    /// This method uses the `input` buffer as scratch space, so the contents of `input` should be considered garbage
    /// after calling
    fn process_dct2(&self, input: &mut [T], output: &mut [T]);
}

/// An umbrella trait for algorithms which compute the Discrete Cosine Transform Type 3 (DCT3)
pub trait DCT3<T: common::DCTnum>: rustfft::Length {
    /// Computes the DCT Type 3 on the `input` buffer and places the result in the `output` buffer.
    ///
    /// This method uses the `input` buffer as scratch space, so the contents of `input` should be considered garbage
    /// after calling
    fn process_dct3(&self, input: &mut [T], output: &mut [T]);
}


/// An umbrella trait for algorithms which compute the Discrete Cosine Transform Type 4 (DCT4)
pub trait DCT4<T: common::DCTnum>: rustfft::Length {
    /// Computes the DCT Type 4 on the `input` buffer and places the result in the `output` buffer.
    ///
    /// This method uses the `input` buffer as scratch space, so the contents of `input` should be considered garbage
    /// after calling
    fn process_dct4(&self, input: &mut [T], output: &mut [T]);
}

/// An umbrella trait for algorithms which compute the Discrete Sine Transform Type 1 (DST1)
pub trait DST1<T: common::DCTnum>: rustfft::Length {
    /// Computes the DST Type 1 on the `input` buffer and places the result in the `output` buffer.
    ///
    /// This method uses the `input` buffer as scratch space, so the contents of `input` should be considered garbage
    /// after calling
    fn process_dst1(&self, input: &mut [T], output: &mut [T]);
}

/// An umbrella trait for algorithms which compute the Discrete Sine Transform Type 2 (DST2)
pub trait DST2<T: common::DCTnum>: rustfft::Length {
    /// Computes the DST Type 2 on the `input` buffer and places the result in the `output` buffer.
    ///
    /// This method uses the `input` buffer as scratch space, so the contents of `input` should be considered garbage
    /// after calling
    fn process_dst2(&self, input: &mut [T], output: &mut [T]);
}

/// An umbrella trait for algorithms which compute the Discrete Sine Transform Type 3 (DST3)
pub trait DST3<T: common::DCTnum>: rustfft::Length {
    /// Computes the DST Type 3 on the `input` buffer and places the result in the `output` buffer.
    ///
    /// This method uses the `input` buffer as scratch space, so the contents of `input` should be considered garbage
    /// after calling
    fn process_dst3(&self, input: &mut [T], output: &mut [T]);
}

/// An umbrella trait for algorithms which compute the Discrete Sine Transform Type 4 (DST4)
pub trait DST4<T: common::DCTnum>: rustfft::Length {
    /// Computes the DST Type 4 on the `input` buffer and places the result in the `output` buffer.
    ///
    /// This method uses the `input` buffer as scratch space, so the contents of `input` should be considered garbage
    /// after calling
    fn process_dst4(&self, input: &mut [T], output: &mut [T]);
}

/// A trait for algorithms that can compute all of DCT2, DCT3, DST2, DST3, all in one struct
pub trait TransformType2And3<T: common::DCTnum> : DCT2<T> + DCT3<T> + DST2<T> + DST3<T> {}

/// A trait for algorithms that can compute both DCT4 and DST4, all in one struct
pub trait TransformType4<T: common::DCTnum> : DCT4<T> + DST4<T> {}
