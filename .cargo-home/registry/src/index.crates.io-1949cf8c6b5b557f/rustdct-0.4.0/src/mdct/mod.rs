use rustfft::Length;

use common;

mod mdct_naive;
mod mdct_via_dct4;

pub mod window_fn;

/// An umbrella trait for algorithms which compute the Modified Discrete Cosine Transform (MDCT)
pub trait MDCT<T: common::DCTnum>: Length {
    /// Computes the MDCT on the `input` buffer and places the result in the `output` buffer.
    ///
    /// To make overlapping array segments easier, this method DOES NOT modify the input buffer.
    fn process_mdct(&self, input: &[T], output: &mut [T]) {
        let (input_a, input_b) = input.split_at(output.len());

        self.process_mdct_split(input_a, input_b, output);
    }

    /// Computes the MDCT on the `input` buffer and places the result in the `output` buffer.
    /// Uses `input_a` for the first half of the input, and `input_b` for the second half of the input
    ///
    /// To make overlapping array segments easier, this method DOES NOT modify the input buffer.
    fn process_mdct_split(&self, input_a: &[T], input_b: &[T], output: &mut [T]);

    /// Computes the IMDCT on the `input` buffer and places the result in the `output` buffer.
    ///
    /// Since the IMDCT is designed with overlapping output segments in mind, this method DOES NOT zero
    /// out the output buffer before writing like most other DCT algorithms. Instead, it sums
    /// the result of the IMDCT with what's already in the output buffer.
    fn process_imdct(&self, input: &[T], output: &mut [T]) {
        let (output_a, output_b) = output.split_at_mut(input.len());

        self.process_imdct_split(input, output_a, output_b);
    }

    /// Computes the IMDCT on the `input` buffer and places the result in the `output` buffer.
    /// Puts the first half of the output in `output_a`, and puts the first half of the output in `output_b`.
    ///
    /// Since the IMDCT is designed with overlapping output segments in mind, this method DOES NOT zero
    /// out the output buffer before writing like most other DCT algorithms. Instead, it sums
    /// the result of the IMDCT with what's already in the output buffer.
    fn process_imdct_split(&self, input: &[T], output_a: &mut [T], output_b: &mut [T]);
}

pub use self::mdct_naive::MDCTNaive;
pub use self::mdct_via_dct4::MDCTViaDCT4;
