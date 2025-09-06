use rustdct::{DCTplanner, TransformType2And3};
use transpose::transpose;

use std::sync::Arc;

pub const SIZE_MULTIPLIER: u32 = 2;
pub const SIZE_MULTIPLIER_U: usize = SIZE_MULTIPLIER as usize;

pub struct DctCtxt {
    row_dct: Arc<dyn TransformType2And3<f32>>,
    col_dct: Arc<dyn TransformType2And3<f32>>,
    width: usize,
    height: usize,
}

impl DctCtxt {
    pub fn new(width: u32, height: u32) -> Self {
        let mut planner = DCTplanner::new();
        let width = width as usize * SIZE_MULTIPLIER_U;
        let height = height as usize * SIZE_MULTIPLIER_U;

        DctCtxt {
            row_dct: planner.plan_dct2(width),
            col_dct: planner.plan_dct2(height),
            width,
            height,
        }
    }

    pub fn width(&self) -> u32 {
        self.width as u32
    }

    pub fn height(&self) -> u32 {
        self.height as u32
    }

    /// Perform a 2D DCT on a 1D-packed vector with a given `width x height`.
    ///
    /// Assumes `packed_2d` is double-length for scratch space. Returns the vector truncated to
    /// `width * height`.
    ///
    /// ### Panics
    /// If `self.width * self.height * 2 != packed_2d.len()`
    pub fn dct_2d(&self, mut packed_2d: Vec<f32>) -> Vec<f32> {
        let Self { ref row_dct, ref col_dct, width, height } = *self;

        let trunc_len = width * height;
        assert_eq!(trunc_len * 2, packed_2d.len());

        {
            let (packed_2d, scratch) = packed_2d.split_at_mut(trunc_len);

            for (row_in, row_out) in packed_2d.chunks_mut(width)
                .zip(scratch.chunks_mut(width)) {
                row_dct.process_dct2(row_in, row_out);
            }

            transpose(scratch, packed_2d, width, height);

            for (row_in, row_out) in packed_2d.chunks_mut(height)
                .zip(scratch.chunks_mut(height)) {
                col_dct.process_dct2(row_in, row_out);
            }

            transpose(scratch, packed_2d, width, height);
        }

        packed_2d.truncate(trunc_len);
        packed_2d
    }

    pub fn crop_2d(&self, packed: Vec<f32>) -> Vec<f32> {
        crop_2d_dct(packed, self.width)
    }
}

/// Crop the values off a 1D-packed 2D DCT.
///
/// Returns `packed` truncated to the premultiplied size, as determined by `rowstride`
///
/// Generic for easier testing
fn crop_2d_dct<T: Copy>(mut packed: Vec<T>, rowstride: usize) -> Vec<T> {
    // assert that the rowstride was previously multiplied by SIZE_MULTIPLIER
    assert_eq!(rowstride % SIZE_MULTIPLIER_U, 0);
    assert!(rowstride / SIZE_MULTIPLIER_U > 0, "rowstride cannot be cropped: {}", rowstride);

    let new_rowstride = rowstride / SIZE_MULTIPLIER_U;

    for new_row in 0 .. packed.len() / (rowstride * SIZE_MULTIPLIER_U) {
        let (dest, src) = packed.split_at_mut(new_row * new_rowstride + rowstride);
        let dest_start = dest.len() - new_rowstride;
        let src_start = new_rowstride * new_row;
        let src_end = src_start + new_rowstride;
        dest[dest_start..].copy_from_slice(&src[src_start..src_end]);
    }

    let new_len = packed.len() / (SIZE_MULTIPLIER_U * SIZE_MULTIPLIER_U);
    packed.truncate(new_len);

    packed
}

#[test]
fn test_crop_2d_dct() {
    let packed: Vec<i32> = (0 .. 64).collect();
    assert_eq!(
        crop_2d_dct(packed.clone(), 8),
        [
            0, 1, 2, 3, // 4, 5, 6, 7
            8, 9, 10, 11, // 12, 13, 14, 15
            16, 17, 18, 19, // 20, 21, 22, 23,
            24, 25, 26, 27, // 28, 29, 30, 31,
            // 32 .. 64
        ]
    );
}

#[test]
fn test_transpose() {

}
