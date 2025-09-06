
use std::f64;
use rustfft::num_complex::Complex;
use common;

#[inline(always)]
pub fn single_twiddle<T: common::DCTnum>(i: usize, fft_len: usize) -> Complex<T> {
    let angle_constant = f64::consts::PI * -2f64 / fft_len as f64;

    let c = Complex::from_polar(&1f64, &(angle_constant * i as f64));

    Complex {
        re: T::from_f64(c.re).unwrap(),
        im: T::from_f64(c.im).unwrap(),
    }
}

// Same as above, but only return the real portion, not the imaginary portion
#[inline(always)]
pub fn single_twiddle_re<T: common::DCTnum>(i: usize, fft_len: usize) -> T {
    let angle_constant = f64::consts::PI * -2f64 / fft_len as f64;

    let c = (angle_constant * i as f64).cos();

    T::from_f64(c).unwrap()
}

// Same as above, but we add 0.5 to 0 before 
#[inline(always)]
pub fn single_twiddle_halfoffset<T: common::DCTnum>(i: usize, fft_len: usize) -> Complex<T> {
    let angle_constant = f64::consts::PI * -2f64 / fft_len as f64;
    
    let c = Complex::from_polar(&1f64, &(angle_constant * (i as f64 + 0.5f64)));

    Complex {
        re: T::from_f64(c.re).unwrap(),
        im: T::from_f64(c.im).unwrap(),
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use std::f32;

    use test_utils::fuzzy_cmp;

    #[test]
    fn test_single() {
        let len = 20;

        for i in 0..len {
            let single: Complex<f32> = single_twiddle(i, len);
            let single_inverse: Complex<f32> = single_twiddle(i, len).conj();

            let expected =
                Complex::from_polar(&1f32, &(-2f32 * f32::consts::PI * i as f32 / len as f32));
            let expected_inverse =
                Complex::from_polar(&1f32, &(2f32 * f32::consts::PI * i as f32 / len as f32));

            assert!(
                fuzzy_cmp(single.re, expected.re, 0.001f32),
                "forwards, i = {}",
                i
            );
            assert!(
                fuzzy_cmp(single.im, expected.im, 0.001f32),
                "forwards, i = {}",
                i
            );
            assert!(
                fuzzy_cmp(single_inverse.re, expected_inverse.re, 0.001f32),
                "inverse, i = {}",
                i
            );
            assert!(
                fuzzy_cmp(single_inverse.im, expected_inverse.im, 0.001f32),
                "inverse, i = {}",
                i
            );
        }
    }
}
