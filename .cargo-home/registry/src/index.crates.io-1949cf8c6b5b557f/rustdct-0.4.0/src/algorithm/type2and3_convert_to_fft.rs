use std::sync::Arc;

use rustfft::num_traits::Zero;
use rustfft::num_complex::Complex;
use rustfft::{FFT, Length};

use common;
use twiddles;
use ::{DCT2, DST2, DCT3, DST3, TransformType2And3};

/// DCT2, DST2, DCT3, and DST3 implementation that converts the problem into a FFT of the same size
///
/// ~~~
/// // Computes a O(NlogN) DCT2, DST2, DCT3, and DST3 of size 1234 by converting them to FFTs
/// use rustdct::{DCT2, DST2, DCT3, DST3};
/// use rustdct::algorithm::Type2And3ConvertToFFT;
/// use rustdct::rustfft::FFTplanner;
///
/// let len = 1234;
/// let mut planner = FFTplanner::new(false);
/// let fft = planner.plan_fft(len);
///
/// let dct = Type2And3ConvertToFFT::new(fft);
/// 
/// let mut dct2_input:  Vec<f32> = vec![0f32; len];
/// let mut dct2_output: Vec<f32> = vec![0f32; len];
/// dct.process_dct2(&mut dct2_input, &mut dct2_output);
/// 
/// let mut dst2_input:  Vec<f32> = vec![0f32; len];
/// let mut dst2_output: Vec<f32> = vec![0f32; len];
/// dct.process_dst2(&mut dst2_input, &mut dst2_output);
/// 
/// let mut dct3_input:  Vec<f32> = vec![0f32; len];
/// let mut dct3_output: Vec<f32> = vec![0f32; len];
/// dct.process_dct3(&mut dct3_input, &mut dct3_output);
/// 
/// let mut dst3_input:  Vec<f32> = vec![0f32; len];
/// let mut dst3_output: Vec<f32> = vec![0f32; len];
/// dct.process_dst3(&mut dst3_input, &mut dst3_output);
/// ~~~
pub struct Type2And3ConvertToFFT<T> {
    fft: Arc<FFT<T>>,
    twiddles: Box<[Complex<T>]>,
}

impl<T: common::DCTnum> Type2And3ConvertToFFT<T> {
    /// Creates a new DCT2, DST2, DCT3, and DST3 context that will process signals of length `inner_fft.len()`.
    pub fn new(inner_fft: Arc<FFT<T>>) -> Self {
        assert!(
            !inner_fft.is_inverse(),
            "The 'DCT type 2 via FFT' algorithm requires a forward FFT, but an inverse FFT \
                 was provided"
        );

        let len = inner_fft.len();

        let twiddles: Vec<Complex<T>> = (0..len)
            .map(|i| twiddles::single_twiddle(i, len * 4))
            .collect();

        Type2And3ConvertToFFT {
            fft: inner_fft,
            twiddles: twiddles.into_boxed_slice(),
        }
    }
}

impl<T: common::DCTnum> DCT2<T> for Type2And3ConvertToFFT<T> {
    fn process_dct2(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        let mut buffer = vec![Complex::zero(); self.len() * 2];
        let (fft_input, fft_output) = buffer.split_at_mut(self.len());

        // the first half of the array will be the even elements, in order
        let even_end = (input.len() + 1) / 2;
        for i in 0..even_end {
            fft_input[i] = Complex::from(input[i * 2]);
        }

        // the second half is the odd elements, in reverse order
        let odd_end = input.len() - 1 - input.len() % 2;
        for i in 0..input.len() / 2 {
            fft_input[even_end + i] = Complex::from(input[odd_end - 2 * i]);
        }

        // run the fft
        self.fft.process(fft_input, fft_output);

        // apply a correction factor to the result
        for ((fft_entry, correction_entry), spectrum_entry) in
            fft_output.iter().zip(self.twiddles.iter()).zip(output.iter_mut())
        {
            *spectrum_entry = (fft_entry * correction_entry).re;
        }
    }
}
impl<T: common::DCTnum> DST2<T> for Type2And3ConvertToFFT<T> {
    fn process_dst2(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        let mut buffer = vec![Complex::zero(); self.len() * 2];
        let (fft_input, fft_output) = buffer.split_at_mut(self.len());

        // the first half of the array will be the even elements, in order
        let even_end = (input.len() + 1) / 2;
        for i in 0..even_end {
            fft_input[i] = Complex::from(input[i * 2]);
        }

        // the second half is the odd elements, in reverse order and negated
        let odd_end = input.len() - 1 - input.len() % 2;
        for i in 0..input.len() / 2 {
            fft_input[even_end + i] = Complex::from(-input[odd_end - 2 * i]);
        }

        // run the fft
        self.fft.process(fft_input, fft_output);

        // apply a correction factor to the result, and put it in reversed order in the output buffer
        for ((fft_entry, correction_entry), spectrum_entry) in
            fft_output.iter().zip(self.twiddles.iter()).zip(output.iter_mut().rev())
        {
            *spectrum_entry = (fft_entry * correction_entry).re;
        }
    }
}
impl<T: common::DCTnum> DCT3<T> for Type2And3ConvertToFFT<T> {
    fn process_dct3(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        let half = T::half();

        let mut buffer = vec![Complex::zero(); self.len() * 2];
        let (mut fft_input, mut fft_output) = buffer.split_at_mut(self.len());

        // compute the FFT input based on the correction factors
        fft_input[0] = Complex::from(input[0] * half);

        for (i, (fft_input_element, twiddle)) in fft_input.iter_mut().zip(self.twiddles.iter()).enumerate().skip(1) {
            let c = Complex {
                re: input[i],
                im: input[input.len() - i],
            };
            *fft_input_element = c * twiddle * half;
        }

        // run the fft
        self.fft.process(&mut fft_input, &mut fft_output);

        // copy the first half of the fft output into the even elements of the output
        let even_end = (input.len() + 1) / 2;
        for i in 0..even_end {
            output[i * 2] = fft_output[i].re;
        }

        // copy the second half of the fft output into the odd elements, reversed
        let odd_end = input.len() - 1 - input.len() % 2;
        for i in 0..input.len() / 2 {
            output[odd_end - 2 * i] = fft_output[i + even_end].re;
        }
    }
}
impl<T: common::DCTnum> DST3<T> for Type2And3ConvertToFFT<T> {
    fn process_dst3(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        let half = T::half();
        
        let mut buffer = vec![Complex::zero(); self.len() * 2];
        let (mut fft_input, mut fft_output) = buffer.split_at_mut(self.len());

        // compute the FFT input based on the correction factors
        fft_input[0] = Complex::from(input[input.len() - 1] * half);

        for (i, (fft_input_element, twiddle)) in fft_input.iter_mut().zip(self.twiddles.iter()).enumerate().skip(1) {
            let c = Complex {
                re: input[input.len() - i - 1],
                im: input[i - 1],
            };
            *fft_input_element = c * twiddle * half;
        }

        // run the fft
        self.fft.process(&mut fft_input, &mut fft_output);

        // copy the first half of the fft output into the even elements of the output
        let even_end = (self.len() + 1) / 2;
        for i in 0..even_end {
            output[i * 2] = fft_output[i].re;
        }

        // copy the second half of the fft output into the odd elements, reversed
        let odd_end = self.len() - 1 - self.len() % 2;
        for i in 0..self.len() / 2 {
            output[odd_end - 2 * i] = -fft_output[i + even_end].re;
        }
    }
}
impl<T: common::DCTnum> TransformType2And3<T> for Type2And3ConvertToFFT<T>{}
impl<T> Length for Type2And3ConvertToFFT<T> {
    fn len(&self) -> usize {
        self.twiddles.len()
    }
}


#[cfg(test)]
mod test {
    use super::*;
    use algorithm::Type2And3Naive;

    use test_utils::{compare_float_vectors, random_signal};
    use rustfft::FFTplanner;

    /// Verify that our fast implementation of the DCT2 gives the same output as the naive version, for many different inputs
    #[test]
    fn test_dct2_via_fft() {
        for size in 2..20 {
            let mut expected_input = random_signal(size);
            let mut actual_input = random_signal(size);

            let mut expected_output = vec![0f32; size];
            let mut actual_output = vec![0f32; size];

            let naive_dct = Type2And3Naive::new(size);
            naive_dct.process_dct2(&mut expected_input, &mut expected_output);

            let mut fft_planner = FFTplanner::new(false);
            let dct = Type2And3ConvertToFFT::new(fft_planner.plan_fft(size));
            dct.process_dct2(&mut actual_input, &mut actual_output);

            println!("{}", size);
            println!("expected: {:?}", expected_output);
            println!("actual: {:?}", actual_output);

            assert!(
                compare_float_vectors(&actual_output, &expected_output),
                "len = {}",
                size
            );
        }
    }

    /// Verify that our fast implementation of the DST2 gives the same output as the naive version, for many different inputs
    #[test]
    fn test_dst2_via_fft() {
        for size in 2..20 {
            let mut expected_input = random_signal(size);
            let mut actual_input = random_signal(size);

            let mut expected_output = vec![0f32; size];
            let mut actual_output = vec![0f32; size];

            let naive_dst = Type2And3Naive::new(size);
            naive_dst.process_dst2(&mut expected_input, &mut expected_output);

            let mut fft_planner = FFTplanner::new(false);
            let dst = Type2And3ConvertToFFT::new(fft_planner.plan_fft(size));
            dst.process_dst2(&mut actual_input, &mut actual_output);

            println!("{}", size);
            println!("expected: {:?}", expected_output);
            println!("actual: {:?}", actual_output);

            assert!(
                compare_float_vectors(&actual_output, &expected_output),
                "len = {}",
                size
            );
        }
    }

    /// Verify that our fast implementation of the DCT3 gives the same output as the naive version, for many different inputs
    #[test]
    fn test_dct3_via_fft() {
        for size in 2..20 {
            let mut expected_input = random_signal(size);
            let mut actual_input = random_signal(size);

            let mut expected_output = vec![0f32; size];
            let mut actual_output = vec![0f32; size];

            let naive_dct = Type2And3Naive::new(size);
            naive_dct.process_dct3(&mut expected_input, &mut expected_output);

            let mut fft_planner = FFTplanner::new(false);
            let dct = Type2And3ConvertToFFT::new(fft_planner.plan_fft(size));
            dct.process_dct3(&mut actual_input, &mut actual_output);

            println!("{}", size);
            println!("expected: {:?}", expected_output);
            println!("actual: {:?}", actual_output);

            assert!(
                compare_float_vectors(&actual_output, &expected_output),
                "len = {}",
                size
            );
        }
    }

    /// Verify that our fast implementation of the DST3 gives the same output as the naive version, for many different inputs
    #[test]
    fn test_dst3_via_fft() {
        for size in 2..20 {
            let mut expected_input = random_signal(size);
            let mut actual_input = random_signal(size);

            let mut expected_output = vec![0f32; size];
            let mut actual_output = vec![0f32; size];

            let naive_dst = Type2And3Naive::new(size);
            naive_dst.process_dst3(&mut expected_input, &mut expected_output);

            let mut fft_planner = FFTplanner::new(false);
            let dst = Type2And3ConvertToFFT::new(fft_planner.plan_fft(size));
            dst.process_dst3(&mut actual_input, &mut actual_output);

            println!("{}", size);
            println!("expected: {:?}", expected_output);
            println!("actual: {:?}", actual_output);

            assert!(
                compare_float_vectors(&actual_output, &expected_output),
                "len = {}",
                size
            );
        }
    }
}
