use num_complex::Complex;
use num_traits::Zero;

use common::{FFTnum, verify_length, verify_length_divisible};

use algorithm::butterflies::{Butterfly2, Butterfly4, Butterfly8, Butterfly16, FFTButterfly};
use ::{Length, IsInverse, FFT};
use twiddles;

/// FFT algorithm optimized for power-of-two sizes
///
/// ~~~
/// // Computes a forward FFT of size 4096
/// use rustfft::algorithm::Radix4;
/// use rustfft::FFT;
/// use rustfft::num_complex::Complex;
/// use rustfft::num_traits::Zero;
///
/// let mut input:  Vec<Complex<f32>> = vec![Zero::zero(); 4096];
/// let mut output: Vec<Complex<f32>> = vec![Zero::zero(); 4096];
///
/// let fft = Radix4::new(4096, false);
/// fft.process(&mut input, &mut output);
/// ~~~

pub struct Radix4<T> {
    twiddles: Box<[Complex<T>]>,
    butterfly8: Butterfly8<T>,
    butterfly16: Butterfly16<T>,
    len: usize,
    inverse: bool,
}

impl<T: FFTnum> Radix4<T> {
    /// Preallocates necessary arrays and precomputes necessary data to efficiently compute the power-of-two FFT
    pub fn new(len: usize, inverse: bool) -> Self {
        assert!(len.is_power_of_two(), "Radix4 algorithm requires a power-of-two input size. Got {}", len);

        // precompute the twiddle factors this algorithm will use.
        // we're doing the same precomputation of twiddle factors as the mixed radix algorithm where width=4 and height=len/4
        // but mixed radix only does one step and then calls itself recusrively, and this algorithm does every layer all the way down
        // so we're going to pack all the "layers" of twiddle factors into a single array, starting with the bottom and going up
        let num_bits = len.trailing_zeros();
        let mut twiddle_stride = if num_bits%2 == 0 {
            len / 64
        } else {
            len / 32
        };

        let mut twiddle_factors = Vec::with_capacity(len * 2);
        while twiddle_stride > 0 {
            let num_rows = len / (twiddle_stride * 4);
            for i in 0..num_rows {
                for k in 1..4 {
                    let twiddle = twiddles::single_twiddle(i * k * twiddle_stride, len, inverse);
                    twiddle_factors.push(twiddle);
                }
            }
            twiddle_stride >>= 2;
        }

        Radix4 {
            twiddles: twiddle_factors.into_boxed_slice(),
            butterfly8: Butterfly8::new(inverse),
            butterfly16: Butterfly16::new(inverse),
            len: len,
            inverse: inverse,
        }
    }

    fn perform_fft(&self, signal: &[Complex<T>], spectrum: &mut [Complex<T>]) {
        match self.len() {
            0|1 => spectrum.copy_from_slice(signal),
            2 => {
                spectrum.copy_from_slice(signal);
                unsafe { Butterfly2::new(self.inverse).process_inplace(spectrum) }
            },
            4 => {
                spectrum.copy_from_slice(signal);
                unsafe { Butterfly4::new(self.inverse).process_inplace(spectrum) }
            },
            _ => {
                // copy the data into the spectrum vector
                prepare_radix4(signal.len(), signal, spectrum, 1);

                // perform the butterflies. the butterfly size depends on the input size
                let num_bits = signal.len().trailing_zeros();
                let mut current_size = if num_bits % 2 == 0 {
                    unsafe { self.butterfly16.process_multi_inplace(spectrum) };

                    // for the cross-ffts we want to to start off with a size of 64 (16 * 4)
                    64
                } else {
                    unsafe { self.butterfly8.process_multi_inplace(spectrum) };

                    // for the cross-ffts we want to to start off with a size of 32 (8 * 4)
                    32
                };

                let mut layer_twiddles: &[Complex<T>] = &self.twiddles;

                // now, perform all the cross-FFTs, one "layer" at a time
                while current_size <= signal.len() {
                    let num_rows = signal.len() / current_size;

                    for i in 0..num_rows {
                        unsafe {
                            butterfly_4(&mut spectrum[i * current_size..],
                                        layer_twiddles,
                                        current_size / 4,
                                        self.inverse)
                        }
                    }

                    //skip past all the twiddle factors used in this layer
                    let twiddle_offset = (current_size * 3) / 4;
                    layer_twiddles = &layer_twiddles[twiddle_offset..];

                    current_size *= 4;
                }
            }
        }
    }
}

impl<T: FFTnum> FFT<T> for Radix4<T> {
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
impl<T> Length for Radix4<T> {
    #[inline(always)]
    fn len(&self) -> usize {
        self.len
    }
}
impl<T> IsInverse for Radix4<T> {
    #[inline(always)]
    fn is_inverse(&self) -> bool {
        self.inverse
    }
}



// after testing an iterative bit reversal algorithm, this recursive algorithm
// was almost an order of magnitude faster at setting up
fn prepare_radix4<T: FFTnum>(size: usize,
                           signal: &[Complex<T>],
                           spectrum: &mut [Complex<T>],
                           stride: usize) {
    match size {
        16 => unsafe {
            for i in 0..16 {
                *spectrum.get_unchecked_mut(i) = *signal.get_unchecked(i * stride);
            }
        },
        8 => unsafe {
            for i in 0..8 {
                *spectrum.get_unchecked_mut(i) = *signal.get_unchecked(i * stride);
            }
        },
        4 => unsafe {
            for i in 0..4 {
                *spectrum.get_unchecked_mut(i) = *signal.get_unchecked(i * stride);
            }
        },
        2 => unsafe {
            for i in 0..2 {
                *spectrum.get_unchecked_mut(i) = *signal.get_unchecked(i * stride);
            }
        },
        _ => {
            for i in 0..4 {
                prepare_radix4(size / 4,
                               &signal[i * stride..],
                               &mut spectrum[i * (size / 4)..],
                               stride * 4);
            }
        }
    }
}

unsafe fn butterfly_4<T: FFTnum>(data: &mut [Complex<T>],
                             twiddles: &[Complex<T>],
                             num_ffts: usize,
                             inverse: bool)
{
    let mut idx = 0usize;
    let mut tw_idx = 0usize;
    let mut scratch: [Complex<T>; 6] = [Zero::zero(); 6];
    for _ in 0..num_ffts {
        scratch[0] = data.get_unchecked(idx + 1 * num_ffts) * twiddles[tw_idx];
        scratch[1] = data.get_unchecked(idx + 2 * num_ffts) * twiddles[tw_idx + 1];
        scratch[2] = data.get_unchecked(idx + 3 * num_ffts) * twiddles[tw_idx + 2];
        scratch[5] = data.get_unchecked(idx) - scratch[1];
        *data.get_unchecked_mut(idx) = data.get_unchecked(idx) + scratch[1];
        scratch[3] = scratch[0] + scratch[2];
        scratch[4] = scratch[0] - scratch[2];
        *data.get_unchecked_mut(idx + 2 * num_ffts) = data.get_unchecked(idx) - scratch[3];
        *data.get_unchecked_mut(idx) = data.get_unchecked(idx) + scratch[3];
        if inverse {
            data.get_unchecked_mut(idx + num_ffts).re = scratch[5].re - scratch[4].im;
            data.get_unchecked_mut(idx + num_ffts).im = scratch[5].im + scratch[4].re;
            data.get_unchecked_mut(idx + 3 * num_ffts).re = scratch[5].re + scratch[4].im;
            data.get_unchecked_mut(idx + 3 * num_ffts).im = scratch[5].im - scratch[4].re;
        } else {
            data.get_unchecked_mut(idx + num_ffts).re = scratch[5].re + scratch[4].im;
            data.get_unchecked_mut(idx + num_ffts).im = scratch[5].im - scratch[4].re;
            data.get_unchecked_mut(idx + 3 * num_ffts).re = scratch[5].re - scratch[4].im;
            data.get_unchecked_mut(idx + 3 * num_ffts).im = scratch[5].im + scratch[4].re;
        }

        tw_idx += 3;
        idx += 1;
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use test_utils::check_fft_algorithm;

    #[test]
    fn test_radix4() {
        for pow in 0..8 {
            let len = 1 << pow;
            test_radix4_with_length(len, false);
            test_radix4_with_length(len, true);
        }
    }

    fn test_radix4_with_length(len: usize, inverse: bool) {
        let fft = Radix4::new(len, inverse);

        check_fft_algorithm(&fft, len, inverse);
    }
}
