/*
 * // Copyright (c) Radzivon Bartoshyk 7/2025. All rights reserved.
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
use crate::double_double::DoubleDouble;
use crate::logs::log_dd::log_poly;
use crate::logs::log_dd_coeffs::LOG_NEG_DD;
use crate::pow_tables::POW_INVERSE;

#[inline(always)]
pub(crate) fn log1p_tiny(z: f64) -> DoubleDouble {
    // See ./notes/log1p_tiny.sollya for generation
    const Q_1: [(u64, u64); 7] = [
        (0xbc85555555555555, 0x3fd5555555555556),
        (0x0000000000000000, 0xbfd0000000000000),
        (0xbc6999999999999a, 0x3fc999999999999a),
        (0x3c75555555555555, 0xbfc5555555555556),
        (0x3c62492492492492, 0x3fc2492492492492),
        (0x0000000000000000, 0xbfc0000000000000),
        (0x3c5c71c71c71c71c, 0x3fbc71c71c71c71c),
    ];
    let mut r = DoubleDouble::quick_mul_f64_add_f64(
        DoubleDouble::from_bit_pair(Q_1[6]),
        z,
        f64::from_bits(0xbfc0000000000000),
    );
    r = DoubleDouble::quick_mul_f64_add(r, z, DoubleDouble::from_bit_pair(Q_1[4]));
    r = DoubleDouble::quick_mul_f64_add(r, z, DoubleDouble::from_bit_pair(Q_1[3]));
    r = DoubleDouble::quick_mul_f64_add(r, z, DoubleDouble::from_bit_pair(Q_1[2]));
    r = DoubleDouble::quick_mul_f64_add_f64(r, z, f64::from_bits(0xbfd0000000000000));
    r = DoubleDouble::quick_mul_f64_add(r, z, DoubleDouble::from_bit_pair(Q_1[0]));
    r = DoubleDouble::quick_mul_f64_add_f64(r, z, f64::from_bits(0xbfe0000000000000));
    r = DoubleDouble::quick_mul_f64_add_f64(r, z, f64::from_bits(0x3ff0000000000000));
    DoubleDouble::quick_mult_f64(r, z)
}

#[inline]
pub(crate) fn log1p_dd(z: f64) -> DoubleDouble {
    let ax = z.to_bits().wrapping_shl(1);
    if ax < 0x7e60000000000000u64 {
        // |x| < 0x1p-12
        return log1p_tiny(z);
    }
    let dz = DoubleDouble::from_full_exact_add(z, 1.0);

    // We'll compute log((z+1)+1) as log(xh+xl) = log(xh) + log(1+xl/xh).
    // since xl/xh < ulp(xh) we'll use for log(1+xl/xh)
    // one taylor term what means that log(1+xl/xh) = log_lo + O(x^2)

    let log_lo = if dz.hi <= f64::from_bits(0x7fd0000000000000) || dz.lo.abs() >= 4.0 {
        dz.lo / dz.hi
    } else {
        0.
    }; // avoid spurious underflow

    let x_u = dz.hi.to_bits();
    let mut m = x_u & 0xfffffffffffff;
    let mut e: i64 = ((x_u >> 52) & 0x7ff) as i64;

    let t;
    if e != 0 {
        t = m | (0x3ffu64 << 52);
        m = m.wrapping_add(1u64 << 52);
        e -= 0x3ff;
    } else {
        /* x is a subnormal double  */
        let k = m.leading_zeros() - 11;

        e = -0x3fei64 - k as i64;
        m = m.wrapping_shl(k);
        t = m | (0x3ffu64 << 52);
    }

    /* now |x| = 2^_e*_t = 2^(_e-52)*m with 1 <= _t < 2,
    and 2^52 <= _m < 2^53 */

    //   log(x) = log(t) + E · log(2)
    let mut t = f64::from_bits(t);

    // If m > sqrt(2) we divide it by 2 so ensure 1/sqrt(2) < t < sqrt(2)
    let c: usize = (m >= 0x16a09e667f3bcd) as usize;
    static CY: [f64; 2] = [1.0, 0.5];
    static CM: [u64; 2] = [44, 45];

    e = e.wrapping_add(c as i64);
    let be = e;
    let i = m >> CM[c];
    t *= CY[c];

    let r = f64::from_bits(POW_INVERSE[(i - 181) as usize]);
    let log_r = DoubleDouble::from_bit_pair(LOG_NEG_DD[(i - 181) as usize]);

    let z = f64::mul_add(r, t, -1.0);

    const LOG2_DD: DoubleDouble = DoubleDouble::new(
        f64::from_bits(0x3c7abc9e3b39803f),
        f64::from_bits(0x3fe62e42fefa39ef),
    );

    let tt = DoubleDouble::mul_f64_add(LOG2_DD, be as f64, log_r);

    let v = DoubleDouble::full_add_f64(tt, z);
    let mut p = log_poly(z);
    // adding log(1+xl/xh) lower term
    p.lo += log_lo;
    DoubleDouble::f64_add(v.hi, DoubleDouble::new(v.lo + p.lo, p.hi))
}
