//! To test the accuracy of our FFT algorithm, we first test that our
//! naive DFT function is correct by comparing its output against several
//! known signal/spectrum relationships. Then, we generate random signals
//! for a variety of lengths, and test that our FFT algorithm matches our
//! DFT calculation for those signals.


extern crate rustfft;
extern crate rand;

use std::f32;

use rustfft::num_complex::Complex;
use rustfft::num_traits::Zero;

use rand::{StdRng, SeedableRng};
use rand::distributions::{Normal, Distribution};
use rustfft::{FFT, FFTplanner};
use rustfft::algorithm::DFT;

/// The seed for the random number generator used to generate
/// random signals. It's defined here so that we have deterministic
/// tests
const RNG_SEED: [u8; 32] = [1, 9, 1, 0, 1, 1, 4, 3, 1, 4, 9, 8,
    4, 1, 4, 8, 2, 8, 1, 2, 2, 2, 6, 1, 2, 3, 4, 5, 6, 7, 8, 9];

/// Returns true if the mean difference in the elements of the two vectors
/// is small
fn compare_vectors(vec1: &[Complex<f32>], vec2: &[Complex<f32>]) -> bool {
    assert_eq!(vec1.len(), vec2.len());
    let mut sse = 0f32;
    for (&a, &b) in vec1.iter().zip(vec2.iter()) {
        sse = sse + (a - b).norm();
    }
    return (sse / vec1.len() as f32) < 0.1f32;
}


fn fft_matches_dft(signal: Vec<Complex<f32>>, inverse: bool) -> bool {
    let mut signal_dft = signal.clone();
    let mut signal_fft = signal.clone();

    let mut spectrum_dft = vec![Zero::zero(); signal.len()];
    let mut spectrum_fft = vec![Zero::zero(); signal.len()];

    let mut planner = FFTplanner::new(inverse);
    let fft = planner.plan_fft(signal.len());
    assert_eq!(fft.len(), signal.len(), "FFTplanner created FFT of wrong length");
    assert_eq!(fft.is_inverse(), inverse, "FFTplanner created FFT of wrong direction");

    fft.process(&mut signal_fft, &mut spectrum_fft);

    let dft = DFT::new(signal.len(), inverse);
    dft.process(&mut signal_dft, &mut spectrum_dft);

    return compare_vectors(&spectrum_dft[..], &spectrum_fft[..]);
}

fn random_signal(length: usize) -> Vec<Complex<f32>> {
    let mut sig = Vec::with_capacity(length);
    let normal_dist = Normal::new(0.0, 10.0);
    let mut rng: StdRng = SeedableRng::from_seed(RNG_SEED);
    for _ in 0..length {
        sig.push(Complex{re: (normal_dist.sample(&mut rng) as f32),
                         im: (normal_dist.sample(&mut rng) as f32)});
    }
    return sig;
}

/// Integration tests that verify our FFT output matches the direct DFT calculation
/// for random signals.
#[test]
fn test_fft() {
    for len in 1..100 {
        let signal = random_signal(len);
        assert!(fft_matches_dft(signal, false), "length = {}", len);
    }

    //test some specific lengths > 100
    for &len in &[256, 768] {
        let signal = random_signal(len);
        assert!(fft_matches_dft(signal, false), "length = {}", len);
    }
}

#[test]
fn test_fft_inverse() {
    for len in 1..100 {
        let signal = random_signal(len);
        assert!(fft_matches_dft(signal, true), "length = {}", len);
    }

    //test some specific lengths > 100
    for &len in &[256, 768] {
        let signal = random_signal(len);
        assert!(fft_matches_dft(signal, true), "length = {}", len);
    }
}
