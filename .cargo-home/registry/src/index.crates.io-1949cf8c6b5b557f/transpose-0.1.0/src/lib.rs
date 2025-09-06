//! Utility for transposing multi-dimensional data stored as a flat slice
//!
//! This library treats Rust slices as flattened row-major 2D arrays, and provides functions to transpose these 2D arrays, so that the row data becomes the column data, and vice versa.
//! ```
//! // Create a 2D array in row-major order: the rows of our 2D array are contiguous,
//! // and the columns are strided
//! let input_array = vec![ 1, 2, 3,
//! 						4, 5, 6];
//! 
//! // Treat our 6-element array as a 2D 3x2 array, and transpose it to a 2x3 array
//! let mut output_array = vec![0; 6];
//! transpose::transpose(&input_array, &mut output_array, 3, 2);
//!
//! // The rows have become the columns, and the columns have become the rows
//! let expected_array =  vec![ 1, 4,
//!								2, 5,
//!								3, 6];
//! assert_eq!(output_array, expected_array);
//!
//! // If we transpose our data again, we should get our original data back.
//! let mut final_array = vec![0; 6];
//! transpose::transpose(&output_array, &mut final_array, 2, 3);
//! assert_eq!(final_array, input_array);
//! ```

const BLOCK_SIZE: usize = 16;

// Transpose a subset of the array, from the input into the output. The idea is that by transposing one block at a time, we can be more cache-friendly
// SAFETY: Width * height must equal input.len() and output.len(), start_x + block_width must be <= width, start_y + block height must be <= height
unsafe fn transpose_block<T: Copy>(input: &[T], output: &mut [T], width: usize, height: usize, start_x: usize, start_y: usize, block_width: usize, block_height: usize) {
    for inner_x in 0..block_width {
        for inner_y in 0..block_height {
            let x = start_x + inner_x;
            let y = start_y + inner_y;

            let input_index = x + y * width;
            let output_index = y + x * height;

            *output.get_unchecked_mut(output_index) = *input.get_unchecked(input_index);
        }
    }
}

/// Transpose the input array into the output array. 
///
/// Given an input array of size input_width * input_height, representing flattened 2D data stored in row-major order,
/// transpose the rows and columns of that input array into the output array
/// ```
/// // row-major order: the rows of our 2D array are contiguous,
/// // and the columns are strided
/// let input_array = vec![ 1, 2, 3,
/// 						4, 5, 6];
/// 
/// // Treat our 6-element array as a 2D 3x2 array, and transpose it to a 2x3 array
/// let mut output_array = vec![0; 6];
/// transpose::transpose(&input_array, &mut output_array, 3, 2);
///
/// // The rows have become the columns, and the columns have become the rows
/// let expected_array =  vec![ 1, 4,
///								2, 5,
///								3, 6];
/// assert_eq!(output_array, expected_array);
///
/// // If we transpose it again, we should get our original data back.
/// let mut final_array = vec![0; 6];
/// transpose::transpose(&output_array, &mut final_array, 2, 3);
/// assert_eq!(final_array, input_array);
/// ```
///
/// # Panics
/// 
/// Panics if `input.len() != input_width * input_height` or if `output.len() != input_width * input_height`
pub fn transpose<T: Copy>(input: &[T], output: &mut [T], input_width: usize, input_height: usize) {
    assert_eq!(input_width*input_height, input.len());
    assert_eq!(input_width*input_height, output.len());

    let x_block_count = input_width / BLOCK_SIZE;
    let y_block_count = input_height / BLOCK_SIZE;

    let remainder_x = input_width - x_block_count * BLOCK_SIZE;
    let remainder_y = input_height - y_block_count * BLOCK_SIZE;

    for y_block in 0..y_block_count {
        for x_block in 0..x_block_count {
            unsafe {
                transpose_block(
                    input, output,
                    input_width, input_height,
                    x_block * BLOCK_SIZE, y_block * BLOCK_SIZE,
                    BLOCK_SIZE, BLOCK_SIZE,
                    );
            }
        }

        //if the input_width is not cleanly divisible by block_size, there are still a few columns that haven't been transposed
        if remainder_x > 0 {
            unsafe {
                transpose_block(
                    input, output, 
                    input_width, input_height, 
                    input_width - remainder_x, y_block * BLOCK_SIZE, 
                    remainder_x, BLOCK_SIZE);
            }
        }
    }

    //if the input_height is not cleanly divisible by BLOCK_SIZE, there are still a few rows that haven't been transposed
    if remainder_y > 0 {
        for x_block in 0..x_block_count {
            unsafe {
                transpose_block(
                    input, output,
                    input_width, input_height,
                    x_block * BLOCK_SIZE, input_height - remainder_y,
                    BLOCK_SIZE, remainder_y,
                    );
            }
        }

        //if the input_width is not cleanly divisible by block_size, there are still a few rows+columns that haven't been transposed
        if remainder_x > 0 {
            unsafe {
                transpose_block(
                    input, output,
                    input_width, input_height, 
                    input_width - remainder_x, input_height - remainder_y, 
                    remainder_x, remainder_y);
            }
        }
    } 
}


#[cfg(test)]
mod unit_tests {
    use super::*;

    fn gen_data(width: usize, height: usize) -> Vec<usize> {
        (0 .. width * height).map(|x| x + 1).collect()
    }

    #[test]
    fn test_transpose() {
        let sizes = [
        	0, 1, 2,
        	BLOCK_SIZE - 1, BLOCK_SIZE, BLOCK_SIZE + 1, 
        	BLOCK_SIZE * 4 - 1, BLOCK_SIZE * 5, BLOCK_SIZE * 4 + 1
        	];

        for &width in &sizes {
            for &height in &sizes {
                let input = gen_data(width, height);
                let mut output = vec![0; width * height];

                transpose(&input, &mut output, width, height);

                for x in 0..width {
                    for y in 0..height {
                        assert_eq!(input[x + y * width], output[y + x * height], "x = {}, y = {}", x, y);
                    }
                }
            }
        }
    }
}
