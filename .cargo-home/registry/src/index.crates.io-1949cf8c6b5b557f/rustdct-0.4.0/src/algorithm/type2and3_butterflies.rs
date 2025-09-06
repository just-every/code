use std::marker::PhantomData;

use rustfft::num_complex::Complex;
use rustfft::Length;

use twiddles;
use ::{DCT2, DST2, DCT3, DST3, TransformType2And3};
use common;

macro_rules! butterfly_boilerplate {
    ($struct_name:ident, $size:expr) => (
        impl<T: common::DCTnum> DCT2<T> for $struct_name<T> {
            fn process_dct2(&self, input: &mut [T], output: &mut [T]) {
                common::verify_length(input, output, self.len());
                
                output.copy_from_slice(input);
                unsafe { self.process_inplace_dct2(output); }
            }
        }
        impl<T: common::DCTnum> DCT3<T> for $struct_name<T> {
            fn process_dct3(&self, input: &mut [T], output: &mut [T]) {
                common::verify_length(input, output, self.len());
                
                output.copy_from_slice(input);
                unsafe { self.process_inplace_dct3(output); }
            }
        }
        impl<T: common::DCTnum> DST2<T> for $struct_name<T> {
            fn process_dst2(&self, input: &mut [T], output: &mut [T]) {
                common::verify_length(input, output, self.len());
                
                output.copy_from_slice(input);
                unsafe { self.process_inplace_dst2(output); }
            }
        }
        impl<T: common::DCTnum> DST3<T> for $struct_name<T> {
            fn process_dst3(&self, input: &mut [T], output: &mut [T]) {
                common::verify_length(input, output, self.len());
                
                output.copy_from_slice(input);
                unsafe { self.process_inplace_dst3(output); }
            }
        }
        impl<T: common::DCTnum> TransformType2And3<T> for $struct_name<T>{}
        impl<T> Length for $struct_name<T> {
            fn len(&self) -> usize {
                $size
            }
        }
    )
}

pub struct Type2And3Butterfly2<T> {
    _phantom: PhantomData<T>
}
impl<T: common::DCTnum> Type2And3Butterfly2<T> {
	pub fn new() -> Self {
		Type2And3Butterfly2 {
            _phantom: PhantomData,
        }
	}
	pub unsafe fn process_inplace_dct2(&self, buffer: &mut [T]) {
		let sum = *buffer.get_unchecked(0) + *buffer.get_unchecked(1);
        *buffer.get_unchecked_mut(1) = (*buffer.get_unchecked(0) - *buffer.get_unchecked(1)) * T::FRAC_1_SQRT_2();
        *buffer.get_unchecked_mut(0) = sum;
	}
	unsafe fn process_scattered_dct2(buffer: &mut [T], zero: usize, one: usize) {
		let sum = *buffer.get_unchecked(zero) + *buffer.get_unchecked(one);
        *buffer.get_unchecked_mut(one) = (*buffer.get_unchecked(zero) - *buffer.get_unchecked(one)) * T::FRAC_1_SQRT_2();
        *buffer.get_unchecked_mut(zero) = sum;
	}

	pub unsafe fn process_inplace_dct3(&self, buffer: &mut [T]) {
		let half_0 = *buffer.get_unchecked(0) * T::half();
		let frac_1 = *buffer.get_unchecked(1) * T::FRAC_1_SQRT_2();

        *buffer.get_unchecked_mut(0) = half_0 + frac_1;
        *buffer.get_unchecked_mut(1) = half_0 - frac_1;
	}
	unsafe fn process_scattered_dct3(buffer: &mut [T], zero: usize, one: usize) {
		let half_0 = *buffer.get_unchecked(zero) * T::half();
		let frac_1 = *buffer.get_unchecked(one) * T::FRAC_1_SQRT_2();

        *buffer.get_unchecked_mut(zero) = half_0 + frac_1;
        *buffer.get_unchecked_mut(one) = half_0 - frac_1;
	}

    pub unsafe fn process_inplace_dst2(&self, buffer: &mut [T]) {
		let sum = *buffer.get_unchecked(0) - *buffer.get_unchecked(1);
        *buffer.get_unchecked_mut(0) = (*buffer.get_unchecked(0) + *buffer.get_unchecked(1)) * T::FRAC_1_SQRT_2();
        *buffer.get_unchecked_mut(1) = sum;
	}

    pub unsafe fn process_inplace_dst3(&self, buffer: &mut [T]) {
		
		let frac_0 = *buffer.get_unchecked(0) * T::FRAC_1_SQRT_2();
        let half_1 = *buffer.get_unchecked(1) * T::half();

        *buffer.get_unchecked_mut(0) = frac_0 + half_1;
        *buffer.get_unchecked_mut(1) = frac_0 - half_1;
	}
}
impl<T: common::DCTnum> DCT2<T> for Type2And3Butterfly2<T> {
    fn process_dct2(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        output[0] = input[0] + input[1];
        output[1] = (input[0] - input[1]) * T::FRAC_1_SQRT_2();      
    }
}
impl<T: common::DCTnum> DCT3<T> for Type2And3Butterfly2<T> {
    fn process_dct3(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

		let half_0 = input[0] * T::half();
		let frac_1 = input[1] * T::FRAC_1_SQRT_2();

		output[0] = half_0 + frac_1;
		output[1] = half_0 - frac_1;  
    }
}
impl<T: common::DCTnum> DST2<T> for Type2And3Butterfly2<T> {
    fn process_dst2(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        output[0] = (input[0] + input[1]) * T::FRAC_1_SQRT_2();
        output[1] = input[0] - input[1];      
    }
}
impl<T: common::DCTnum> DST3<T> for Type2And3Butterfly2<T> {
    fn process_dst3(&self, input: &mut [T], output: &mut [T]) {
        common::verify_length(input, output, self.len());

        let frac_0 = input[0] * T::FRAC_1_SQRT_2();
		let half_1 = input[1] * T::half();

		output[0] = frac_0 + half_1;  
        output[1] = frac_0 - half_1;
    }
}
impl<T: common::DCTnum> TransformType2And3<T> for Type2And3Butterfly2<T>{}
impl<T> Length for Type2And3Butterfly2<T> {
    fn len(&self) -> usize {
        2
    }
}

pub struct Type2And3Butterfly3<T> {
	twiddle: T,
}
impl<T: common::DCTnum> Type2And3Butterfly3<T> {
	pub fn new() -> Self {
		Self {
			twiddle: twiddles::single_twiddle_re(1,12),
		}
	}
	pub unsafe fn process_inplace_dct2(&self, buffer: &mut [T]) {
		// This algorithm is derived by hardcoding the dct2 naive algorithm for size 3
		let buffer_0 = *buffer.get_unchecked(0);
		let buffer_1 = *buffer.get_unchecked(1);
		let buffer_2 = *buffer.get_unchecked(2);

		*buffer.get_unchecked_mut(0) = buffer_0 + buffer_1 + buffer_2;
        *buffer.get_unchecked_mut(1) = (buffer_0 - buffer_2) * self.twiddle;
        *buffer.get_unchecked_mut(2) = (buffer_0 + buffer_2) * T::half() - buffer_1;
	}
    pub unsafe fn process_inplace_dct3(&self, buffer: &mut [T]) {
    	// This algorithm is derived by hardcoding the dct3 naive algorithm for size 3
    	let buffer0_half = *buffer.get_unchecked(0) * T::half();
    	let buffer1 = *buffer.get_unchecked(1);
    	let buffer2 = *buffer.get_unchecked(2);
    	let buffer2_half = buffer2 * T::half();

    	*buffer.get_unchecked_mut(0) = buffer0_half + buffer1 * self.twiddle + buffer2_half;
    	*buffer.get_unchecked_mut(1) = buffer0_half - buffer2;
    	*buffer.get_unchecked_mut(2) = buffer0_half + buffer1 * -self.twiddle + buffer2_half;
	}
    pub unsafe fn process_inplace_dst2(&self, buffer: &mut [T]) {
    	// This algorithm is derived by hardcoding the dct2 naive algorithm for size 3, then negating the odd inputs and revering the outputs
		let buffer_0 = *buffer.get_unchecked(0);
		let buffer_1 = *buffer.get_unchecked(1);
		let buffer_2 = *buffer.get_unchecked(2);

		*buffer.get_unchecked_mut(2) = buffer_0 - buffer_1 + buffer_2;
        *buffer.get_unchecked_mut(1) = (buffer_0 - buffer_2) * self.twiddle;
        *buffer.get_unchecked_mut(0) = (buffer_0 + buffer_2) * T::half() + buffer_1;
	}
    pub unsafe fn process_inplace_dst3(&self, buffer: &mut [T]) {
    	// This algorithm is derived by hardcoding the dct3 naive algorithm for size 3, then reversing the inputs and negating the odd outputs
    	let buffer0_half = *buffer.get_unchecked(2) * T::half();
    	let buffer1 = *buffer.get_unchecked(1);
    	let buffer2 = *buffer.get_unchecked(0);
    	let buffer2_half = buffer2 * T::half();

    	*buffer.get_unchecked_mut(0) = buffer0_half + buffer1 * self.twiddle + buffer2_half;
    	*buffer.get_unchecked_mut(1) = buffer2 - buffer0_half;
    	*buffer.get_unchecked_mut(2) = buffer0_half + buffer1 * -self.twiddle + buffer2_half;
	}
}
butterfly_boilerplate!(Type2And3Butterfly3, 3);

pub struct Type2And3Butterfly4<T> {
	twiddle: Complex<T>,
}
impl<T: common::DCTnum> Type2And3Butterfly4<T> {
	pub fn new() -> Self {
		Type2And3Butterfly4 {
			twiddle: twiddles::single_twiddle(1,16).conj()
		}
	}
	pub unsafe fn process_inplace_dct2(&self, buffer: &mut [T]) {
		// perform a step of split radix -- derived from DCT2SplitRadix with n = 4

		let lower_dct4 = *buffer.get_unchecked(0) - *buffer.get_unchecked(3);
		let upper_dct4 = *buffer.get_unchecked(2) - *buffer.get_unchecked(1);

		*buffer.get_unchecked_mut(0) = *buffer.get_unchecked(0) + *buffer.get_unchecked(3);
		*buffer.get_unchecked_mut(2) = *buffer.get_unchecked(2) + *buffer.get_unchecked(1);

        Type2And3Butterfly2::process_scattered_dct2(buffer, 0, 2);

        *buffer.get_unchecked_mut(1) = lower_dct4 * self.twiddle.re - upper_dct4 * self.twiddle.im;
        *buffer.get_unchecked_mut(3) = upper_dct4 * self.twiddle.re + lower_dct4 * self.twiddle.im;
	}
    pub unsafe fn process_inplace_dct3(&self, buffer: &mut [T]) {
		// perform a step of split radix -- derived from DCT3SplitRadix with n = 4

		// inner DCT3 of size 2
		Type2And3Butterfly2::process_scattered_dct3(buffer, 0, 2);

		// inner DCT3 of size 1, then sclared by twiddle factors
		let lower_dct4 = *buffer.get_unchecked(1) * self.twiddle.re + *buffer.get_unchecked(3) * self.twiddle.im;
        let upper_dct4 = *buffer.get_unchecked(1) * self.twiddle.im - *buffer.get_unchecked(3) * self.twiddle.re;

		// Merge our results
		*buffer.get_unchecked_mut(1) = *buffer.get_unchecked(2) + upper_dct4;
		*buffer.get_unchecked_mut(3) = *buffer.get_unchecked(0) - lower_dct4;
		*buffer.get_unchecked_mut(0) = *buffer.get_unchecked(0) + lower_dct4;
		*buffer.get_unchecked_mut(2) = *buffer.get_unchecked(2) - upper_dct4;
	}
    pub unsafe fn process_inplace_dst2(&self, buffer: &mut [T]) {
		// Derived from process_inplace_dct2 by negating the odd inputs, and reversing the outputs

		let lower_dct4 = *buffer.get_unchecked(0) + *buffer.get_unchecked(3);
		let upper_dct4 = *buffer.get_unchecked(2) + *buffer.get_unchecked(1);

		*buffer.get_unchecked_mut(3) = *buffer.get_unchecked(0) - *buffer.get_unchecked(3);
		*buffer.get_unchecked_mut(1) = *buffer.get_unchecked(2) - *buffer.get_unchecked(1);

        Type2And3Butterfly2::process_scattered_dct2(buffer, 3, 1);

        *buffer.get_unchecked_mut(2) = lower_dct4 * self.twiddle.re - upper_dct4 * self.twiddle.im;
        *buffer.get_unchecked_mut(0) = upper_dct4 * self.twiddle.re + lower_dct4 * self.twiddle.im;
	}
    pub unsafe fn process_inplace_dst3(&self, buffer: &mut [T]) {
		// Derived from process_inplace_dst3 by reversing the inputs, and negating the odd outputs

		// inner DCT3 of size 2
		Type2And3Butterfly2::process_scattered_dct3(buffer, 3, 1);

		// inner DCT3 of size 1, then sclared by twiddle factors
		let lower_dct4 = *buffer.get_unchecked(2) * self.twiddle.re + *buffer.get_unchecked(0) * self.twiddle.im;
        let upper_dct4 = *buffer.get_unchecked(2) * self.twiddle.im - *buffer.get_unchecked(0) * self.twiddle.re;

		// Merge our results
		*buffer.get_unchecked_mut(0) = *buffer.get_unchecked(3) + lower_dct4;
		*buffer.get_unchecked_mut(2) = *buffer.get_unchecked(1) - upper_dct4;
        *buffer.get_unchecked_mut(1) = -(*buffer.get_unchecked(1) + upper_dct4);
		*buffer.get_unchecked_mut(3) = lower_dct4 - *buffer.get_unchecked(3);
	}
}
butterfly_boilerplate!(Type2And3Butterfly4, 4);

pub struct Type2And3Butterfly8<T> {
	butterfly4: Type2And3Butterfly4<T>,
	butterfly2: Type2And3Butterfly2<T>,
    twiddles: [Complex<T>; 2],
}
impl<T: common::DCTnum> Type2And3Butterfly8<T> {
	pub fn new() -> Self {
		Type2And3Butterfly8 {
			butterfly4: Type2And3Butterfly4::new(),
			butterfly2: Type2And3Butterfly2::new(),
			twiddles: [
                twiddles::single_twiddle(1,32).conj(),
			    twiddles::single_twiddle(3,32).conj(),
            ],
		}
	}
	pub unsafe fn process_inplace_dct2(&self, buffer: &mut [T]) {
		// perform a step of split radix -- derived from DCT2SplitRadix with n = 8

		//process the evens
		let mut dct2_buffer = [
			*buffer.get_unchecked(0) + *buffer.get_unchecked(7),
			*buffer.get_unchecked(1) + *buffer.get_unchecked(6),
			*buffer.get_unchecked(2) + *buffer.get_unchecked(5),
			*buffer.get_unchecked(3) + *buffer.get_unchecked(4),
		];
		self.butterfly4.process_inplace_dct2(&mut dct2_buffer);

		//process the odds
		let differences = [
			*buffer.get_unchecked(0) - *buffer.get_unchecked(7),
			*buffer.get_unchecked(3) - *buffer.get_unchecked(4),
			*buffer.get_unchecked(1) - *buffer.get_unchecked(6),
			*buffer.get_unchecked(2) - *buffer.get_unchecked(5),
		];

		let mut dct4_even_buffer = [
			differences[0] * self.twiddles[0].re + differences[1] * self.twiddles[0].im,
			differences[2] * self.twiddles[1].re + differences[3] * self.twiddles[1].im,
		];
		let mut dct4_odd_buffer = [
			differences[3] * self.twiddles[1].re - differences[2] * self.twiddles[1].im,
			differences[1] * self.twiddles[0].re - differences[0] * self.twiddles[0].im,
		];

		self.butterfly2.process_inplace_dct2(&mut dct4_even_buffer);
		self.butterfly2.process_inplace_dst2(&mut dct4_odd_buffer);

		// combine the results
		*buffer.get_unchecked_mut(0) = dct2_buffer[0];
		*buffer.get_unchecked_mut(1) = dct4_even_buffer[0];
		*buffer.get_unchecked_mut(2) = dct2_buffer[1];
		*buffer.get_unchecked_mut(3) = dct4_even_buffer[1] - dct4_odd_buffer[0];
		*buffer.get_unchecked_mut(4) = dct2_buffer[2];
		*buffer.get_unchecked_mut(5) = dct4_even_buffer[1] + dct4_odd_buffer[0];
		*buffer.get_unchecked_mut(6) = dct2_buffer[3];
		*buffer.get_unchecked_mut(7) = dct4_odd_buffer[1];
	}

    pub unsafe fn process_inplace_dct3(&self, buffer: &mut [T]) {
		// perform a step of split radix -- derived from DCT3SplitRadix with n = 8

		//process the evens
		let mut dct3_buffer = [
			*buffer.get_unchecked(0),
			*buffer.get_unchecked(2),
			*buffer.get_unchecked(4),
			*buffer.get_unchecked(6),
		];
		self.butterfly4.process_inplace_dct3(&mut dct3_buffer);

		//process the odds
		let mut recursive_buffer_n1 = [
			*buffer.get_unchecked(1) * T::two(),
			*buffer.get_unchecked(3) + *buffer.get_unchecked(5),
		];
		let mut recursive_buffer_n3 = [
			*buffer.get_unchecked(3) - *buffer.get_unchecked(5),
            *buffer.get_unchecked(7) * T::two(),
		];
		self.butterfly2.process_inplace_dct3(&mut recursive_buffer_n1);
		self.butterfly2.process_inplace_dst3(&mut recursive_buffer_n3);

		// merge the temp buffers into the final output
		for i in 0..2 {
			let twiddle = self.twiddles[i];

            let lower_dct4 = recursive_buffer_n1[i] * twiddle.re + recursive_buffer_n3[i] * twiddle.im;
            let upper_dct4 = recursive_buffer_n1[i] * twiddle.im - recursive_buffer_n3[i] * twiddle.re;

            let lower_dct3 = dct3_buffer[i];
            let upper_dct3 = dct3_buffer[3 - i];

            *buffer.get_unchecked_mut(i) =     lower_dct3 + lower_dct4;
            *buffer.get_unchecked_mut(7 - i) = lower_dct3 - lower_dct4;

            *buffer.get_unchecked_mut(3 - i) = upper_dct3 + upper_dct4;
            *buffer.get_unchecked_mut(4 + i) = upper_dct3 - upper_dct4;
		}
	}

    pub unsafe fn process_inplace_dst2(&self, buffer: &mut [T]) {
		// Derived from process_inplace_dct2, negating the odd inputs and reversing the outputs

		//process the evens
		let mut dct2_buffer = [
			*buffer.get_unchecked(0) - *buffer.get_unchecked(7),
			*buffer.get_unchecked(6) - *buffer.get_unchecked(1),
			*buffer.get_unchecked(2) - *buffer.get_unchecked(5),
			*buffer.get_unchecked(4) - *buffer.get_unchecked(3),
		];
		self.butterfly4.process_inplace_dct2(&mut dct2_buffer);

		//process the odds
		let differences = [
			*buffer.get_unchecked(0) + *buffer.get_unchecked(7),
			-*buffer.get_unchecked(3) - *buffer.get_unchecked(4),
			-*buffer.get_unchecked(1) - *buffer.get_unchecked(6),
			*buffer.get_unchecked(2) + *buffer.get_unchecked(5),
		];

		let mut dct4_even_buffer = [
			differences[0] * self.twiddles[0].re + differences[1] * self.twiddles[0].im,
			differences[2] * self.twiddles[1].re + differences[3] * self.twiddles[1].im,
		];
		let mut dct4_odd_buffer = [
			differences[3] * self.twiddles[1].re - differences[2] * self.twiddles[1].im,
			differences[1] * self.twiddles[0].re - differences[0] * self.twiddles[0].im,
		];

		self.butterfly2.process_inplace_dct2(&mut dct4_even_buffer);
		self.butterfly2.process_inplace_dst2(&mut dct4_odd_buffer);

		// combine the results
		*buffer.get_unchecked_mut(7) = dct2_buffer[0];
		*buffer.get_unchecked_mut(6) = dct4_even_buffer[0];
		*buffer.get_unchecked_mut(5) = dct2_buffer[1];
		*buffer.get_unchecked_mut(4) = dct4_even_buffer[1] - dct4_odd_buffer[0];
		*buffer.get_unchecked_mut(3) = dct2_buffer[2];
		*buffer.get_unchecked_mut(2) = dct4_even_buffer[1] + dct4_odd_buffer[0];
		*buffer.get_unchecked_mut(1) = dct2_buffer[3];
		*buffer.get_unchecked_mut(0) = dct4_odd_buffer[1];
	}

    pub unsafe fn process_inplace_dst3(&self, buffer: &mut [T]) {
		// Derived from process_inplace_dct3, reversing the inputs and negating the odd outputs

		//process the evens
		let mut dct3_buffer = [
			*buffer.get_unchecked(7),
			*buffer.get_unchecked(5),
			*buffer.get_unchecked(3),
			*buffer.get_unchecked(1),
		];
		self.butterfly4.process_inplace_dct3(&mut dct3_buffer);

		//process the odds
		let mut recursive_buffer_n1 = [
			*buffer.get_unchecked(6) * T::two(),
			*buffer.get_unchecked(4) + *buffer.get_unchecked(2),
		];
		let mut recursive_buffer_n3 = [
			*buffer.get_unchecked(4) - *buffer.get_unchecked(2),
            *buffer.get_unchecked(0) * T::two(),
		];
		self.butterfly2.process_inplace_dct3(&mut recursive_buffer_n1);
		self.butterfly2.process_inplace_dst3(&mut recursive_buffer_n3);

        let merged_odds = [
            recursive_buffer_n1[0] * self.twiddles[0].re + recursive_buffer_n3[0] * self.twiddles[0].im,
            recursive_buffer_n1[0] * self.twiddles[0].im - recursive_buffer_n3[0] * self.twiddles[0].re,
            recursive_buffer_n1[1] * self.twiddles[1].re + recursive_buffer_n3[1] * self.twiddles[1].im,
            recursive_buffer_n1[1] * self.twiddles[1].im - recursive_buffer_n3[1] * self.twiddles[1].re,
        ];

		// merge the temp buffers into the final output
        *buffer.get_unchecked_mut(0) = dct3_buffer[0] + merged_odds[0];
        *buffer.get_unchecked_mut(7) = merged_odds[0] - dct3_buffer[0];

        *buffer.get_unchecked_mut(3) = -(dct3_buffer[3] + merged_odds[1]);
        *buffer.get_unchecked_mut(4) = dct3_buffer[3] - merged_odds[1];

        *buffer.get_unchecked_mut(1) = -(dct3_buffer[1] + merged_odds[2]);
        *buffer.get_unchecked_mut(6) = dct3_buffer[1] - merged_odds[2];

        *buffer.get_unchecked_mut(2) = dct3_buffer[2] + merged_odds[3];
        *buffer.get_unchecked_mut(5) = merged_odds[3] - dct3_buffer[2];
    }
}
butterfly_boilerplate!(Type2And3Butterfly8, 8);

pub struct Type2And3Butterfly16<T> {
	butterfly8: Type2And3Butterfly8<T>,
	butterfly4: Type2And3Butterfly4<T>,
    twiddles: [Complex<T>; 4],
}
impl<T: common::DCTnum> Type2And3Butterfly16<T> {
	pub fn new() -> Self {
		Type2And3Butterfly16 {
			butterfly8: Type2And3Butterfly8::new(),
			butterfly4: Type2And3Butterfly4::new(),
            twiddles: [
                twiddles::single_twiddle(1,64).conj(),
                twiddles::single_twiddle(3,64).conj(),
                twiddles::single_twiddle(5,64).conj(),
                twiddles::single_twiddle(7,64).conj(),
            ],
		}
	}
	pub unsafe fn process_inplace_dct2(&self, buffer: &mut [T]) {
		// perform a step of split radix -- derived from DCT2SplitRadix with n = 16

		//process the evens
		let mut dct2_buffer = [
			*buffer.get_unchecked(0) + *buffer.get_unchecked(15),
			*buffer.get_unchecked(1) + *buffer.get_unchecked(14),
			*buffer.get_unchecked(2) + *buffer.get_unchecked(13),
			*buffer.get_unchecked(3) + *buffer.get_unchecked(12),
			*buffer.get_unchecked(4) + *buffer.get_unchecked(11),
			*buffer.get_unchecked(5) + *buffer.get_unchecked(10),
			*buffer.get_unchecked(6) + *buffer.get_unchecked(9),
			*buffer.get_unchecked(7) + *buffer.get_unchecked(8),
		];
		self.butterfly8.process_inplace_dct2(&mut dct2_buffer);

		//process the odds
		let differences = [
			*buffer.get_unchecked(0) - *buffer.get_unchecked(15),
			*buffer.get_unchecked(7) - *buffer.get_unchecked(8),
			*buffer.get_unchecked(1) - *buffer.get_unchecked(14),
			*buffer.get_unchecked(6) - *buffer.get_unchecked(9),
			*buffer.get_unchecked(2) - *buffer.get_unchecked(13),
			*buffer.get_unchecked(5) - *buffer.get_unchecked(10),
			*buffer.get_unchecked(3) - *buffer.get_unchecked(12),
			*buffer.get_unchecked(4) - *buffer.get_unchecked(11),
		];

		let mut dct4_even_buffer = [
			differences[0] * self.twiddles[0].re + differences[1] * self.twiddles[0].im,
			differences[2] * self.twiddles[1].re + differences[3] * self.twiddles[1].im,
			differences[4] * self.twiddles[2].re + differences[5] * self.twiddles[2].im,
			differences[6] * self.twiddles[3].re + differences[7] * self.twiddles[3].im,
		];
		let mut dct4_odd_buffer = [
			differences[7] * self.twiddles[3].re - differences[6] * self.twiddles[3].im,
			differences[5] * self.twiddles[2].re - differences[4] * self.twiddles[2].im,
			differences[3] * self.twiddles[1].re - differences[2] * self.twiddles[1].im,
			differences[1] * self.twiddles[0].re - differences[0] * self.twiddles[0].im,
		];

		self.butterfly4.process_inplace_dct2(&mut dct4_even_buffer);
		self.butterfly4.process_inplace_dst2(&mut dct4_odd_buffer);

		// combine the results
		*buffer.get_unchecked_mut(0) =  dct2_buffer[0];
		*buffer.get_unchecked_mut(1) =  dct4_even_buffer[0];
		*buffer.get_unchecked_mut(2) =  dct2_buffer[1];
		*buffer.get_unchecked_mut(3) =  dct4_even_buffer[1] - dct4_odd_buffer[0];
		*buffer.get_unchecked_mut(4) =  dct2_buffer[2];
		*buffer.get_unchecked_mut(5) =  dct4_even_buffer[1] + dct4_odd_buffer[0];
		*buffer.get_unchecked_mut(6) =  dct2_buffer[3];
		*buffer.get_unchecked_mut(7) =  dct4_even_buffer[2] + dct4_odd_buffer[1];
		*buffer.get_unchecked_mut(8) =  dct2_buffer[4];
		*buffer.get_unchecked_mut(9) =  dct4_even_buffer[2] - dct4_odd_buffer[1];
		*buffer.get_unchecked_mut(10) = dct2_buffer[5];
		*buffer.get_unchecked_mut(11) = dct4_even_buffer[3] - dct4_odd_buffer[2];
		*buffer.get_unchecked_mut(12) = dct2_buffer[6];
		*buffer.get_unchecked_mut(13) = dct4_even_buffer[3] + dct4_odd_buffer[2];
		*buffer.get_unchecked_mut(14) = dct2_buffer[7];
		*buffer.get_unchecked_mut(15) = dct4_odd_buffer[3];
	}
    pub unsafe fn process_inplace_dst2(&self, buffer: &mut [T]) {
		// Derived from process_inplace_dct2, negating the odd inputs and reversing the outputs

		//process the evens
		let mut dct2_buffer = [
			 *buffer.get_unchecked(0) - *buffer.get_unchecked(15),
			-*buffer.get_unchecked(1) + *buffer.get_unchecked(14),
			 *buffer.get_unchecked(2) - *buffer.get_unchecked(13),
			-*buffer.get_unchecked(3) + *buffer.get_unchecked(12),
			 *buffer.get_unchecked(4) - *buffer.get_unchecked(11),
			-*buffer.get_unchecked(5) + *buffer.get_unchecked(10),
			 *buffer.get_unchecked(6) - *buffer.get_unchecked(9),
			-*buffer.get_unchecked(7) + *buffer.get_unchecked(8),
		];
		self.butterfly8.process_inplace_dct2(&mut dct2_buffer);

		//process the odds
		let differences = [
			 *buffer.get_unchecked(0) + *buffer.get_unchecked(15),
			-*buffer.get_unchecked(7) - *buffer.get_unchecked(8),
			-*buffer.get_unchecked(1) - *buffer.get_unchecked(14),
			 *buffer.get_unchecked(6) + *buffer.get_unchecked(9),
			 *buffer.get_unchecked(2) + *buffer.get_unchecked(13),
			-*buffer.get_unchecked(5) - *buffer.get_unchecked(10),
			-*buffer.get_unchecked(3) - *buffer.get_unchecked(12),
			 *buffer.get_unchecked(4) + *buffer.get_unchecked(11),
		];

		let mut dct4_even_buffer = [
			differences[0] * self.twiddles[0].re + differences[1] * self.twiddles[0].im,
			differences[2] * self.twiddles[1].re + differences[3] * self.twiddles[1].im,
			differences[4] * self.twiddles[2].re + differences[5] * self.twiddles[2].im,
			differences[6] * self.twiddles[3].re + differences[7] * self.twiddles[3].im,
		];
		let mut dct4_odd_buffer = [
			differences[7] * self.twiddles[3].re - differences[6] * self.twiddles[3].im,
			differences[5] * self.twiddles[2].re - differences[4] * self.twiddles[2].im,
			differences[3] * self.twiddles[1].re - differences[2] * self.twiddles[1].im,
			differences[1] * self.twiddles[0].re - differences[0] * self.twiddles[0].im,
		];

		self.butterfly4.process_inplace_dct2(&mut dct4_even_buffer);
		self.butterfly4.process_inplace_dst2(&mut dct4_odd_buffer);

		// combine the results
		*buffer.get_unchecked_mut(15) =  dct2_buffer[0];
		*buffer.get_unchecked_mut(14) =  dct4_even_buffer[0];
		*buffer.get_unchecked_mut(13) =  dct2_buffer[1];
		*buffer.get_unchecked_mut(12) =  dct4_even_buffer[1] - dct4_odd_buffer[0];
		*buffer.get_unchecked_mut(11) =  dct2_buffer[2];
		*buffer.get_unchecked_mut(10) =  dct4_even_buffer[1] + dct4_odd_buffer[0];
		*buffer.get_unchecked_mut(9) =  dct2_buffer[3];
		*buffer.get_unchecked_mut(8) =  dct4_even_buffer[2] + dct4_odd_buffer[1];
		*buffer.get_unchecked_mut(7) =  dct2_buffer[4];
		*buffer.get_unchecked_mut(6) =  dct4_even_buffer[2] - dct4_odd_buffer[1];
		*buffer.get_unchecked_mut(5) = dct2_buffer[5];
		*buffer.get_unchecked_mut(4) = dct4_even_buffer[3] - dct4_odd_buffer[2];
		*buffer.get_unchecked_mut(3) = dct2_buffer[6];
		*buffer.get_unchecked_mut(2) = dct4_even_buffer[3] + dct4_odd_buffer[2];
		*buffer.get_unchecked_mut(1) = dct2_buffer[7];
		*buffer.get_unchecked_mut(0) = dct4_odd_buffer[3];
	}
    pub unsafe fn process_inplace_dct3(&self, buffer: &mut [T]) {
		// perform a step of split radix -- derived from DCT3SplitRadix with n = 16

		//process the evens
		let mut dct3_buffer = [
			*buffer.get_unchecked(0),
			*buffer.get_unchecked(2),
			*buffer.get_unchecked(4),
			*buffer.get_unchecked(6),
			*buffer.get_unchecked(8),
			*buffer.get_unchecked(10),
			*buffer.get_unchecked(12),
			*buffer.get_unchecked(14)
		];
		self.butterfly8.process_inplace_dct3(&mut dct3_buffer);

		//process the odds
		let mut recursive_buffer_n1 = [
			*buffer.get_unchecked(1) * T::two(),
			*buffer.get_unchecked(3) + *buffer.get_unchecked(5),
			*buffer.get_unchecked(7) + *buffer.get_unchecked(9),
			*buffer.get_unchecked(11) + *buffer.get_unchecked(13),
		];
		let mut recursive_buffer_n3 = [
            *buffer.get_unchecked(3) - *buffer.get_unchecked(5),
            *buffer.get_unchecked(7) - *buffer.get_unchecked(9),
            *buffer.get_unchecked(11) - *buffer.get_unchecked(13),
			*buffer.get_unchecked(15) * T::two(),
		];
		self.butterfly4.process_inplace_dct3(&mut recursive_buffer_n1);
		self.butterfly4.process_inplace_dst3(&mut recursive_buffer_n3);

		// merge the temp buffers into the final output
		for i in 0..4 {
            let lower_dct4 = recursive_buffer_n1[i] * self.twiddles[i].re + recursive_buffer_n3[i] * self.twiddles[i].im;
            let upper_dct4 = recursive_buffer_n1[i] * self.twiddles[i].im - recursive_buffer_n3[i] * self.twiddles[i].re;

            let lower_dct3 = dct3_buffer[i];
            let upper_dct3 = dct3_buffer[7 - i];

            *buffer.get_unchecked_mut(i) =     lower_dct3 + lower_dct4;
            *buffer.get_unchecked_mut(15 - i) = lower_dct3 - lower_dct4;

            *buffer.get_unchecked_mut(7 - i) = upper_dct3 + upper_dct4;
            *buffer.get_unchecked_mut(8 + i) = upper_dct3 - upper_dct4;
		}
	}
    pub unsafe fn process_inplace_dst3(&self, buffer: &mut [T]) {
		// Derived from process_inplace_dct3, reversing the inputs and negating the odd outputs

		//process the evens
		let mut dct3_buffer = [
			*buffer.get_unchecked(15),
			*buffer.get_unchecked(13),
			*buffer.get_unchecked(11),
			*buffer.get_unchecked(9),
			*buffer.get_unchecked(7),
			*buffer.get_unchecked(5),
			*buffer.get_unchecked(3),
			*buffer.get_unchecked(1)
		];
		self.butterfly8.process_inplace_dct3(&mut dct3_buffer);

		//process the odds
		let mut recursive_buffer_n1 = [
			*buffer.get_unchecked(14) * T::two(),
			*buffer.get_unchecked(12) + *buffer.get_unchecked(10),
			*buffer.get_unchecked(8) + *buffer.get_unchecked(6),
			*buffer.get_unchecked(4) + *buffer.get_unchecked(2),
		];
		let mut recursive_buffer_n3 = [
            *buffer.get_unchecked(12) - *buffer.get_unchecked(10),
            *buffer.get_unchecked(8) - *buffer.get_unchecked(6),
            *buffer.get_unchecked(4) - *buffer.get_unchecked(2),
			*buffer.get_unchecked(0) * T::two(),
		];
		self.butterfly4.process_inplace_dct3(&mut recursive_buffer_n1);
		self.butterfly4.process_inplace_dst3(&mut recursive_buffer_n3);

		let merged_odds = [
            recursive_buffer_n1[0] * self.twiddles[0].re + recursive_buffer_n3[0] * self.twiddles[0].im,
            recursive_buffer_n1[0] * self.twiddles[0].im - recursive_buffer_n3[0] * self.twiddles[0].re,
            recursive_buffer_n1[1] * self.twiddles[1].re + recursive_buffer_n3[1] * self.twiddles[1].im,
            recursive_buffer_n1[1] * self.twiddles[1].im - recursive_buffer_n3[1] * self.twiddles[1].re,
            recursive_buffer_n1[2] * self.twiddles[2].re + recursive_buffer_n3[2] * self.twiddles[2].im,
            recursive_buffer_n1[2] * self.twiddles[2].im - recursive_buffer_n3[2] * self.twiddles[2].re,
            recursive_buffer_n1[3] * self.twiddles[3].re + recursive_buffer_n3[3] * self.twiddles[3].im,
            recursive_buffer_n1[3] * self.twiddles[3].im - recursive_buffer_n3[3] * self.twiddles[3].re,
        ];

		// merge the temp buffers into the final output
        *buffer.get_unchecked_mut(0) =      dct3_buffer[0] + merged_odds[0];
        *buffer.get_unchecked_mut(15) =     merged_odds[0] - dct3_buffer[0];

        *buffer.get_unchecked_mut(7) =      -(dct3_buffer[7] + merged_odds[1]);
        *buffer.get_unchecked_mut(8) =      dct3_buffer[7] - merged_odds[1];

        *buffer.get_unchecked_mut(1) =      -(dct3_buffer[1] + merged_odds[2]);
        *buffer.get_unchecked_mut(14) =     dct3_buffer[1] - merged_odds[2];

        *buffer.get_unchecked_mut(6) =      dct3_buffer[6] + merged_odds[3];
        *buffer.get_unchecked_mut(9) =      merged_odds[3] - dct3_buffer[6];

        *buffer.get_unchecked_mut(2) =      dct3_buffer[2] + merged_odds[4];
        *buffer.get_unchecked_mut(13) =     merged_odds[4] - dct3_buffer[2];

        *buffer.get_unchecked_mut(5) =      -(dct3_buffer[5] + merged_odds[5]);
        *buffer.get_unchecked_mut(10) =     dct3_buffer[5] - merged_odds[5];

        *buffer.get_unchecked_mut(3) =      -(dct3_buffer[3] + merged_odds[6]);
        *buffer.get_unchecked_mut(12) =     dct3_buffer[3] - merged_odds[6];

        *buffer.get_unchecked_mut(4) =      dct3_buffer[4] + merged_odds[7];
        *buffer.get_unchecked_mut(11) =     merged_odds[7] - dct3_buffer[4];
	}
}
butterfly_boilerplate!(Type2And3Butterfly16, 16);

#[cfg(test)]
mod test {
    use super::*;

    //the tests for all butterflies will be identical except for the identifiers used and size
    //so it's ideal for a macro
    macro_rules! test_butterfly_func {
        ($test_name:ident, $struct_name:ident, $size:expr) => (
            mod $test_name {
                use super::*;
                use algorithm::Type2And3Naive;
                use test_utils::{compare_float_vectors, random_signal};
                #[test]
                fn $test_name() {
                    let size = $size;
                    println!("{}", size);

                    let naive = Type2And3Naive::new(size);
                    let butterfly = $struct_name::new();

                    check_dct2(&butterfly, &naive);
                    check_dct3(&butterfly, &naive);
                    check_dst2(&butterfly, &naive);
                    check_dst3(&butterfly, &naive);
                }

                fn check_dct2(butterfly_instance: &$struct_name<f32>, naive_instance: &dyn DCT2<f32>) {
                    // set up buffers
                    let expected_input = random_signal($size);
                    
                    let mut expected_output = vec![0f32; $size];
                    let mut inplace_buffer = expected_input.clone();
                    let mut actual_output = expected_output.clone();

                    // perform the test
                    naive_instance.process_dct2(&mut expected_input.clone(), &mut expected_output);

                    unsafe { butterfly_instance.process_inplace_dct2(&mut inplace_buffer); }

                    butterfly_instance.process_dct2(&mut expected_input.clone(), &mut actual_output);
                    println!("");
                    println!("expected output: {:?}", expected_output);
                    println!("inplace output:  {:?}", inplace_buffer);
                    println!("process output:  {:?}", actual_output);

                    assert!(compare_float_vectors(&expected_output, &inplace_buffer), "process_inplace_dct2() failed, length = {}", $size);
                    assert!(compare_float_vectors(&expected_output, &actual_output), "process_dct2() failed, length = {}", $size);
                }

                fn check_dct3(butterfly_instance: &$struct_name<f32>, naive_instance: &dyn DCT3<f32>) {
                    // set up buffers
                    let expected_input = random_signal($size);
                    
                    let mut expected_output = vec![0f32; $size];
                    let mut inplace_buffer = expected_input.clone();
                    let mut actual_output = expected_output.clone();

                    // perform the test
                    naive_instance.process_dct3(&mut expected_input.clone(), &mut expected_output);

                    unsafe { butterfly_instance.process_inplace_dct3(&mut inplace_buffer); }

                    butterfly_instance.process_dct3(&mut expected_input.clone(), &mut actual_output);
                    println!("");
                    println!("expected output: {:?}", expected_output);
                    println!("inplace output:  {:?}", inplace_buffer);
                    println!("process output:  {:?}", actual_output);

                    assert!(compare_float_vectors(&expected_output, &inplace_buffer), "process_inplace_dct3() failed, length = {}", $size);
                    assert!(compare_float_vectors(&expected_output, &actual_output), "process_dct3() failed, length = {}", $size);
                }

                fn check_dst2(butterfly_instance: &$struct_name<f32>, naive_instance: &dyn DST2<f32>) {
                    // set up buffers
                    let expected_input = random_signal($size);
                    
                    let mut expected_output = vec![0f32; $size];
                    let mut inplace_buffer = expected_input.clone();
                    let mut actual_output = expected_output.clone();

                    // perform the test
                    naive_instance.process_dst2(&mut expected_input.clone(), &mut expected_output);

                    unsafe { butterfly_instance.process_inplace_dst2(&mut inplace_buffer); }

                    butterfly_instance.process_dst2(&mut expected_input.clone(), &mut actual_output);
                    println!("");
                    println!("expected output: {:?}", expected_output);
                    println!("inplace output:  {:?}", inplace_buffer);
                    println!("process output:  {:?}", actual_output);

                    assert!(compare_float_vectors(&expected_output, &inplace_buffer), "process_inplace_dst2() failed, length = {}", $size);
                    assert!(compare_float_vectors(&expected_output, &actual_output), "process_dst2() failed, length = {}", $size);
                }

                fn check_dst3(butterfly_instance: &$struct_name<f32>, naive_instance: &dyn DST3<f32>) {
                    // set up buffers
                    let expected_input = random_signal($size);
                    
                    let mut expected_output = vec![0f32; $size];
                    let mut inplace_buffer = expected_input.clone();
                    let mut actual_output = expected_output.clone();

                    // perform the test
                    naive_instance.process_dst3(&mut expected_input.clone(), &mut expected_output);

                    unsafe { butterfly_instance.process_inplace_dst3(&mut inplace_buffer); }

                    butterfly_instance.process_dst3(&mut expected_input.clone(), &mut actual_output);
                    println!("");
                    println!("expected output: {:?}", expected_output);
                    println!("inplace output:  {:?}", inplace_buffer);
                    println!("process output:  {:?}", actual_output);

                    assert!(compare_float_vectors(&expected_output, &inplace_buffer), "process_inplace_dst3() failed, length = {}", $size);
                    assert!(compare_float_vectors(&expected_output, &actual_output), "process_dst3() failed, length = {}", $size);
                }
            }
        )
    }
    test_butterfly_func!(test_butterfly2_type2and3, Type2And3Butterfly2, 2);
    test_butterfly_func!(test_butterfly3_type2and3, Type2And3Butterfly3, 3);
    test_butterfly_func!(test_butterfly4_type2and3, Type2And3Butterfly4, 4);
    test_butterfly_func!(test_butterfly8_type2and3, Type2And3Butterfly8, 8);
    test_butterfly_func!(test_butterfly16_type2and3, Type2And3Butterfly16, 16);
}