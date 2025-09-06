use std::sync::Arc;

use num_complex::Complex;
use transpose;

use common::{FFTnum, verify_length, verify_length_divisible};

use ::{Length, IsInverse, FFT};
use algorithm::butterflies::FFTButterfly;
use array_utils;
use twiddles;

/// Implementation of the Mixed-Radix FFT algorithm
///
/// This algorithm factors a size n FFT into n1 * n2, computes several inner FFTs of size n1 and n2, then combines the 
/// results to get the final answer
///
/// ~~~
/// // Computes a forward FFT of size 1200, using the Mixed-Radix Algorithm
/// use rustfft::algorithm::MixedRadix;
/// use rustfft::{FFT, FFTplanner};
/// use rustfft::num_complex::Complex;
/// use rustfft::num_traits::Zero;
///
/// let mut input:  Vec<Complex<f32>> = vec![Zero::zero(); 1200];
/// let mut output: Vec<Complex<f32>> = vec![Zero::zero(); 1200];
///
/// // we need to find an n1 and n2 such that n1 * n2 == 1200
/// // n1 = 30 and n2 = 40 satisfies this
/// let mut planner = FFTplanner::new(false);
/// let inner_fft_n1 = planner.plan_fft(30);
/// let inner_fft_n2 = planner.plan_fft(40);
///
/// // the mixed radix FFT length will be inner_fft_n1.len() * inner_fft_n2.len() = 1200
/// let fft = MixedRadix::new(inner_fft_n1, inner_fft_n2);
/// fft.process(&mut input, &mut output);
/// ~~~

pub struct MixedRadix<T> {
    width: usize,
    width_size_fft: Arc<FFT<T>>,

    height: usize,
    height_size_fft: Arc<FFT<T>>,

    twiddles: Box<[Complex<T>]>,
    inverse: bool,
}

impl<T: FFTnum> MixedRadix<T> {
    /// Creates a FFT instance which will process inputs/outputs of size `width_fft.len() * height_fft.len()`
    pub fn new(width_fft: Arc<FFT<T>>, height_fft: Arc<FFT<T>>) -> Self {
        assert_eq!(
            width_fft.is_inverse(), height_fft.is_inverse(), 
            "width_fft and height_fft must both be inverse, or neither. got width inverse={}, height inverse={}",
            width_fft.is_inverse(), height_fft.is_inverse());

        let inverse = width_fft.is_inverse();

        let width = width_fft.len();
        let height = height_fft.len();

        let len = width * height;

        let mut twiddles = Vec::with_capacity(len);
        for x in 0..width {
            for y in 0..height {
                twiddles.push(twiddles::single_twiddle(x * y, len, inverse));
            }
        }

        MixedRadix {
            width: width,
            width_size_fft: width_fft,

            height: height,
            height_size_fft: height_fft,

            twiddles: twiddles.into_boxed_slice(),
            inverse: inverse,
        }
    }


    fn perform_fft(&self, input: &mut [Complex<T>], output: &mut [Complex<T>]) {
        // SIX STEP FFT:

        // STEP 1: transpose
        transpose::transpose(input, output, self.width, self.height);

        // STEP 2: perform FFTs of size `height`
        self.height_size_fft.process_multi(output, input);

        // STEP 3: Apply twiddle factors
        for (element, &twiddle) in input.iter_mut().zip(self.twiddles.iter()) {
            *element = *element * twiddle;
        }

        // STEP 4: transpose again
        transpose::transpose(input, output, self.height, self.width);

        // STEP 5: perform FFTs of size `width`
        self.width_size_fft.process_multi(output, input);

        // STEP 6: transpose again
        transpose::transpose(input, output, self.width, self.height);
    }
}
impl<T: FFTnum> FFT<T> for MixedRadix<T> {
    fn process(&self, input: &mut [Complex<T>], output: &mut [Complex<T>]) {
        verify_length(input, output, self.len());

        self.perform_fft(input, output);
    }
    fn process_multi(&self, input: &mut [Complex<T>], output: &mut [Complex<T>]) {
        verify_length_divisible(input, output, self.len());

        for (in_chunk, out_chunk) in input.chunks_mut(self.len()).zip(output.chunks_mut(self.len())) {
            self.perform_fft(in_chunk, out_chunk);
        }
    }
}
impl<T> Length for MixedRadix<T> {
    #[inline(always)]
    fn len(&self) -> usize {
        self.twiddles.len()
    }
}
impl<T> IsInverse for MixedRadix<T> {
    #[inline(always)]
    fn is_inverse(&self) -> bool {
        self.inverse
    }
}





/// Implementation of the Mixed-Radix FFT algorithm, specialized for the case where both inner FFTs are butterflies
///
/// This algorithm factors a size n FFT into n1 * n2
///
/// ~~~
/// // Computes a forward FFT of size 56, using the Mixed-Radix Butterfly Algorithm
/// use std::sync::Arc;
/// use rustfft::algorithm::MixedRadixDoubleButterfly;
/// use rustfft::algorithm::butterflies::{Butterfly7, Butterfly8};
/// use rustfft::FFT;
/// use rustfft::num_complex::Complex;
/// use rustfft::num_traits::Zero;
///
/// let mut input:  Vec<Complex<f32>> = vec![Zero::zero(); 56];
/// let mut output: Vec<Complex<f32>> = vec![Zero::zero(); 56];
///
/// // we need to find an n1 and n2 such that n1 * n2 == 56
/// // n1 = 7 and n2 = 8 satisfies this
/// let inner_fft_n1 = Arc::new(Butterfly7::new(false));
/// let inner_fft_n2 = Arc::new(Butterfly8::new(false));
///
/// // the mixed radix FFT length will be inner_fft_n1.len() * inner_fft_n2.len() = 56
/// let fft = MixedRadixDoubleButterfly::new(inner_fft_n1, inner_fft_n2);
/// fft.process(&mut input, &mut output);
/// ~~~
pub struct MixedRadixDoubleButterfly<T> {
    width: usize,
    width_size_fft: Arc<FFTButterfly<T>>,

    height: usize,
    height_size_fft: Arc<FFTButterfly<T>>,

    twiddles: Box<[Complex<T>]>,
    inverse: bool,
}

impl<T: FFTnum> MixedRadixDoubleButterfly<T> {
    /// Creates a FFT instance which will process inputs/outputs of size `width_fft.len() * height_fft.len()`
    pub fn new(width_fft: Arc<FFTButterfly<T>>, height_fft: Arc<FFTButterfly<T>>) -> Self {
        assert_eq!(
            width_fft.is_inverse(), height_fft.is_inverse(), 
            "width_fft and height_fft must both be inverse, or neither. got width inverse={}, height inverse={}",
            width_fft.is_inverse(), height_fft.is_inverse());

        let inverse = width_fft.is_inverse();

        let width = width_fft.len();
        let height = height_fft.len();

        let len = width * height;

        let mut twiddles = Vec::with_capacity(len);
        for x in 0..width {
            for y in 0..height {
                twiddles.push(twiddles::single_twiddle(x * y, len, inverse));
            }
        }

        MixedRadixDoubleButterfly {
            width: width,
            width_size_fft: width_fft,

            height: height,
            height_size_fft: height_fft,

            twiddles: twiddles.into_boxed_slice(),
            inverse: inverse
        }
    }


    unsafe fn perform_fft(&self, input: &mut [Complex<T>], output: &mut [Complex<T>]) {
        // SIX STEP FFT:

        // STEP 1: transpose
        array_utils::transpose_small(self.width, self.height, input, output);

        // STEP 2: perform FFTs of size 'height'
        self.height_size_fft.process_multi_inplace(output);

        // STEP 3: Apply twiddle factors
        for (element, &twiddle) in output.iter_mut().zip(self.twiddles.iter()) {
            *element = *element * twiddle;
        }

        // STEP 4: transpose again
        array_utils::transpose_small(self.height, self.width, output, input);

        // STEP 5: perform FFTs of size 'width'
        self.width_size_fft.process_multi_inplace(input);

        // STEP 6: transpose again
        array_utils::transpose_small(self.width, self.height, input, output);
    }
}

impl<T: FFTnum> FFT<T> for MixedRadixDoubleButterfly<T> {
    fn process(&self, input: &mut [Complex<T>], output: &mut [Complex<T>]) {
        verify_length(input, output, self.len());

        unsafe { self.perform_fft(input, output) };
    }
    fn process_multi(&self, input: &mut [Complex<T>], output: &mut [Complex<T>]) {
        verify_length_divisible(input, output, self.len());

        for (in_chunk, out_chunk) in input.chunks_mut(self.len()).zip(output.chunks_mut(self.len())) {
            unsafe { self.perform_fft(in_chunk, out_chunk) };
        }
    }
}
impl<T> Length for MixedRadixDoubleButterfly<T> {
    #[inline(always)]
    fn len(&self) -> usize {
        self.twiddles.len()
    }
}
impl<T> IsInverse for MixedRadixDoubleButterfly<T> {
    #[inline(always)]
    fn is_inverse(&self) -> bool {
        self.inverse
    }
}




#[cfg(test)]
mod unit_tests {
    use super::*;
    use std::sync::Arc;
    use test_utils::{check_fft_algorithm, make_butterfly};
    use algorithm::DFT;

    #[test]
    fn test_mixed_radix() {
        for width in 1..7 {
            for height in 1..7 {
                test_mixed_radix_with_lengths(width, height, false);
                test_mixed_radix_with_lengths(width, height, true);
            }
        }
    }

    #[test]
    fn test_mixed_radix_double_butterfly() {
        for width in 2..7 {
            for height in 2..7 {
                test_mixed_radix_butterfly_with_lengths(width, height, false);
                test_mixed_radix_butterfly_with_lengths(width, height, true);
            }
        }
    }




    fn test_mixed_radix_with_lengths(width: usize, height: usize, inverse: bool) {
        let width_fft = Arc::new(DFT::new(width, inverse)) as Arc<FFT<f32>>;
        let height_fft = Arc::new(DFT::new(height, inverse)) as Arc<FFT<f32>>;

        let fft = MixedRadix::new(width_fft, height_fft);

        check_fft_algorithm(&fft, width * height, inverse);
    }

    fn test_mixed_radix_butterfly_with_lengths(width: usize, height: usize, inverse: bool) {
        let width_fft = make_butterfly(width, inverse);
        let height_fft = make_butterfly(height, inverse);

        let fft = MixedRadixDoubleButterfly::new(width_fft, height_fft);

        check_fft_algorithm(&fft, width * height, inverse);
    }
}
