use std::collections::HashMap;
use std::sync::Arc;
use num_integer::gcd;

use common::FFTnum;

use FFT;
use algorithm::*;
use algorithm::butterflies::*;

use math_utils;


const MIN_RADIX4_BITS: u32 = 5; // smallest size to consider radix 4 an option is 2^5 = 32
const MAX_RADIX4_BITS: u32 = 16; // largest size to consider radix 4 an option is 2^16 = 65536
const BUTTERFLIES: [usize; 9] = [2, 3, 4, 5, 6, 7, 8, 16, 32];
const COMPOSITE_BUTTERFLIES: [usize; 5] = [4, 6, 8, 16, 32];

/// The FFT planner is used to make new FFT algorithm instances.
///
/// RustFFT has several FFT algorithms available; For a given FFT size, the FFTplanner decides which of the
/// available FFT algorithms to use and then initializes them.
///
/// ~~~
/// // Perform a forward FFT of size 1234
/// use std::sync::Arc;
/// use rustfft::FFTplanner;
/// use rustfft::num_complex::Complex;
/// use rustfft::num_traits::Zero;
///
/// let mut input:  Vec<Complex<f32>> = vec![Zero::zero(); 1234];
/// let mut output: Vec<Complex<f32>> = vec![Zero::zero(); 1234];
///
/// let mut planner = FFTplanner::new(false);
/// let fft = planner.plan_fft(1234);
/// fft.process(&mut input, &mut output);
/// 
/// // The fft instance returned by the planner is stored behind an `Arc`, so it's cheap to clone
/// let fft_clone = Arc::clone(&fft);
/// ~~~
///
/// If you plan on creating multiple FFT instances, it is recommnded to reuse the same planner for all of them. This
/// is because the planner re-uses internal data across FFT instances wherever possible, saving memory and reducing
/// setup time. (FFT instances created with one planner will never re-use data and buffers with FFT instances created
/// by a different planner)
///
/// Each FFT instance owns `Arc`s to its internal data, rather than borrowing it from the planner, so it's perfectly
/// safe to drop the planner after creating FFT instances.
pub struct FFTplanner<T> {
    inverse: bool,
    algorithm_cache: HashMap<usize, Arc<FFT<T>>>,
    butterfly_cache: HashMap<usize, Arc<FFTButterfly<T>>>,
}

impl<T: FFTnum> FFTplanner<T> {
    /// Creates a new FFT planner.
    ///
    /// If `inverse` is false, this planner will plan forward FFTs. If `inverse` is true, it will plan inverse FFTs.
    pub fn new(inverse: bool) -> Self {
        FFTplanner {
            inverse: inverse,
            algorithm_cache: HashMap::new(),
            butterfly_cache: HashMap::new(),
        }
    }

    /// Returns a FFT instance which processes signals of size `len`
    /// If this is called multiple times, it will attempt to re-use internal data between instances
    pub fn plan_fft(&mut self, len: usize) -> Arc<FFT<T>> {
        if len < 2 {
            Arc::new(DFT::new(len, self.inverse)) as Arc<FFT<T>>
        } else {
            let factors = math_utils::prime_factors(len);
            self.plan_fft_with_factors(len, &factors)
        }
    }

    fn plan_butterfly(&mut self, len: usize) -> Arc<FFTButterfly<T>> {
        let inverse = self.inverse;
        let instance = self.butterfly_cache.entry(len).or_insert_with(|| 
            match len {
                2 => Arc::new(Butterfly2::new(inverse)),
                3 => Arc::new(Butterfly3::new(inverse)),
                4 => Arc::new(Butterfly4::new(inverse)),
                5 => Arc::new(Butterfly5::new(inverse)),
                6 => Arc::new(Butterfly6::new(inverse)),
                7 => Arc::new(Butterfly7::new(inverse)),
                8 => Arc::new(Butterfly8::new(inverse)),
                16 => Arc::new(Butterfly16::new(inverse)),
                32 => Arc::new(Butterfly32::new(inverse)),
                _ => panic!("Invalid butterfly size: {}", len),
            }
        );
        Arc::clone(instance)
    }
    
    fn plan_fft_with_factors(&mut self, len: usize, factors: &[usize]) -> Arc<FFT<T>> {
        if self.algorithm_cache.contains_key(&len) {
            Arc::clone(self.algorithm_cache.get(&len).unwrap())
        } else {
            let result = if factors.len() == 1 || COMPOSITE_BUTTERFLIES.contains(&len) {
                self.plan_fft_single_factor(len)

            } else if len.trailing_zeros() <= MAX_RADIX4_BITS && len.trailing_zeros() >= MIN_RADIX4_BITS {
                //the number of trailing zeroes in len is the number of `2` factors
                //ie if len = 2048 * n, len.trailing_zeros() will equal 11 because 2^11 == 2048

                if len.is_power_of_two() {
                    Arc::new(Radix4::new(len, self.inverse))
                } else {
                    let left_len = 1 << len.trailing_zeros();
                    let right_len = len / left_len;

                    let (left_factors, right_factors) = factors.split_at(len.trailing_zeros() as usize);

                    self.plan_mixed_radix(left_len, left_factors, right_len, right_factors)
                }

            } else {
                let sqrt = (len as f32).sqrt() as usize;
                if sqrt * sqrt == len {
                    // since len is a perfect square, each of its prime factors is duplicated.
                    // since we know they're sorted, we can loop through them in chunks of 2 and keep one out of each chunk
                    // if the stride iterator ever becomes stabilized, it'll be cleaner to use that instead of chunks
                    let mut sqrt_factors = Vec::with_capacity(factors.len() / 2);
                    for chunk in factors.chunks(2) {
                        sqrt_factors.push(chunk[0]);
                    }

                    self.plan_mixed_radix(sqrt, &sqrt_factors, sqrt, &sqrt_factors)
                } else {
                    //len isn't a perfect square. greedily take factors from the list until both sides are as close as possible to sqrt(len)
                    //TODO: We can probably make this more optimal by using a more sophisticated non-greedy algorithm
                    let mut product = 1;
                    let mut second_half_index = 1;
                    for (i, factor) in factors.iter().enumerate() {
                        if product * *factor > sqrt {
                            second_half_index = i;
                            break;
                        } else {
                            product = product * *factor;
                        }
                    }

                    //we now know that product is the largest it can be without being greater than len / product
                    //there's one more thing we can try to make them closer together -- if product * factors[index] < len / product,
                    if product * factors[second_half_index] < len / product {
                        product = product * factors[second_half_index];
                        second_half_index = second_half_index + 1;
                    }

                    //we now have our two FFT sizes: product and product / len
                    let (left_factors, right_factors) = factors.split_at(second_half_index);
                    self.plan_mixed_radix(product, left_factors, len / product, right_factors)
                }
            };
            self.algorithm_cache.insert(len, Arc::clone(&result));
            result
        }
    }

    fn plan_mixed_radix(&mut self,
                        left_len: usize,
                        left_factors: &[usize],
                        right_len: usize,
                        right_factors: &[usize])
                        -> Arc<FFT<T>> {

        let left_is_butterfly = BUTTERFLIES.contains(&left_len);
        let right_is_butterfly = BUTTERFLIES.contains(&right_len);

        //if both left_len and right_len are butterflies, use a mixed radix implementation specialized for butterfly sub-FFTs
        if left_is_butterfly && right_is_butterfly {
            let left_fft = self.plan_butterfly(left_len);
            let right_fft = self.plan_butterfly(right_len);

            // for butterflies, if gcd is 1, we always want to use good-thomas
            if gcd(left_len, right_len) == 1 {
                Arc::new(GoodThomasAlgorithmDoubleButterfly::new(left_fft, right_fft)) as Arc<FFT<T>>
            } else {
                Arc::new(MixedRadixDoubleButterfly::new(left_fft, right_fft)) as Arc<FFT<T>>
            }
        } else {
            //neither size is a butterfly, so go with the normal algorithm
            let left_fft = self.plan_fft_with_factors(left_len, left_factors);
            let right_fft = self.plan_fft_with_factors(right_len, right_factors);

            Arc::new(MixedRadix::new(left_fft, right_fft)) as Arc<FFT<T>>
        }
    }


    fn plan_fft_single_factor(&mut self, len: usize) -> Arc<FFT<T>> {
        match len {
            0|1 => Arc::new(DFT::new(len, self.inverse)) as Arc<FFT<T>>,
            2 => Arc::new(butterflies::Butterfly2::new(self.inverse)) as Arc<FFT<T>>,
            3 => Arc::new(butterflies::Butterfly3::new(self.inverse)) as Arc<FFT<T>>,
            4 => Arc::new(butterflies::Butterfly4::new(self.inverse)) as Arc<FFT<T>>,
            5 => Arc::new(butterflies::Butterfly5::new(self.inverse)) as Arc<FFT<T>>,
            6 => Arc::new(butterflies::Butterfly6::new(self.inverse)) as Arc<FFT<T>>,
            7 => Arc::new(butterflies::Butterfly7::new(self.inverse)) as Arc<FFT<T>>,
            8 => Arc::new(butterflies::Butterfly8::new(self.inverse)) as Arc<FFT<T>>,
            16 => Arc::new(butterflies::Butterfly16::new(self.inverse)) as Arc<FFT<T>>,
            32 => Arc::new(butterflies::Butterfly32::new(self.inverse)) as Arc<FFT<T>>,
            _ => self.plan_prime(len),
        }
    }

    fn plan_prime(&mut self, len: usize) -> Arc<FFT<T>> {
        let inner_fft_len = len - 1;
        let factors = math_utils::prime_factors(inner_fft_len);

        let inner_fft = self.plan_fft_with_factors(inner_fft_len, &factors);

        Arc::new(RadersAlgorithm::new(len, inner_fft)) as Arc<FFT<T>>
    }
}
