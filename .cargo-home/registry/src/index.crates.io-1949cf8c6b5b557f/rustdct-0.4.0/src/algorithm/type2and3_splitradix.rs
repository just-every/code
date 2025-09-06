use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::Length;

use common;
use twiddles;
use ::{DCT2, DST2, DCT3, DST3, TransformType2And3};

/// DCT2, DCT3, DST2, and DST3 implemention that recursively divides the problem in half.
/// 
/// The problem size must be 2^n, n > 1
///
/// ~~~
/// // Computes a DCT Type 2 of size 1024
/// use rustdct::algorithm::Type2And3SplitRadix;
/// use rustdct::DCT2;
/// use rustdct::DCTplanner;
///
/// let len = 1024;
/// let mut input:  Vec<f32> = vec![0f32; len];
/// let mut output: Vec<f32> = vec![0f32; len];
///
/// let mut planner = DCTplanner::new();
/// let quarter_dct = planner.plan_dct2(len / 4);
/// let half_dct = planner.plan_dct2(len / 2);
///
/// let dct = Type2And3SplitRadix::new(half_dct, quarter_dct);
/// dct.process_dct2(&mut input, &mut output);
/// ~~~
pub struct Type2And3SplitRadix<T> {
    half_dct: Arc<TransformType2And3<T>>,
    quarter_dct: Arc<TransformType2And3<T>>,
    twiddles: Box<[Complex<T>]>,
}

impl<T: common::DCTnum> Type2And3SplitRadix<T> {
    /// Creates a new DCT2, DCT3, DST2, and DST3 context that will process signals of length `half_dct.len() * 2`
    pub fn new(half_dct: Arc<TransformType2And3<T>>, quarter_dct: Arc<TransformType2And3<T>>) -> Self {
        let half_len = half_dct.len();
        let quarter_len = quarter_dct.len();
        let len = half_len * 2;

        assert!(
            len.is_power_of_two() && len > 2,
            "The DCT2SplitRadix algorithm requires a power-of-two input size greater than two. Got {}", len 
        );
        assert_eq!(half_len, quarter_len * 2,
            "half_dct.len() must be 2 * quarter_dct.len(). Got half_dct.len()={}, quarter_dct.len()={}", half_len, quarter_len
        );

        let twiddles: Vec<Complex<T>> = (0..(len/4))
            .map(|i| twiddles::single_twiddle(2 * i + 1, len * 4).conj())
            .collect();

        Type2And3SplitRadix {
            half_dct: half_dct,
            quarter_dct: quarter_dct,
            twiddles: twiddles.into_boxed_slice(),
        }
    }
}

impl<T: common::DCTnum> DCT2<T> for Type2And3SplitRadix<T> {
    fn process_dct2(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        let len = input.len();
        let half_len = len / 2;
        let quarter_len = len / 4;

        //preprocess the data by splitting it up into vectors of size n/2, n/4, and n/4
        unsafe {
            let (input_dct2, input_dct4) = output.split_at_mut(half_len);
            let (input_dct4_even, input_dct4_odd) = input_dct4.split_at_mut(quarter_len);

            for i in 0..quarter_len {
                let input_bottom = *input.get_unchecked(i);
                let input_top =    *input.get_unchecked(len - i - 1);

                let input_half_bottom = *input.get_unchecked(half_len - i - 1);
                let input_half_top =    *input.get_unchecked(half_len + i);

                //prepare the inner DCT2
                *input_dct2.get_unchecked_mut(i) =                input_top + input_bottom;
                *input_dct2.get_unchecked_mut(half_len - i - 1) = input_half_bottom + input_half_top;

                //prepare the inner DCT4 - which consists of two DCT2s of half size
                let lower_dct4 = input_bottom - input_top;
                let upper_dct4 = input_half_bottom - input_half_top;
                let twiddle = self.twiddles.get_unchecked(i);

                let cos_input = lower_dct4 * twiddle.re + upper_dct4 * twiddle.im;
                let sin_input = upper_dct4 * twiddle.re - lower_dct4 * twiddle.im;

                *input_dct4_even.get_unchecked_mut(i) = cos_input;
                *input_dct4_odd.get_unchecked_mut(quarter_len - i - 1) = if i % 2 == 0 {
                    sin_input
                } else {
                    -sin_input
                };
            }

            // compute the recursive DCT2s
            let (output_dct2, output_dct4) = input.split_at_mut(half_len);
            let (output_dct4_even, output_dct4_odd) = output_dct4.split_at_mut(quarter_len);

            self.half_dct.process_dct2(input_dct2, output_dct2);
            self.quarter_dct.process_dct2(input_dct4_even, output_dct4_even);
            self.quarter_dct.process_dct2(input_dct4_odd, output_dct4_odd);
        }



        let (output_dct2, output_dct4) = input.split_at(half_len);
        let (output_dct4_even, output_dct4_odd) = output_dct4.split_at(quarter_len);

        unsafe {
            //post process the 3 DCT2 outputs. the first few and the last will be done outside of the loop
            *output.get_unchecked_mut(0) = *output_dct2.get_unchecked(0);
            *output.get_unchecked_mut(1) = *output_dct4_even.get_unchecked(0);
            *output.get_unchecked_mut(2) = *output_dct2.get_unchecked(1);

            for i in 1..quarter_len {
                let dct4_cos_output = *output_dct4_even.get_unchecked(i);
                let dct4_sin_output = if (i + quarter_len) % 2 == 0 {
                    -*output_dct4_odd.get_unchecked(quarter_len - i)
                } else {
                    *output_dct4_odd.get_unchecked(quarter_len - i)
                };

                *output.get_unchecked_mut(i * 4 - 1) = dct4_cos_output + dct4_sin_output;
                *output.get_unchecked_mut(i * 4) = *output_dct2.get_unchecked(i * 2);

                *output.get_unchecked_mut(i * 4 + 1) =  dct4_cos_output - dct4_sin_output;
                *output.get_unchecked_mut(i * 4 + 2) = *output_dct2.get_unchecked(i * 2 + 1);
            }

            *output.get_unchecked_mut(len - 1) = -*output_dct4_odd.get_unchecked(0);
        }
    }
}
impl<T: common::DCTnum> DST2<T> for Type2And3SplitRadix<T> {
    fn process_dst2(&self, input: &mut [T], output: &mut [T]) {
        
        for i in 0..(self.len() / 2) {
            input[2 * i + 1] = input[2 * i + 1].neg();
        }

        self.process_dct2(input, output);

        output.reverse();
    }
}
impl<T: common::DCTnum> DCT3<T> for Type2And3SplitRadix<T> {
    fn process_dct3(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        let len = input.len();
        let half_len = len / 2;
        let quarter_len = len / 4;
        
        {
            // divide the output into 3 sub-lists to use for our inner DCTs, one of size N/2 and two of size N/4
            let (recursive_input_evens, recursive_input_odds) = output.split_at_mut(half_len);
            let (recursive_input_n1, recursive_input_n3) = recursive_input_odds.split_at_mut(quarter_len);

            // do the same pre-loop setup as DCT4ViaDCT3, and since we're skipping the first iteration of the loop we
            // to also set up the corresponding evens cells
            recursive_input_evens[0] = input[0];
            recursive_input_evens[1] = input[2];
            recursive_input_n1[0] = input[1] * T::two();
            recursive_input_n3[0] = input[len - 1] * T::two();

            // populate the recursive input arrays
            for i in 1..quarter_len {
                let k = 4 * i;

                unsafe {
                    // the evens are the easy ones - just copy straight over
                    *recursive_input_evens.get_unchecked_mut(i * 2) =     *input.get_unchecked(k);
                    *recursive_input_evens.get_unchecked_mut(i * 2 + 1) = *input.get_unchecked(k + 2);

                    // for the odd ones we're going to do the same addition/subtraction we do in the setup for DCT4ViaDCT3
                    *recursive_input_n1.get_unchecked_mut(i) =               *input.get_unchecked(k - 1) + *input.get_unchecked(k + 1);
                    *recursive_input_n3.get_unchecked_mut(quarter_len - i) = *input.get_unchecked(k - 1) - *input.get_unchecked(k + 1);
                }
            }

            //now that we're done with the input, divide it up the same way we did the output
            let (recursive_output_evens, recursive_output_odds) = input.split_at_mut(half_len);
            let (recursive_output_n1, recursive_output_n3) = recursive_output_odds.split_at_mut(quarter_len);

            //perform our recursive DCTs
            self.half_dct.process_dct3(recursive_input_evens, recursive_output_evens);
            self.quarter_dct.process_dct3(recursive_input_n1, recursive_output_n1);
            self.quarter_dct.process_dct3(recursive_input_n3, recursive_output_n3);
        }

        //we want the input array to stay split, but to placate the borrow checker it's easier to just re-split it
        let (recursive_output_evens, recursive_output_odds) = input.split_at(half_len);
        let (recursive_output_n1, recursive_output_n3) = recursive_output_odds.split_at(quarter_len);

        //merge the results. we're going to combine 2 separate things:
        // - merging the two smaller DCT3 outputs into a DCT4 output
        // - marging the DCT4 outputand the larger DCT3 output into the final output
        for i in 0..quarter_len {
            let twiddle = self.twiddles[i];
            let cosine_value = recursive_output_n1[i];

            // flip the sign of every other sine value to finish the job of using a DCT3 to compute a DST3
            let sine_value = if i % 2 == 0 {
                recursive_output_n3[i]
            } else {
                -recursive_output_n3[i]
            };

            let lower_dct4 = cosine_value * twiddle.re + sine_value * twiddle.im;
            let upper_dct4 = cosine_value * twiddle.im - sine_value * twiddle.re;

            unsafe {
                let lower_dct3 = *recursive_output_evens.get_unchecked(i);
                let upper_dct3 = *recursive_output_evens.get_unchecked(half_len - i - 1);

                *output.get_unchecked_mut(i) =                lower_dct3 + lower_dct4;
                *output.get_unchecked_mut(len - i - 1) =      lower_dct3 - lower_dct4;

                *output.get_unchecked_mut(half_len - i - 1) = upper_dct3 + upper_dct4;
                *output.get_unchecked_mut(half_len + i) =     upper_dct3 - upper_dct4;
            }
        }
    }
}
impl<T: common::DCTnum> DST3<T> for Type2And3SplitRadix<T> {
    fn process_dst3(&self, input: &mut [T], output: &mut [T]) {
        
        input.reverse();

        self.process_dct3(input, output);

        for i in 0..(self.len() / 2) {
            output[2 * i + 1] = output[2 * i + 1].neg();
        }
    }
}
impl<T: common::DCTnum> TransformType2And3<T> for Type2And3SplitRadix<T>{}
impl<T> Length for Type2And3SplitRadix<T> {
    fn len(&self) -> usize {
        self.twiddles.len() * 4
    }
}




#[cfg(test)]
mod test {
    use super::*;
    use algorithm::Type2And3Naive;

    use test_utils::{compare_float_vectors, random_signal};

    /// Verify that our fast implementation of the DCT2 gives the same output as the slow version, for many different inputs
    #[test]
    fn test_dct2_splitradix() {

        for i in 2..8 {
            let size = 1 << i;
            println!("len: {}", size);

            let mut expected_input = random_signal(size);
            let mut actual_input = expected_input.clone();

            let mut expected_output = vec![0f32; size];
            let mut actual_output = vec![0f32; size];

            let naive_dct = Type2And3Naive::new(size);
            naive_dct.process_dct2(&mut expected_input, &mut expected_output);

            let quarter_dct = Arc::new(Type2And3Naive::new(size/4));
            let half_dct = Arc::new(Type2And3Naive::new(size/2));

            let dct = Type2And3SplitRadix::new(half_dct, quarter_dct);
            dct.process_dct2(&mut actual_input, &mut actual_output);

            println!("input:       {:?}", expected_input);
            println!("expected:    {:?}", expected_output);
            println!("fast output: {:?}", actual_output);

            assert!(
                compare_float_vectors(&actual_output, &expected_output),
                "len = {}",
                size
            );
        }
    }
}
