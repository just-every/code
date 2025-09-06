use rustfft::num_complex::Complex;
use rustfft::Length;

use twiddles;
use ::{DCT2, DST2, DCT3, DST3, TransformType2And3};
use common;

/// Naive O(n^2 ) DCT Type 2, DST Type 2, DCT Type 3, and DST Type 3 implementation
///
/// ~~~
/// // Computes a naive DCT2, DST2, DCT3, and DST3 of size 23
/// use rustdct::{DCT2, DST2, DCT3, DST3};
/// use rustdct::algorithm::Type2And3Naive;
///
/// let len = 23;
/// let naive = Type2And3Naive::new(len);
/// 
/// let mut dct2_input:  Vec<f32> = vec![0f32; len];
/// let mut dct2_output: Vec<f32> = vec![0f32; len];
/// naive.process_dct2(&mut dct2_input, &mut dct2_output);
/// 
/// let mut dst2_input:  Vec<f32> = vec![0f32; len];
/// let mut dst2_output: Vec<f32> = vec![0f32; len];
/// naive.process_dst2(&mut dst2_input, &mut dst2_output);
/// 
/// let mut dct3_input:  Vec<f32> = vec![0f32; len];
/// let mut dct3_output: Vec<f32> = vec![0f32; len];
/// naive.process_dct3(&mut dct3_input, &mut dct3_output);
/// 
/// let mut dst3_input:  Vec<f32> = vec![0f32; len];
/// let mut dst3_output: Vec<f32> = vec![0f32; len];
/// naive.process_dst3(&mut dst3_input, &mut dst3_output);
/// ~~~
pub struct Type2And3Naive<T> {
    twiddles: Box<[Complex<T>]>,
}

impl<T: common::DCTnum> Type2And3Naive<T> {
    /// Creates a new DCT2, DCT3, DST2, and DST3 context that will process signals of length `len`
    pub fn new(len: usize) -> Self {
        let twiddles: Vec<Complex<T>> = (0..len * 4)
            .map(|i| twiddles::single_twiddle(i, len * 4))
            .collect();

        Type2And3Naive { twiddles: twiddles.into_boxed_slice() }
    }
}

impl<T: common::DCTnum> DCT2<T> for Type2And3Naive<T> {
    fn process_dct2(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        for k in 0..output.len() {
            let output_cell = output.get_mut(k).unwrap();
            *output_cell = T::zero();

            let twiddle_stride = k * 2;
            let mut twiddle_index = k;

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
impl<T: common::DCTnum> DST2<T> for Type2And3Naive<T> {
    fn process_dst2(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        for k in 0..output.len() {
            let output_cell = output.get_mut(k).unwrap();
            *output_cell = T::zero();

            let twiddle_stride = (k + 1) * 2;
            let mut twiddle_index = k + 1;

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
impl<T: common::DCTnum> DCT3<T> for Type2And3Naive<T> {
    fn process_dct3(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        let half_first = T::half() * input[0];

        for k in 0..output.len() {
            let output_cell = output.get_mut(k).unwrap();
            *output_cell = half_first;

            let twiddle_stride = k * 2 + 1;
            let mut twiddle_index = twiddle_stride;

            for i in 1..input.len() {
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
impl<T: common::DCTnum> DST3<T> for Type2And3Naive<T> {
    fn process_dst3(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        // scale the last input value by half before going into the loop
        input[input.len() - 1] = input[input.len() - 1] * T::half();

        for k in 0..output.len() {
            let output_cell = output.get_mut(k).unwrap();
            *output_cell = T::zero();

            let twiddle_stride = k * 2 + 1;
            let mut twiddle_index = twiddle_stride;

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
impl<T: common::DCTnum> TransformType2And3<T> for Type2And3Naive<T>{}
impl<T> Length for Type2And3Naive<T> {
    fn len(&self) -> usize {
        self.twiddles.len() / 4
    }
}
