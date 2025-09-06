/*
 * // Copyright (c) Radzivon Bartoshyk 4/2025. All rights reserved.
 * //
 * // Redistribution and use in source and binary forms, with or without modification,
 * // are permitted provided that the following conditions are met:
 * //
 * // 1.  Redistributions of source code must retain the above copyright notice, this
 * // list of conditions and the following disclaimer.
 * //
 * // 2.  Redistributions in binary form must reproduce the above copyright notice,
 * // this list of conditions and the following disclaimer in the documentation
 * // and/or other materials provided with the distribution.
 * //
 * // 3.  Neither the name of the copyright holder nor the names of its
 * // contributors may be used to endorse or promote products derived from
 * // this software without specific prior written permission.
 * //
 * // THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
 * // AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * // IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
 * // DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
 * // FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * // DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
 * // SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
 * // CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
 * // OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
 * // OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */
use crate::common::EXP_MASK_F32;

#[inline]
pub fn f_hypot3f(x: f32, y: f32, z: f32) -> f32 {
    let x = x.abs();
    let y = y.abs();
    let z = z.abs();

    let max = x.max(y).max(z);

    if max == 0.0 {
        return 0.0;
    }

    let recip_max = 1. / max;

    let norm_x = x * recip_max;
    let norm_y = y * recip_max;
    let norm_z = z * recip_max;

    max * (norm_x * norm_x + norm_y * norm_y + norm_z * norm_z).sqrt()

    // if x == f32::INFINITY || y == f32::INFINITY || z == f32::INFINITY {
    //     f32::INFINITY
    // } else if x.is_nan() || y.is_nan() || z.is_nan() || ret.is_nan() {
    //     f32::NAN
    // // } else {
    // ret
    // }
}

/// Hypot function
///
/// Max ULP 0.5
#[inline]
pub fn f_hypotf(x: f32, y: f32) -> f32 {
    let x_abs = x.abs();
    let y_abs = y.abs();

    let x_abs_larger = x_abs >= y_abs;

    let a_bits = (if x_abs_larger { x_abs } else { y_abs }).to_bits();
    let b_bits = (if x_abs_larger { y_abs } else { x_abs }).to_bits();

    let a_u = a_bits;
    let b_u = b_bits;

    if a_u >= EXP_MASK_F32 {
        // x or y is inf or nan
        if f32::from_bits(a_bits).is_nan() || f32::from_bits(b_bits).is_nan() {
            return f32::NAN;
        }
        if f32::from_bits(a_bits).is_infinite() || f32::from_bits(b_bits).is_infinite() {
            return f32::INFINITY;
        }
        return f32::from_bits(a_bits);
    }

    if a_u.wrapping_sub(b_u) >= ((23u32 + 2) << 23) {
        return x_abs + y_abs;
    }

    let ad = f32::from_bits(a_bits) as f64;
    let bd = f32::from_bits(b_bits) as f64;

    // These squares are exact.
    let a_sq: f64 = ad * ad;
    let sum_sq: f64;
    #[cfg(not(any(
        all(
            any(target_arch = "x86", target_arch = "x86_64"),
            target_feature = "fma"
        ),
        all(target_arch = "aarch64", target_feature = "neon")
    )))]
    let b_sq: f64;
    #[cfg(any(
        all(
            any(target_arch = "x86", target_arch = "x86_64"),
            target_feature = "fma"
        ),
        all(target_arch = "aarch64", target_feature = "neon")
    ))]
    {
        use crate::common::f_fmla;
        sum_sq = f_fmla(bd, bd, a_sq);
    }
    #[cfg(not(any(
        all(
            any(target_arch = "x86", target_arch = "x86_64"),
            target_feature = "fma"
        ),
        all(target_arch = "aarch64", target_feature = "neon")
    )))]
    {
        b_sq = bd * bd;
        sum_sq = a_sq + b_sq;
    }

    let mut r_u: u64 = sum_sq.sqrt().to_bits();

    // If any of the sticky bits of the result are non-zero, except the LSB, then
    // the rounded result is correct.
    if ((r_u + 1) & 0x0000_0000_0FFF_FFFE) == 0 {
        let r_d = f64::from_bits(r_u);

        let (sum_sq_lo, err);

        #[cfg(any(
            all(
                any(target_arch = "x86", target_arch = "x86_64"),
                target_feature = "fma"
            ),
            all(target_arch = "aarch64", target_feature = "neon")
        ))]
        {
            use crate::common::f_fmla;
            sum_sq_lo = f_fmla(bd, bd, a_sq - sum_sq);
            err = sum_sq_lo - f_fmla(r_d, r_d, -sum_sq);
        }
        #[cfg(not(any(
            all(
                any(target_arch = "x86", target_arch = "x86_64"),
                target_feature = "fma"
            ),
            all(target_arch = "aarch64", target_feature = "neon")
        )))]
        {
            use crate::double_double::DoubleDouble;
            let r_sq = DoubleDouble::from_exact_mult(r_d, r_d);
            sum_sq_lo = b_sq - (sum_sq - a_sq);
            err = (sum_sq - r_sq.hi) + (sum_sq_lo - r_sq.lo);
        }

        if err > 0. {
            r_u |= 1;
        } else if (err < 0.) && (r_u & 1) == 0 {
            r_u = r_u.wrapping_sub(r_u);
        }
        return f64::from_bits(r_u) as f32;
    }
    f64::from_bits(r_u) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hypotf() {
        assert_eq!(
            f_hypotf(
                0.000000000000000000000000000000000000000091771,
                0.000000000000000000000000000000000000011754585
            ),
            0.000000000000000000000000000000000000011754944
        );
        assert_eq!(
            f_hypotf(9.177e-41, 1.1754585e-38),
            0.000000000000000000000000000000000000011754944
        );
        let dx = (f_hypotf(1f32, 1f32) - (1f32 * 1f32 + 1f32 * 1f32).sqrt()).abs();
        assert!(dx < 1e-5);
        let dx = (f_hypotf(5f32, 5f32) - (5f32 * 5f32 + 5f32 * 5f32).sqrt()).abs();
        assert!(dx < 1e-5);
    }
}
