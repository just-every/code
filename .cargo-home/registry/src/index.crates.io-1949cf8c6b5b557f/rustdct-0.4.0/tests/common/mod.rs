use rustdct::num_traits::{Float, FromPrimitive};
use rand::{StdRng, SeedableRng};
use rand::distributions::{Normal, IndependentSample};

pub mod known_data;
pub mod slow_fns;

#[macro_use]
pub mod macros;

pub fn fuzzy_cmp<T: Float>(a: T, b: T, tolerance: T) -> bool {
    a >= b - tolerance && a <= b + tolerance
}

pub fn compare_float_vectors<T: Float + FromPrimitive>(expected: &[T], observed: &[T]) -> bool {
    assert_eq!(expected.len(), observed.len());

    let tolerance = T::from_f64(0.001).unwrap();

    for i in 0..expected.len() {
        if !fuzzy_cmp(observed[i], expected[i], tolerance) {
            return false;
        }
    }
    true
}

pub fn random_signal<T: Float + FromPrimitive>(length: usize) -> Vec<T> {
    let mut sig = Vec::with_capacity(length);
    let normal_dist = Normal::new(0.0, 10.0);

    let seed: [usize; 5] = [1910, 11431, 4984, 14828, length];
    let mut rng: StdRng = SeedableRng::from_seed(&seed[..]);

    for _ in 0..length {
        sig.push(T::from_f64(normal_dist.ind_sample(&mut rng)).unwrap());
    }
    return sig;
}