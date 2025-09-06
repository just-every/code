
/// Given an array of size width * height, representing a flattened 2D array,
/// transpose the rows and columns of that 2D array into the output
/// benchmarking shows that loop tiling isn't effective for small arrays (in the range of 50x50 or smaller)
pub unsafe fn transpose_small<T: Copy>(width: usize, height: usize, input: &[T], output: &mut [T]) {
    for x in 0..width {
        for y in 0..height {
            let input_index = x + y * width;
            let output_index = y + x * height;

            *output.get_unchecked_mut(output_index) = *input.get_unchecked(input_index);
        }
    }
}



#[cfg(test)]
mod unit_tests {
    use super::*;
    use test_utils::random_signal;
    use num_complex::Complex;
    use num_traits::Zero;

    #[test]
    fn test_transpose() {
        let sizes: Vec<usize> = (1..16).collect();

        for &width in &sizes {
            for &height in &sizes {
                let len = width * height;

                let input: Vec<Complex<f32>> = random_signal(len);
                let mut output = vec![Zero::zero(); len];

                unsafe { transpose_small(width, height, &input, &mut output) };

                for x in 0..width {
                    for y in 0..height {
                        assert_eq!(input[x + y * width], output[y + x * height], "x = {}, y = {}", x, y);
                    }
                }

            }
        }
    }
}
