use num_complex::Complex;
use num_traits::Zero;

use common::{FFTnum, verify_length, verify_length_divisible};

use ::{Length, IsInverse, FFT};
use twiddles;

/// Naive O(n^2 ) Discrete Fourier Transform implementation
///
/// This implementation is primarily used to test other FFT algorithms. In a few rare cases, such as small
/// [Cunningham Chain](https://en.wikipedia.org/wiki/Cunningham_chain) primes, this can be faster than the O(nlogn)
/// algorithms
///
/// ~~~
/// // Computes a naive DFT of size 1234
/// use rustfft::algorithm::DFT;
/// use rustfft::FFT;
/// use rustfft::num_complex::Complex;
/// use rustfft::num_traits::Zero;
///
/// let mut input:  Vec<Complex<f32>> = vec![Zero::zero(); 1234];
/// let mut output: Vec<Complex<f32>> = vec![Zero::zero(); 1234];
///
/// let dft = DFT::new(1234, false);
/// dft.process(&mut input, &mut output);
/// ~~~
pub struct DFT<T> {
    twiddles: Vec<Complex<T>>,
    inverse: bool,
}

impl<T: FFTnum> DFT<T> {
    /// Preallocates necessary arrays and precomputes necessary data to efficiently compute DFT
    pub fn new(len: usize, inverse: bool) -> Self {
        DFT {
            twiddles: twiddles::generate_twiddle_factors(len, inverse),
            inverse: inverse
        }
    }

    #[inline(always)]
    fn perform_fft(&self, signal: &[Complex<T>], spectrum: &mut [Complex<T>]) {
        for k in 0..spectrum.len() {
            let output_cell = spectrum.get_mut(k).unwrap();

            *output_cell = Zero::zero();
            let mut twiddle_index = 0;

            for input_cell in signal {
                let twiddle = self.twiddles[twiddle_index];
                *output_cell = *output_cell + twiddle * input_cell;

                twiddle_index += k;
                if twiddle_index >= self.twiddles.len() {
                    twiddle_index -= self.twiddles.len();
                }
            }
        }
    }
}

impl<T: FFTnum> FFT<T> for DFT<T> {
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
impl<T> Length for DFT<T> {
    #[inline(always)]
    fn len(&self) -> usize {
        self.twiddles.len()
    }
}
impl<T> IsInverse for DFT<T> {
    #[inline(always)]
    fn is_inverse(&self) -> bool {
        self.inverse
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use std::f32;
    use test_utils::{random_signal, compare_vectors};
    use num_complex::Complex;
    use num_traits::Zero;

    fn dft(signal: &[Complex<f32>], spectrum: &mut [Complex<f32>]) {
        for (k, spec_bin) in spectrum.iter_mut().enumerate() {
            let mut sum = Zero::zero();
            for (i, &x) in signal.iter().enumerate() {
                let angle = -1f32 * (i * k) as f32 * 2f32 * f32::consts::PI / signal.len() as f32;
                let twiddle = Complex::from_polar(&1f32, &angle);

                sum = sum + twiddle * x;
            }
            *spec_bin = sum;
        }
    }

    #[test]
    fn test_matches_dft() {
        let n = 4;

        for len in 1..20 {
            let dft_instance = DFT::new(len, false);
            assert_eq!(dft_instance.len(), len, "DFT instance reported incorrect length");

            let mut expected_input = random_signal(len * n);
            let mut actual_input = expected_input.clone();
            let mut multi_input = expected_input.clone();

            let mut expected_output = vec![Zero::zero(); len * n];
            let mut actual_output = expected_output.clone();
            let mut multi_output = expected_output.clone();

            // perform the test
            dft_instance.process_multi(&mut multi_input, &mut multi_output);

            for (input_chunk, output_chunk) in actual_input.chunks_mut(len).zip(actual_output.chunks_mut(len)) {
                dft_instance.process(input_chunk, output_chunk);
            }

            for (input_chunk, output_chunk) in expected_input.chunks_mut(len).zip(expected_output.chunks_mut(len)) {
                dft(input_chunk, output_chunk);
            }

            assert!(compare_vectors(&expected_output, &actual_output), "process() failed, length = {}", len);
            assert!(compare_vectors(&expected_output, &multi_output), "process_multi() failed, length = {}", len);
        }

        //verify that it doesn't crash if we have a length of 0
        let zero_dft = DFT::new(0, false);
        let mut zero_input: Vec<Complex<f32>> = Vec::new();
        let mut zero_output: Vec<Complex<f32>> = Vec::new();

        zero_dft.process(&mut zero_input, &mut zero_output);
    }

    /// Returns true if our `dft` function calculates the given spectrum from the
    /// given signal, and if rustfft's DFT struct does the same
    fn test_dft_correct(signal: &[Complex<f32>], spectrum: &[Complex<f32>]) -> bool {
        assert_eq!(signal.len(), spectrum.len());

        let expected_signal = signal.to_vec();
        let mut expected_spectrum = vec![Zero::zero(); spectrum.len()];

        let mut actual_signal = signal.to_vec();
        let mut actual_spectrum = vec![Zero::zero(); spectrum.len()];

        dft(&expected_signal, &mut expected_spectrum);

        let dft_instance = DFT::new(signal.len(), false);
        dft_instance.process(&mut actual_signal, &mut actual_spectrum);

        return compare_vectors(spectrum, &expected_spectrum) && compare_vectors(spectrum, &actual_spectrum);
    }

    #[test]
    fn test_dft_known_len_2() {
        let signal = [Complex{re: 1f32, im: 0f32},
                      Complex{re:-1f32, im: 0f32}];
        let spectrum = [Complex{re: 0f32, im: 0f32},
                        Complex{re: 2f32, im: 0f32}];
        assert!(test_dft_correct(&signal[..], &spectrum[..]));
    }

    #[test]
    fn test_dft_known_len_3() {
        let signal = [Complex{re: 1f32, im: 1f32},
                      Complex{re: 2f32, im:-3f32},
                          Complex{re:-1f32, im: 4f32}];
        let spectrum = [Complex{re: 2f32, im: 2f32},
                        Complex{re: -5.562177f32, im: -2.098076f32},
                        Complex{re: 6.562178f32, im: 3.09807f32}];
        assert!(test_dft_correct(&signal[..], &spectrum[..]));
    }

    #[test]
    fn test_dft_known_len_4() {
        let signal = [Complex{re: 0f32, im: 1f32},
                      Complex{re: 2.5f32, im:-3f32},
                      Complex{re:-1f32, im: -1f32},
                      Complex{re: 4f32, im: 0f32}];
        let spectrum = [Complex{re: 5.5f32, im: -3f32},
                        Complex{re: -2f32, im: 3.5f32},
                        Complex{re: -7.5f32, im: 3f32},
                        Complex{re: 4f32, im: 0.5f32}];
        assert!(test_dft_correct(&signal[..], &spectrum[..]));
    }

    #[test]
    fn test_dft_known_len_6() {
        let signal = [Complex{re: 1f32, im: 1f32},
                      Complex{re: 2f32, im: 2f32},
                      Complex{re: 3f32, im: 3f32},
                      Complex{re: 4f32, im: 4f32},
                      Complex{re: 5f32, im: 5f32},
                      Complex{re: 6f32, im: 6f32}];
        let spectrum = [Complex{re: 21f32, im: 21f32},
                        Complex{re: -8.16f32, im: 2.16f32},
                        Complex{re: -4.76f32, im: -1.24f32},
                        Complex{re: -3f32, im: -3f32},
                        Complex{re: -1.24f32, im: -4.76f32},
                        Complex{re: 2.16f32, im: -8.16f32}];
        assert!(test_dft_correct(&signal[..], &spectrum[..]));
    }
}