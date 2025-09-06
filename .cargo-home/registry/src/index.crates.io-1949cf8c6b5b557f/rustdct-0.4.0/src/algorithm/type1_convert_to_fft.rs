use std::sync::Arc;

use rustfft::num_traits::Zero;
use rustfft::num_complex::Complex;
use rustfft::{FFT, Length};

use common;
use ::{DCT1, DST1};

/// DCT Type 1 implementation that converts the problem into a FFT of size 2 * (n - 1)
///
/// ~~~
/// // Computes a DCT Type 1 of size 1234
/// use rustdct::DCT1;
/// use rustdct::algorithm::DCT1ConvertToFFT;
/// use rustdct::rustfft::FFTplanner;
///
/// let len = 1234;
/// let mut input:  Vec<f32> = vec![0f32; len];
/// let mut output: Vec<f32> = vec![0f32; len];
///
/// let mut planner = FFTplanner::new(false);
/// let fft = planner.plan_fft(2 * (len - 1));
///
/// let dct = DCT1ConvertToFFT::new(fft);
/// dct.process_dct1(&mut input, &mut output);
pub struct DCT1ConvertToFFT<T> {
    fft: Arc<FFT<T>>,
}

impl<T: common::DCTnum> DCT1ConvertToFFT<T> {
    /// Creates a new DCT1 context that will process signals of length `inner_fft.len() / 2 + 1`.
    pub fn new(inner_fft: Arc<FFT<T>>) -> Self {
        let inner_len = inner_fft.len();

        assert!(
            inner_len % 2 == 0,
            "For DCT1 via FFT, the inner FFT size must be even. Got {}",
            inner_len
        );
        assert!(
            !inner_fft.is_inverse(),
            "The 'DCT type 1 via FFT' algorithm requires a forward FFT, but an inverse FFT \
                 was provided"
        );

        DCT1ConvertToFFT {
            fft: inner_fft,
        }
    }
}

impl<T: common::DCTnum> DCT1<T> for DCT1ConvertToFFT<T> {
    fn process_dct1(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        let inner_len = self.fft.len();
        let mut buffer = vec![Complex::zero(); inner_len * 2];
        let (mut fft_input, mut fft_output) = buffer.split_at_mut(inner_len);

        for (&input_val, fft_cell) in input.iter().zip(&mut fft_input[..input.len()]) {
            *fft_cell = Complex {
                re: input_val,
                im: T::zero(),
            };
        }
        for (&input_val, fft_cell) in
            input.iter().rev().skip(1).zip(
                &mut fft_input[input.len()..],
            )
        {
            *fft_cell = Complex {
                re: input_val,
                im: T::zero(),
            };
        }

        // run the fft
        self.fft.process(&mut fft_input, &mut fft_output);

        // apply a correction factor to the result
        let half = T::half();
        for (fft_entry, output_val) in fft_output.iter().zip(output.iter_mut()) {
            *output_val = fft_entry.re * half;
        }
    }
}
impl<T> Length for DCT1ConvertToFFT<T> {
    fn len(&self) -> usize {
        self.fft.len() / 2 + 1
    }
}

/// DST Type 1 implementation that converts the problem into a FFT of size 2 * (n + 1)
///
/// ~~~
/// // Computes a DST Type 1 of size 1234
/// use rustdct::DST1;
/// use rustdct::algorithm::DST1ConvertToFFT;
/// use rustdct::rustfft::FFTplanner;
///
/// let len = 1234;
/// let mut input:  Vec<f32> = vec![0f32; len];
/// let mut output: Vec<f32> = vec![0f32; len];
///
/// let mut planner = FFTplanner::new(false);
/// let fft = planner.plan_fft(2 * (len + 1));
///
/// let dct = DST1ConvertToFFT::new(fft);
/// dct.process_dst1(&mut input, &mut output);
/// ~~~
pub struct DST1ConvertToFFT<T> {
    fft: Arc<FFT<T>>,
}

impl<T: common::DCTnum> DST1ConvertToFFT<T> {
    /// Creates a new DCT1 context that will process signals of length `inner_fft.len() / 2 - 1`.
    pub fn new(inner_fft: Arc<FFT<T>>) -> Self {
        let inner_len = inner_fft.len();

        assert!(
            inner_len % 2 == 0,
            "For DCT1 via FFT, the inner FFT size must be even. Got {}",
            inner_len
        );
        assert!(
            !inner_fft.is_inverse(),
            "The 'DCT type 1 via FFT' algorithm requires a forward FFT, but an inverse FFT \
                 was provided"
        );

        DST1ConvertToFFT {
            fft: inner_fft,
        }
    }
}

impl<T: common::DCTnum> DST1<T> for DST1ConvertToFFT<T> {
    fn process_dst1(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        let inner_len = self.fft.len();
        let mut buffer = vec![Complex::zero(); inner_len * 2];
        let (mut fft_input, mut fft_output) = buffer.split_at_mut(inner_len);

        // the first half of the FFT input will be a 0, followed by the input array
        for (input_val, fft_cell) in input.iter().zip(fft_input.iter_mut().skip(1)) {
            *fft_cell = Complex::from(input_val);
        }

        // the second half of the FFT input will be a 0, followed by the input array, reversed and negated
        for (input_val, fft_cell) in input.iter().zip(fft_input.iter_mut().rev()) {
            *fft_cell = Complex::from(-*input_val);
        }

        // run the fft
        self.fft.process(&mut fft_input, &mut fft_output);

        // apply a correction factor to the result
        let half = T::half();
        for (fft_entry, output_val) in fft_output.iter().rev().zip(output.iter_mut()) {
            *output_val = fft_entry.im * half;
        }
    }
}
impl<T> Length for DST1ConvertToFFT<T> {
    fn len(&self) -> usize {
        self.fft.len() / 2 - 1
    }
}


#[cfg(test)]
mod test {
    use super::*;
    use algorithm::{DCT1Naive, DST1Naive};

    use test_utils::{compare_float_vectors, random_signal};
    use rustfft::FFTplanner;

    /// Verify that our fast implementation of the DCT1 gives the same output as the slow version, for many different inputs
    #[test]
    fn test_dct1_via_fft() {
        for size in 2..20 {

            let mut expected_input = random_signal(size);
            let mut actual_input = expected_input.clone();


            let mut expected_output = vec![0f32; size];
            let mut actual_output = vec![0f32; size];

            let mut naive_dct = DCT1Naive::new(size);
            naive_dct.process_dct1(&mut expected_input, &mut expected_output);

            let mut fft_planner = FFTplanner::new(false);
            let inner_fft = fft_planner.plan_fft((size - 1) * 2);
            println!("size: {}", size);
            println!("inner fft len: {}", inner_fft.len());
            

            let mut dct = DCT1ConvertToFFT::new(inner_fft);
            println!("dct len: {}", dct.len());
            dct.process_dct1(&mut actual_input, &mut actual_output);

            assert!(
                compare_float_vectors(&actual_output, &expected_output),
                "len = {}",
                size
            );
        }
    }

    /// Verify that our fast implementation of the DST1 gives the same output as the slow version, for many different inputs
    #[test]
    fn test_dst1_via_fft() {
        for size in 2..20 {

            let mut expected_input = random_signal(size);
            let mut actual_input = expected_input.clone();

            let mut expected_output = vec![0f32; size];
            let mut actual_output = vec![0f32; size];

            let mut naive_dct = DST1Naive::new(size);
            naive_dct.process_dst1(&mut expected_input, &mut expected_output);

            let mut fft_planner = FFTplanner::new(false);
            let inner_fft = fft_planner.plan_fft((size + 1) * 2);
            println!("size: {}", size);
            println!("inner fft len: {}", inner_fft.len());

            let mut dct = DST1ConvertToFFT::new(inner_fft);
            println!("dst len: {}", dct.len());
            dct.process_dst1(&mut actual_input, &mut actual_output);

            assert!(
                compare_float_vectors(&actual_output, &expected_output),
                "len = {}",
                size
            );
        }
    }
}
