use rustfft::num_complex::Complex;
use rustfft::Length;

use twiddles;
use ::{DCT4, DST4, TransformType4};
use common;

/// Naive O(n^2 ) DCT Type 4 and DST Type 4 implementation
///
/// ~~~
/// // Computes a naive DCT4 of size 23
/// use rustdct::{DCT4, DST4};
/// use rustdct::algorithm::Type4Naive;
///
/// let len = 23;
/// let naive = Type4Naive::new(len);
/// 
/// let mut dct4_input:  Vec<f32> = vec![0f32; len];
/// let mut dct4_output: Vec<f32> = vec![0f32; len];
/// naive.process_dct4(&mut dct4_input, &mut dct4_output);
/// 
/// let mut dst4_input:  Vec<f32> = vec![0f32; len];
/// let mut dst4_output: Vec<f32> = vec![0f32; len];
/// naive.process_dst4(&mut dst4_input, &mut dst4_output);
/// ~~~
pub struct Type4Naive<T> {
    twiddles: Box<[Complex<T>]>,
}

impl<T: common::DCTnum> Type4Naive<T> {
    /// Creates a new DCT4 and DTS4 context that will process signals of length `len`
    pub fn new(len: usize) -> Self {
        let twiddles: Vec<Complex<T>> = (0..len * 4)
            .map(|i| twiddles::single_twiddle_halfoffset(i, len * 4))
            .collect();

        Type4Naive { twiddles: twiddles.into_boxed_slice() }
    }
}

impl<T: common::DCTnum> DCT4<T> for Type4Naive<T> {
    fn process_dct4(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        for k in 0..output.len() {
            let output_cell = output.get_mut(k).unwrap();
            *output_cell = T::zero();

            let mut twiddle_index = k;
            let twiddle_stride = k * 2 + 1;

            for i in 0..input.len() {
                let twiddle = self.twiddles[twiddle_index];

                *output_cell = *output_cell + input[i] * twiddle.re;

                twiddle_index += twiddle_stride;
                if twiddle_index >= self.twiddles.len() {
                    twiddle_index -= self.twiddles.len();
                }
            }
        }
    }
}
impl<T: common::DCTnum> DST4<T> for Type4Naive<T> {
    fn process_dst4(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        for k in 0..output.len() {
            let output_cell = output.get_mut(k).unwrap();
            *output_cell = T::zero();

            let mut twiddle_index = k;
            let twiddle_stride = k * 2 + 1;

            for i in 0..input.len() {
                let twiddle = self.twiddles[twiddle_index];

                *output_cell = *output_cell - input[i] * twiddle.im;

                twiddle_index += twiddle_stride;
                if twiddle_index >= self.twiddles.len() {
                    twiddle_index -= self.twiddles.len();
                }
            }
        }
    }
}
impl<T: common::DCTnum> TransformType4<T> for Type4Naive<T>{}
impl<T> Length for Type4Naive<T> {
    fn len(&self) -> usize {
        self.twiddles.len() / 4
    }
}
