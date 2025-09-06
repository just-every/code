use std::f64;

use rustfft::Length;

use ::{DCT1, DST1};
use common;


/// Naive O(n^2 ) DCT Type 1 implementation
///
/// This implementation is primarily used to test other DCT1 algorithms. For small input sizes, this is actually
/// faster than `DCT1ViaFFT` because we don't have to pay the cost associated with converting the problem to a FFT.
///
/// ~~~
/// // Computes a naive DCT1 of size 23
/// use rustdct::DCT1;
/// use rustdct::algorithm::DCT1Naive;
///
/// let len = 23;
/// let mut input:  Vec<f32> = vec![0f32; len];
/// let mut output: Vec<f32> = vec![0f32; len];
///
/// let dct = DCT1Naive::new(len);
/// dct.process_dct1(&mut input, &mut output);
/// ~~~
pub struct DCT1Naive<T> {
    twiddles: Box<[T]>,
}

impl<T: common::DCTnum> DCT1Naive<T> {
    pub fn new(len: usize) -> Self {
        assert_ne!(len, 1, "DCT Type 1 is undefined for len == 1");

        let constant_factor = f64::consts::PI / ((len - 1) as f64);

        let twiddles: Vec<T> = (0..(len - 1) * 2)
            .map(|i| (constant_factor * (i as f64)).cos())
            .map(|c| T::from_f64(c).unwrap())
            .collect();

        DCT1Naive { twiddles: twiddles.into_boxed_slice() }
    }
}

impl<T: common::DCTnum> DCT1<T> for DCT1Naive<T> {
    fn process_dct1(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        let half = T::half();
        input[0] = input[0] * half;
        input[self.len() - 1] = input[self.len() - 1] * half;

        for k in 0..output.len() {
            let output_cell = output.get_mut(k).unwrap();
            *output_cell = input[0];

            let twiddle_stride = k;
            let mut twiddle_index = twiddle_stride;

            for i in 1..input.len() {
                let twiddle = self.twiddles[twiddle_index];

                *output_cell = *output_cell + input[i] * twiddle;

                twiddle_index += twiddle_stride;
                if twiddle_index >= self.twiddles.len() {
                    twiddle_index -= self.twiddles.len();
                }
            }
        }

    }
}
impl<T> Length for DCT1Naive<T> {
    fn len(&self) -> usize {
        self.twiddles.len() / 2 + 1
    }
}


/// Naive O(n^2 ) DST Type 1 implementation
///
/// This implementation is primarily used to test other DST1 algorithms.
///
/// ~~~
/// // Computes a naive DST1 of size 23
/// use rustdct::DST1;
/// use rustdct::algorithm::DST1Naive;
///
/// let len = 23;
/// let mut input:  Vec<f32> = vec![0f32; len];
/// let mut output: Vec<f32> = vec![0f32; len];
///
/// let dst = DST1Naive::new(len);
/// dst.process_dst1(&mut input, &mut output);
/// ~~~
pub struct DST1Naive<T> {
    twiddles: Box<[T]>,
}

impl<T: common::DCTnum> DST1Naive<T> {
    /// Creates a new DST1 context that will process signals of length `len`
    pub fn new(len: usize) -> Self {

        let constant_factor = f64::consts::PI / ((len + 1) as f64);

        let twiddles: Vec<T> = (0..(len + 1) * 2)
            .map(|i| (constant_factor * (i as f64)).sin())
            .map(|c| T::from_f64(c).unwrap())
            .collect();

        DST1Naive { twiddles: twiddles.into_boxed_slice() }
    }
}

impl<T: common::DCTnum> DST1<T> for DST1Naive<T> {
    fn process_dst1(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        for k in 0..output.len() {
            let output_cell = output.get_mut(k).unwrap();
            *output_cell = T::zero();

            let twiddle_stride = k + 1;
            let mut twiddle_index = twiddle_stride;

            for i in 0..input.len() {
                let twiddle = self.twiddles[twiddle_index];

                *output_cell = *output_cell + input[i] * twiddle;

                twiddle_index += twiddle_stride;
                if twiddle_index >= self.twiddles.len() {
                    twiddle_index -= self.twiddles.len();
                }
            }
        }
    }
}
impl<T> Length for DST1Naive<T> {
    fn len(&self) -> usize {
        self.twiddles.len() / 2 - 1
    }
}
