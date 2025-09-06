use std::sync::Arc;

use rustfft::num_traits::Zero;
use rustfft::num_complex::Complex;
use rustfft::{FFT, Length};

use common;
use ::{DCT4, DST4, TransformType4};

/// DCT Type 4 and DST Type 4 implementation that converts the problem into a FFT of the same size. 
///
/// This algorithm can only be used if the problem size is odd.
///
/// ~~~
/// // Computes a DCT Type 4 and DST Type 4 of size 1233
/// use rustdct::{DCT4, DST4};
/// use rustdct::algorithm::Type4ConvertToFFTOdd;
/// use rustdct::rustfft::FFTplanner;
/// 
/// let len = 1233;
/// let mut planner = FFTplanner::new(false);
/// let fft = planner.plan_fft(len);
/// let dct = Type4ConvertToFFTOdd::new(fft);
///
/// let mut dct4_input:  Vec<f32> = vec![0f32; len];
/// let mut dct4_output: Vec<f32> = vec![0f32; len];
/// dct.process_dct4(&mut dct4_input, &mut dct4_output);
/// 
/// let mut dst4_input:  Vec<f32> = vec![0f32; len];
/// let mut dst4_output: Vec<f32> = vec![0f32; len];
/// dct.process_dst4(&mut dst4_input, &mut dst4_output);
/// ~~~
pub struct Type4ConvertToFFTOdd<T> {
    fft: Arc<FFT<T>>,
}

impl<T: common::DCTnum> Type4ConvertToFFTOdd<T> {
    /// Creates a new DCT4 context that will process signals of length `inner_fft.len()`. `inner_fft.len()` must be odd.
    pub fn new(inner_fft: Arc<FFT<T>>) -> Self {
        assert!(
            !inner_fft.is_inverse(),
            "Type4ConvertToFFTOdd requires a forward FFT, but an inverse FFT was provided");

        let len = inner_fft.len();

        assert!(len % 2 == 1, "Type4ConvertToFFTOdd size must be odd. Got {}", len);

        Type4ConvertToFFTOdd {
            fft: inner_fft,
        }
    }
}

impl<T: common::DCTnum> DCT4<T> for Type4ConvertToFFTOdd<T> {
    fn process_dct4(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        let len = self.len();
        let half_len = len / 2;
        let quarter_len = len / 4;

        let mut buffer = vec![Complex::zero(); len * 2];
        let (mut fft_input, mut fft_output) = buffer.split_at_mut(len);

        //start by reordering the input into the FFT input
        let mut input_index = half_len;
        let mut fft_index = 0;
        while input_index < len {
            fft_input[fft_index] = Complex{ re: input[input_index], im: T::zero() };

            input_index += 4;
            fft_index += 1;
        }

        //subtract len to simulate modular arithmetic
        input_index = input_index - len;
        while input_index < len {
            fft_input[fft_index] = Complex{ re: -input[len - input_index - 1], im: T::zero() };

            input_index += 4;
            fft_index += 1;
        }

        input_index = input_index - len;
        while input_index < len {
            fft_input[fft_index] = Complex{ re: -input[input_index], im: T::zero() };

            input_index += 4;
            fft_index += 1;
        }

        input_index = input_index - len;
        while input_index < len {
            fft_input[fft_index] = Complex{ re: input[len - input_index - 1], im: T::zero() };

            input_index += 4;
            fft_index += 1;
        }

        input_index = input_index - len;
        while fft_index < len {
            fft_input[fft_index] = Complex{ re: input[input_index], im: T::zero() };

            input_index += 4;
            fft_index += 1;
        }

        // run the fft
        self.fft.process(&mut fft_input, &mut fft_output);

        let result_scale = T::SQRT_2() * T::half();
        let second_half_sign = if len % 4 == 1 { T::one() } else { -T::one() };

        //post-process the results 4 at a time
        let mut output_sign = T::one();
        for i in 0..quarter_len {
            let fft_result = fft_output[4 * i + 1] * (output_sign * result_scale);
            let next_result = fft_output[4 * i + 3] * (output_sign * result_scale);

            output[i * 2] =           fft_result.re + fft_result.im;
            output[i * 2 + 1] =      -next_result.re + next_result.im;

            output[len - i * 2 - 2] = (next_result.re + next_result.im) * second_half_sign;
            output[len - i * 2 - 1] = (fft_result.re - fft_result.im) * second_half_sign;

            output_sign = output_sign.neg();
        }

        //we either have 1 or 3 elements left over that we couldn't get in the above loop, handle them here
        if len % 4 == 1 {
            output[half_len] = fft_output[0].re * output_sign * result_scale;
        }
        else {
            let fft_result = fft_output[len - 2] * (output_sign * result_scale);

            output[half_len - 1] =  fft_result.re + fft_result.im;
            output[half_len + 1] = -fft_result.re + fft_result.im;
            output[half_len] =     -fft_output[0].re * output_sign * result_scale;
        }
        
    }
}
impl<T: common::DCTnum> DST4<T> for Type4ConvertToFFTOdd<T> {
    fn process_dst4(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        self.process_dct4(input, output);

        let len = self.len();
        let half_len = len / 2;
        let quarter_len = len / 4;

        let mut buffer = vec![Complex::zero(); len * 2];
        let (mut fft_input, mut fft_output) = buffer.split_at_mut(len);

        //start by reordering the input into the FFT input
        let mut input_index = half_len;
        let mut fft_index = 0;
        while input_index < len {
            fft_input[fft_index] = Complex{ re: input[len - input_index - 1], im: T::zero() };

            input_index += 4;
            fft_index += 1;
        }

        //subtract len to simulate modular arithmetic
        input_index = input_index - len;
        while input_index < len {
            fft_input[fft_index] = Complex{ re: -input[input_index], im: T::zero() };

            input_index += 4;
            fft_index += 1;
        }

        input_index = input_index - len;
        while input_index < len {
            fft_input[fft_index] = Complex{ re: -input[len - input_index - 1], im: T::zero() };

            input_index += 4;
            fft_index += 1;
        }

        input_index = input_index - len;
        while input_index < len {
            fft_input[fft_index] = Complex{ re: input[input_index], im: T::zero() };

            input_index += 4;
            fft_index += 1;
        }

        input_index = input_index - len;
        while fft_index < len {
            fft_input[fft_index] = Complex{ re: input[len - input_index - 1], im: T::zero() };

            input_index += 4;
            fft_index += 1;
        }

        // run the fft
        self.fft.process(&mut fft_input, &mut fft_output);

        let result_scale = T::SQRT_2() * T::half();
        let second_half_sign = if len % 4 == 1 { T::one() } else { -T::one() };

        //post-process the results 4 at a time
        let mut output_sign = T::one();
        for i in 0..quarter_len {
            let fft_result = fft_output[4 * i + 1] * (output_sign * result_scale);
            let next_result = fft_output[4 * i + 3] * (output_sign * result_scale);

            output[i * 2] =           fft_result.re + fft_result.im;
            output[i * 2 + 1] =       next_result.re - next_result.im;

            output[len - i * 2 - 2] = -(next_result.re + next_result.im) * second_half_sign;
            output[len - i * 2 - 1] = (fft_result.re - fft_result.im) * second_half_sign;

            output_sign = output_sign.neg();
        }

        //we either have 1 or 3 elements left over that we couldn't get in the above loop, handle them here
        if len % 4 == 1 {
            output[half_len] = fft_output[0].re * output_sign * result_scale;
        }
        else {
            let fft_result = fft_output[len - 2] * (output_sign * result_scale);

            output[half_len - 1] =  fft_result.re + fft_result.im;
            output[half_len + 1] = -fft_result.re + fft_result.im;
            output[half_len] =      fft_output[0].re * output_sign * result_scale;
        }
        
    }
}
impl<T: common::DCTnum> TransformType4<T> for Type4ConvertToFFTOdd<T>{}
impl<T> Length for Type4ConvertToFFTOdd<T> {
    fn len(&self) -> usize {
        self.fft.len()
    }
}


#[cfg(test)]
mod test {
    use super::*;
    use algorithm::Type4Naive;

    use test_utils::{compare_float_vectors, random_signal};
    use rustfft::FFTplanner;

    /// Verify that our fast implementation of the DCT4 gives the same output as the slow version, for many different inputs
    #[test]
    fn test_dct4_via_fft_odd() {
        for n in 0..50 {
            let size = 2 * n + 1;
            println!("{}", size);

            let mut expected_input = random_signal(size);
            let mut actual_input = expected_input.clone();

            println!("input: {:?}", actual_input);

            let mut expected_output = vec![0f32; size];
            let mut actual_output = vec![0f32; size];

            let mut naive_dct = Type4Naive::new(size);
            naive_dct.process_dct4(&mut expected_input, &mut expected_output);

            let mut fft_planner = FFTplanner::new(false);
            let mut dct = Type4ConvertToFFTOdd::new(fft_planner.plan_fft(size));
            dct.process_dct4(&mut actual_input, &mut actual_output);

            println!("expected: {:?}", expected_output);
            println!("actual: {:?}", actual_output);

            assert!(
                compare_float_vectors(&actual_output, &expected_output),
                "len = {}",
                size
            );
        }
    }

    /// Verify that our fast implementation of the DST4 gives the same output as the slow version, for many different inputs
    #[test]
    fn test_dst4_via_fft_odd() {
        for n in 0..50 {
            let size = 2 * n + 1;
            println!("{}", size);

            let mut expected_input = random_signal(size);
            let mut actual_input = expected_input.clone();

            println!("input: {:?}", actual_input);

            let mut expected_output = vec![0f32; size];
            let mut actual_output = vec![0f32; size];

            let mut naive_dct = Type4Naive::new(size);
            naive_dct.process_dst4(&mut expected_input, &mut expected_output);

            let mut fft_planner = FFTplanner::new(false);
            let mut dct = Type4ConvertToFFTOdd::new(fft_planner.plan_fft(size));
            dct.process_dst4(&mut actual_input, &mut actual_output);

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
