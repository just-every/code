use std::fmt::Debug;
use rustfft::FFTnum;
use rustfft::num_traits::FloatConst;


/// Generic floating point number, implemented for f32 and f64
pub trait DCTnum: FFTnum + FloatConst + Debug {
	fn half() -> Self;
	fn two() -> Self;
}
impl DCTnum for f32 {
	fn half() -> Self {
		0.5
	}
	fn two() -> Self {
		2.0
	}
}
impl DCTnum for f64 {
	fn half() -> Self {
		0.5
	}
	fn two() -> Self {
		2.0
	}
}

#[inline(always)]
pub fn verify_length<T>(input: &[T], output: &[T], expected: usize) {
	assert_eq!(input.len(), expected, "Input is the wrong length. Expected {}, got {}", expected, input.len());
	assert_eq!(output.len(), expected, "Output is the wrong length. Expected {}, got {}", expected, output.len());
}


#[allow(unused)]
#[inline(always)]
pub fn verify_length_divisible<T>(input: &[T], output: &[T], expected: usize) {
	assert_eq!(input.len() % expected, 0, "Input is the wrong length. Expected multiple of {}, got {}", expected, input.len());
	assert_eq!(input.len(), output.len(), "Input and output must have the same length. Expected {}, got {}", input.len(), output.len());
}