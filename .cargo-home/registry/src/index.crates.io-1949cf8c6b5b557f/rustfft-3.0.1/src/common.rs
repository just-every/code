use num_traits::{FromPrimitive, Signed};

/// Generic floating point number, implemented for f32 and f64
pub trait FFTnum: Copy + FromPrimitive + Signed + Sync + Send + 'static {}

impl FFTnum for f32 {}
impl FFTnum for f64 {}


#[inline(always)]
pub fn verify_length<T>(input: &[T], output: &[T], expected: usize) {
	assert_eq!(input.len(), expected, "Input is the wrong length. Expected {}, got {}", expected, input.len());
	assert_eq!(output.len(), expected, "Output is the wrong length. Expected {}, got {}", expected, output.len());
}


#[inline(always)]
pub fn verify_length_divisible<T>(input: &[T], output: &[T], expected: usize) {
	assert_eq!(input.len() % expected, 0, "Input is the wrong length. Expected multiple of {}, got {}", expected, input.len());
	assert_eq!(input.len(), output.len(), "Input and output must have the same length. Expected {}, got {}", input.len(), output.len());
}
