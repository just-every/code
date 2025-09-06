use std::f64;

use rustfft::Length;

use mdct::MDCT;
use common;

/// Naive O(n^2 ) MDCT implementation
///
/// This implementation is primarily used to test other MDCT algorithms.
///
/// ~~~
/// // Computes a naive MDCT of output size 124, using the MP3 window function
/// use rustdct::mdct::{MDCT, MDCTNaive, window_fn};
///
/// let len = 124;
/// let mut input:  Vec<f32> = vec![0f32; len * 2];
/// let mut output: Vec<f32> = vec![0f32; len];
///
/// let dct = MDCTNaive::new(len, window_fn::mp3);
/// dct.process_mdct(&input, &mut output);
/// ~~~
pub struct MDCTNaive<T> {
    twiddles: Box<[T]>,
    window: Box<[T]>,
}

impl<T: common::DCTnum> MDCTNaive<T> {
    /// Creates a new MDCT context that will process inputs of length `output_len * 2` and produce
    /// outputs of length `output_len`
    ///
    /// `output_len` must be even.
    ///
    /// `window_fn` is a function that takes a `size` and returns a `Vec` containing `size` window values.
    /// See the [`window_fn`](mdct/window_fn/index.html) module for provided window functions.
    pub fn new<F>(output_len: usize, window_fn: F) -> Self
    where
        F: FnOnce(usize) -> Vec<T>,
    {
        assert!(output_len % 2 == 0, "The MDCT len must be even. Got {}", output_len);

        let constant_factor = 0.5f64 * f64::consts::PI / (output_len as f64);
        let twiddles: Vec<T> = (0..output_len * 4)
            .map(|i| (constant_factor * (i as f64 + 0.5_f64)).cos())
            .map(|c| T::from_f64(c).unwrap())
            .collect();

        let window = window_fn(output_len * 2);
        assert_eq!(window.len(), output_len * 2, "Window function returned incorrect number of values");

        Self {
            twiddles: twiddles.into_boxed_slice(),
            window: window.into_boxed_slice(),
        }
    }
}

impl<T: common::DCTnum> MDCT<T> for MDCTNaive<T> {
    fn process_mdct_split(&self, input_a: &[T], input_b: &[T], output: &mut [T]) {
        common::verify_length(input_a, output, self.len());
        assert_eq!(input_a.len(), input_b.len());

        let output_len = output.len();
        let half_output = output.len() / 2;

        for k in 0..output_len {
            let output_cell = output.get_mut(k).unwrap();
            *output_cell = T::zero();

            let mut twiddle_index = (half_output + k * (output_len + 1)) % self.twiddles.len();
            let twiddle_stride = k * 2 + 1;

            for i in 0..input_a.len() {
                let twiddle = self.twiddles[twiddle_index];

                *output_cell = *output_cell + input_a[i] * self.window[i] * twiddle;

                twiddle_index += twiddle_stride;
                if twiddle_index >= self.twiddles.len() {
                    twiddle_index -= self.twiddles.len();
                }
            }

            for i in 0..input_b.len() {
                let twiddle = self.twiddles[twiddle_index];

                *output_cell = *output_cell + input_b[i] * self.window[i + output_len] * twiddle;

                twiddle_index += twiddle_stride;
                if twiddle_index >= self.twiddles.len() {
                    twiddle_index -= self.twiddles.len();
                }
            }
        }
    }
    fn process_imdct_split(&self, input: &[T], output_a: &mut [T], output_b: &mut [T]) {
        common::verify_length(input, output_a, self.len());
        assert_eq!(output_a.len(), output_b.len());

        let input_len = input.len();
        let half_input = input_len / 2;

        for k in 0..input_len {
            let mut output_val = T::zero();

            let mut twiddle_index = half_input + k;
            let twiddle_stride = input_len + k * 2 + 1;

            for i in 0..input.len() {
                let twiddle = self.twiddles[twiddle_index];

                output_val = output_val + input[i] * twiddle;

                twiddle_index += twiddle_stride;
                if twiddle_index >= self.twiddles.len() {
                    twiddle_index -= self.twiddles.len();
                }
            }
            output_a[k] = output_a[k] + output_val * self.window[k];
        }

        for k in 0..input_len {
            let mut output_val = T::zero();

            let twiddle_k = if k < input_len / 2 {
                k
            } else {
                input_len - k - 1
            };

            let mut twiddle_index = self.twiddles.len() - input_len * 2 + half_input - twiddle_k -
                1;
            let twiddle_stride = input_len - 1 - twiddle_k * 2;

            for i in 0..input.len() {
                let twiddle = self.twiddles[twiddle_index];

                output_val = output_val + input[i] * twiddle;

                twiddle_index += twiddle_stride;
                if twiddle_index >= self.twiddles.len() {
                    twiddle_index -= self.twiddles.len();
                }
            }
            output_b[k] = output_b[k] + output_val * self.window[k + input_len];
        }
    }
}
impl<T> Length for MDCTNaive<T> {
    fn len(&self) -> usize {
        self.twiddles.len() / 4
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use std::f32;
    use mdct::window_fn;

    use test_utils::{compare_float_vectors, random_signal};

    /// Verify our naive implementation against some known values
    #[test]
    fn test_known_values_mdct() {
        let input_list = vec![
            vec![0_f32, 0_f32, 0_f32, 0_f32],
            vec![1_f32, 1_f32, -5_f32, 5_f32],
            vec![7_f32, 3_f32, 8_f32, 4_f32, -1_f32, 3_f32, 0_f32, 4_f32],
            vec![
                7_f32,
                3_f32,
                8_f32,
                4_f32,
                -1_f32,
                3_f32,
                0_f32,
                4_f32,
                1f32,
                1f32,
                1f32,
                1f32,
            ],
        ];
        let expected_list = vec![
            vec![0_f32, 0_f32],
            vec![0_f32, 0_f32],
            vec![-4.7455063, -2.073643, -2.2964284, 8.479767],
            vec![
                -2.90775651,
                -12.30026278,
                6.92661442,
                2.79403335,
                3.56420194,
                -2.40007133,
            ],
        ];

        for (input, expected) in input_list.iter().zip(expected_list.iter()) {
            let output = slow_mdct(&input, window_fn::one);

            assert!(compare_float_vectors(&expected, &output));
        }
    }

    /// Verify our naive windowed implementation against some known values
    #[test]
    fn test_known_values_windowed_mdct() {
        let input_list = vec![
            vec![0_f32, 0_f32, 0_f32, 0_f32],
            vec![1_f32, 1_f32, -5_f32, 5_f32],
            vec![7_f32, 3_f32, 8_f32, 4_f32, -1_f32, 3_f32, 0_f32, 4_f32],
            vec![
                7_f32,
                3_f32,
                8_f32,
                4_f32,
                -1_f32,
                3_f32,
                0_f32,
                4_f32,
                1f32,
                1f32,
                1f32,
                1f32,
            ],
        ];
        let expected_list = vec![
            vec![0_f32, 0_f32],
            vec![2.29289322, 1.53553391],
            vec![-4.67324308, 3.1647844, -6.22625186, 2.1647844],
            vec![
                -5.50153067,
                -3.46580575,
                3.79375195,
                -1.25072987,
                4.6738204,
                3.16506351,
            ],
        ];

        for (input, expected) in input_list.iter().zip(expected_list.iter()) {

            let output = slow_mdct(&input, window_fn::mp3);

            assert!(compare_float_vectors(&expected, &output));
        }
    }

    /// Verify that our fast implementation of the MDCT gives the same output as the slow version, for many different inputs
    #[test]
    fn test_matches_mdct() {
        for current_window_fn in &[window_fn::one, window_fn::mp3, window_fn::vorbis] {
            for i in 1..10 {
                let input_len = i * 4;
                let output_len = i * 2;

                let mut input = random_signal(input_len);
                let slow_output = slow_mdct(&input, current_window_fn);


                let mut fast_output = vec![0f32; output_len];

                let mut dct = MDCTNaive::new(output_len, current_window_fn);

                dct.process_mdct(&mut input, &mut fast_output);

                println!("{}", output_len);
                println!("expected: {:?}", slow_output);
                println!("actual: {:?}", fast_output);

                assert!(
                    compare_float_vectors(&slow_output, &fast_output),
                    "i = {}",
                    i
                );
            }
        }
    }


    fn slow_mdct<F>(input: &[f32], window_fn: F) -> Vec<f32>
    where
        F: Fn(usize) -> Vec<f32>,
    {
        let mut output = vec![0f32; input.len() / 2];

        let size_float = output.len() as f32;

        let window = window_fn(input.len());
        let windowed_input: Vec<f32> = input.iter().zip(window).map(|(i, w)| i * w).collect();

        for k in 0..output.len() {
            let mut current_value = 0_f32;

            let k_float = k as f32;

            for n in 0..input.len() {
                let n_float = n as f32;

                let twiddle = (f32::consts::PI * (n_float + 0.5_f32 + size_float * 0.5) *
                                   (k_float + 0.5_f32) / size_float)
                    .cos();

                current_value += windowed_input[n] * twiddle;
            }
            output[k] = current_value;
        }
        output
    }

    /// Verify our naive implementation against some known values
    #[test]
    fn test_known_values_imdct() {
        let input_list = vec![
            vec![0f32, 0f32],
            vec![1f32, 5f32],
            vec![7f32, 3f32, 8f32, 4f32],
            vec![7f32, 3f32, 8f32, 4f32, -1f32, 3f32],
        ];
        let expected_list = vec![
            vec![0f32, 0f32, 0f32, 0f32],
            vec![-4.2367144, 4.2367153, -2.837299, -2.8372989],
            vec![
                5.833236,
                2.4275358,
                -2.4275393,
                -5.833232,
                4.8335495,
                -14.584825,
                -14.584811,
                4.8335423,
            ],
            vec![
                2.4138875,
                8.921771,
                -2.4359043,
                2.4359055,
                -8.921769,
                -2.4138737,
                3.1458342,
                -0.63405657,
                -18.502512,
                -18.502502,
                -0.6340414,
                3.1458292,
            ],
        ];

        for (input, expected) in input_list.iter().zip(expected_list.iter()) {
            let slow_output = slow_imdct(&input, window_fn::one);

            let mut fast_output = vec![0f32; input.len() * 2];

            let mut dct = MDCTNaive::new(input.len(), window_fn::one);
            dct.process_imdct(&input, &mut fast_output);

            assert!(compare_float_vectors(&expected, &slow_output));
            assert!(compare_float_vectors(&expected, &fast_output));
        }
    }

    /// Verify our naive windowed implementation against some known values
    #[test]
    fn test_known_values_windowed_imdct() {
        let input_list = vec![
            vec![0f32, 0f32],
            vec![1_f32, 5_f32],
            vec![7_f32, 3_f32, 8_f32, 4_f32],
            vec![7_f32, 3_f32, 8_f32, 4_f32, -1_f32, 3_f32],
        ];
        let expected_list = vec![
            vec![0f32, 0f32, 0f32, 0f32],
            vec![
                -1.6213203435596431,
                3.9142135623730936,
                -2.6213203435596433,
                -1.0857864376269069,
            ],
            vec![
                1.1380080486867217,
                1.3486674811260955,
                -2.0184235241728627,
                -5.7211528055198331,
                4.7406716077536428,
                -12.126842074178105,
                -8.1028968193867765,
                0.94297821246780911,
            ],
            vec![
                0.3150751815802082,
                3.4142135623730949,
                -1.4828837895525038,
                1.9325317795197492,
                -8.2426406871192732,
                -2.3932336063055089,
                3.1189227588735786,
                -0.58578643762689731,
                -14.679036212259122,
                -11.263620643186901,
                -0.24264068711929426,
                0.41061397098787894,
            ],
        ];

        for (input, expected) in input_list.iter().zip(expected_list.iter()) {

            let slow_output = slow_imdct(&input, window_fn::mp3);

            let mut fast_output = vec![0f32; input.len() * 2];

            let mut dct = MDCTNaive::new(input.len(), window_fn::mp3);
            dct.process_imdct(&input, &mut fast_output);

            assert!(compare_float_vectors(&expected, &slow_output));
            assert!(compare_float_vectors(&expected, &fast_output));
        }
    }

    /// Verify that our fast implementation of the MDCT gives the same output as the slow version, for many different inputs
    #[test]
    fn test_matches_imdct() {
        for current_window_fn in &[window_fn::one, window_fn::mp3, window_fn::vorbis] {
            for i in 1..10 {
                let input_len = i * 2;
                let output_len = i * 4;

                let mut input = random_signal(input_len);
                let slow_output = slow_imdct(&input, current_window_fn);

                let mut fast_output = vec![0f32; output_len];

                let mut dct = MDCTNaive::new(input_len, current_window_fn);
                dct.process_imdct(&mut input, &mut fast_output);

                assert!(
                    compare_float_vectors(&slow_output, &fast_output),
                    "i = {}",
                    i
                );
            }
        }
    }


    fn slow_imdct<F>(input: &[f32], window_fn: F) -> Vec<f32>
    where
        F: Fn(usize) -> Vec<f32>,
    {
        let mut output = vec![0f32; input.len() * 2];

        let size_float = input.len() as f32;

        for n in 0..output.len() {
            let mut current_value = 0_f32;

            let n_float = n as f32;

            for k in 0..input.len() {
                let k_float = k as f32;

                let twiddle = (f32::consts::PI * (n_float + 0.5_f32 + size_float * 0.5) *
                                   (k_float + 0.5_f32) / size_float)
                    .cos();

                current_value += input[k] * twiddle;
            }
            output[n] = current_value;
        }

        let window = window_fn(output.len());
        output.iter().zip(window).map(|(e, w)| e * w).collect()
    }
}
