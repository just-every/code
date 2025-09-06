use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::Length;

use twiddles;
use common;
use ::{DCT4, DST4, TransformType2And3, TransformType4};

/// DCT4 and DST4 implementation that converts the problem into two DCT3 of half size.
/// 
/// If the inner DCT3 is  O(nlogn), then so is this. This algorithm can only be used if the problem size is even.
///
/// ~~~
/// // Computes a DCT Type 4 of size 1234
/// use std::sync::Arc;
/// use rustdct::DCT4;
/// use rustdct::algorithm::Type4ConvertToType3Even;
/// use rustdct::DCTplanner;
///
/// let len = 1234;
/// let mut input:  Vec<f32> = vec![0f32; len];
/// let mut output: Vec<f32> = vec![0f32; len];
///
/// let mut planner = DCTplanner::new();
/// let inner_dct3 = planner.plan_dct3(len / 2);
/// 
/// let dct = Type4ConvertToType3Even::new(inner_dct3);
/// dct.process_dct4(&mut input, &mut output);
/// ~~~
pub struct Type4ConvertToType3Even<T> {
    inner_dct: Arc<TransformType2And3<T>>,
    twiddles: Box<[Complex<T>]>,
}

impl<T: common::DCTnum> Type4ConvertToType3Even<T> {
    /// Creates a new DCT4 context that will process signals of length `inner_dct.len() * 2`.
    pub fn new(inner_dct: Arc<TransformType2And3<T>>) -> Self {
        let inner_len = inner_dct.len();
        let len = inner_len * 2;


        let twiddles: Vec<Complex<T>> = (0..inner_len)
            .map(|i| twiddles::single_twiddle(2 * i + 1, len * 8).conj())
            .collect();

        Type4ConvertToType3Even {
            inner_dct: inner_dct,
            twiddles: twiddles.into_boxed_slice(),
        }
    }
}
impl<T: common::DCTnum> DCT4<T> for Type4ConvertToType3Even<T> {
    fn process_dct4(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        let len = self.len();
        let inner_len = len / 2;

        //pre-process the input by splitting into into two arrays, one for the inner DCT3, and the other for the DST3
        let (mut output_left, mut output_right) = output.split_at_mut(inner_len);

        output_left[0] = input[0] * T::two();
        for k in 1..inner_len {
            output_left[k] =        input[2 * k - 1] + input[2 * k];
            output_right[k - 1] =   input[2 * k - 1] - input[2 * k];
        }
        output_right[inner_len - 1] = input[len - 1] * T::two();

        //run the two inner DCTs on our separated arrays
        let (mut inner_result_cos, mut inner_result_sin) = input.split_at_mut(inner_len);

        self.inner_dct.process_dct3(&mut output_left, &mut inner_result_cos);
        self.inner_dct.process_dst3(&mut output_right, &mut inner_result_sin);

        //post-process the data by combining it back into a single array
        for k in 0..inner_len {
            let twiddle = self.twiddles[k];
            let cos_value = inner_result_cos[k];
            let sin_value = inner_result_sin[k];

            output_left[k] =                  cos_value * twiddle.re + sin_value * twiddle.im;
            output_right[inner_len - 1 - k] = cos_value * twiddle.im - sin_value * twiddle.re;
        }
    }
}
impl<T: common::DCTnum> DST4<T> for Type4ConvertToType3Even<T> {
    fn process_dst4(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        let len = self.len();
        let inner_len = len / 2;

        //pre-process the input by splitting into into two arrays, one for the inner DCT3, and the other for the DST3
        let (mut output_left, mut output_right) = output.split_at_mut(inner_len);

        output_right[0] = input[0] * T::two();
        for k in 1..inner_len {
            output_left[k - 1] =  input[2 * k - 1] + input[2 * k];
            output_right[k] =     input[2 * k] - input[2 * k - 1];
        }
        output_left[inner_len - 1] = input[len - 1] * T::two();

        //run the two inner DCTs on our separated arrays
        let (mut inner_result_cos, mut inner_result_sin) = input.split_at_mut(inner_len);

        self.inner_dct.process_dst3(&mut output_left, &mut inner_result_cos);
        self.inner_dct.process_dct3(&mut output_right, &mut inner_result_sin);

        //post-process the data by combining it back into a single array
        for k in 0..inner_len {
            let twiddle = self.twiddles[k];
            let cos_value = inner_result_cos[k];
            let sin_value = inner_result_sin[k];

            output_left[k] =                  cos_value * twiddle.re + sin_value * twiddle.im;
            output_right[inner_len - 1 - k] = sin_value * twiddle.re - cos_value * twiddle.im;
        }
    }
}
impl<T: common::DCTnum> TransformType4<T> for Type4ConvertToType3Even<T>{}
impl<T> Length for Type4ConvertToType3Even<T> {
    fn len(&self) -> usize {
        self.twiddles.len() * 2
    }
}


#[cfg(test)]
mod test {
    use super::*;
    use test_utils::{compare_float_vectors, random_signal};
    use algorithm::{Type2And3Naive, Type4Naive};

    #[test]
    fn unittest_dct4_via_type3() {
        for inner_size in 1..20 {
            let size = inner_size * 2;

            let mut expected_input = random_signal(size);
            let mut actual_input = expected_input.clone();

            let mut expected_output = vec![0f32; size];
            let mut actual_output = vec![0f32; size];

            
            let mut naive_dct4 = Type4Naive::new(size);
            naive_dct4.process_dct4(&mut expected_input, &mut expected_output);

            let inner_dct3 = Arc::new(Type2And3Naive::new(inner_size));
            let mut dct = Type4ConvertToType3Even::new(inner_dct3);
            dct.process_dct4(&mut actual_input, &mut actual_output);

            println!("");
            println!("expected: {:?}", expected_output);
            println!("actual:   {:?}", actual_output);

            assert!(
                compare_float_vectors(&expected_output, &actual_output),
                "len = {}",
                size
            );
        }
    }

    #[test]
    fn unittest_dst4_via_type3() {
        for inner_size in 1..20 {
            let size = inner_size * 2;

            let mut expected_input = random_signal(size);
            let mut actual_input = expected_input.clone();

            let mut expected_output = vec![0f32; size];
            let mut actual_output = vec![0f32; size];

            
            let mut naive_dst4 = Type4Naive::new(size);
            naive_dst4.process_dst4(&mut expected_input, &mut expected_output);

            let inner_dct3 = Arc::new(Type2And3Naive::new(inner_size));
            let mut dct = Type4ConvertToType3Even::new(inner_dct3);
            dct.process_dst4(&mut actual_input, &mut actual_output);

            println!("");
            println!("expected: {:?}", expected_output);
            println!("actual:   {:?}", actual_output);

            assert!(
                compare_float_vectors(&expected_output, &actual_output),
                "len = {}",
                size
            );
        }
    }
}
