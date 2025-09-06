// Implementation adapted from Python version:
// https://github.com/commonsmachinery/blockhash-python/blob/e8b009d/blockhash.py
// Main site: http://blockhash.io
use image::{GenericImageView, Pixel};

use {BitSet, Image, HashBytes};

use std::cmp::Ordering;
use std::ops::AddAssign;
use std::mem;

const FLOAT_EQ_MARGIN: f32 = 0.001;

pub fn blockhash<I: Image, B: HashBytes>(img: &I, width: u32, height: u32) -> B {
    assert_eq!(width % 4, 0, "width must be multiple of 4");
    assert_eq!(height % 4, 0, "height must be multiple of 4");

    let (iwidth, iheight) = img.dimensions();

    // Skip the floating point math if it's unnecessary
    if iwidth % width == 0 && iheight % height == 0 {
        blockhash_fast(img, width, height)
    } else {
        blockhash_slow(img, width, height)
    }        
} 

macro_rules! gen_hash {
    ($imgty:ty, $valty:ty, $blocks: expr, $width:expr, $block_width:expr, $block_height:expr, $eq_fn:expr) => ({
        #[allow(deprecated)] // deprecated as of 0.22
        let channel_count = <<$imgty as GenericImageView>::Pixel as Pixel>::channel_count() as u32;

        let group_len = ($width * 4) as usize;

        let block_area = $block_width * $block_height;

        let cmp_factor = match channel_count {
            3 | 4 => 255u32 as $valty * 3u32 as $valty,
            2 | 1 => 255u32 as $valty,
            _ => panic!("Unrecognized channel count from Image: {}", channel_count),
        }  
            * block_area 
            / (2u32 as $valty);

        let medians: Vec<$valty> = $blocks.chunks(group_len).map(get_median).collect();

        BitSet::from_bools(
            $blocks.chunks(group_len).zip(medians)
            .flat_map(|(blocks, median)| 
                blocks.iter().map(move |&block| 
                    block > median ||
                        ($eq_fn(block, median) && median > cmp_factor)
                )
            )
        )
    })
}

fn block_adder<'a, T: AddAssign + 'a>(blocks: &'a mut [T], width: u32) -> impl FnMut(u32, u32, T) + 'a {
    move |x, y, add| (blocks[(y as usize) * (width as usize) + (x as usize)] += add)
}

fn blockhash_slow<I: Image, B: HashBytes>(img: &I, hwidth: u32, hheight: u32) -> B {
    let mut blocks = vec![0f32; (hwidth * hheight) as usize];

    let (iwidth, iheight) = img.dimensions();
    
    // Block dimensions, in pixels
    let (block_width, block_height) = (iwidth as f32 / hwidth as f32, iheight as f32 / hheight as f32);

    img.foreach_pixel8(|x, y, px| {
        let mut add_to_block = block_adder(&mut blocks, hwidth);

        let px_sum = sum_px(px) as f32;

        let (x, y) = (x as f32, y as f32);

        let block_x = x / block_width;
        let block_y = y / block_height;

        let x_mod = x + 1. % block_width;
        let y_mod = y + 1. % block_height;

        // terminology is mostly arbitrary as long as we're consistent
        // if `x` evenly divides `block_height`, this weight will be 0
        // so we don't double the sum as `block_top` will equal `block_bottom`
        let weight_left = x_mod.fract();
        let weight_right = 1. - weight_left;
        let weight_top = y_mod.fract();
        let weight_bottom = 1. - weight_top;

        let block_left = block_x.floor() as u32;
        let block_top = block_y.floor() as u32;

        let block_right = if x_mod.trunc() == 0. {
            block_x.ceil() as u32
        } else {
            block_left
        };

        let block_bottom = if y_mod.trunc() == 0. {
            block_y.ceil() as u32
        } else {
            block_top
        };

        add_to_block(block_left, block_top, px_sum * weight_left * weight_top);
        add_to_block(block_left, block_bottom, px_sum * weight_left * weight_bottom);
        add_to_block(block_right, block_top, px_sum * weight_right * weight_top);
        add_to_block(block_right, block_bottom, px_sum * weight_right * weight_bottom);
    });

    
    gen_hash!(I, f32, blocks, hwidth, block_width, block_height,
        |l: f32, r: f32| (l - r).abs() < FLOAT_EQ_MARGIN)
}

fn blockhash_fast<I: Image, B: HashBytes>(img: &I, hwidth: u32, hheight: u32) -> B {
    let mut blocks = vec![0u32; (hwidth * hheight) as usize];
    let (iwidth, iheight) = img.dimensions();

    let (block_width, block_height) = (iwidth / hwidth, iheight / hheight);

    img.foreach_pixel8(|x, y, px| {
        let mut add_to_block = block_adder(&mut blocks, hwidth);

        let px_sum = sum_px(px);

        let block_x = x / block_width;
        let block_y = y / block_height;

        add_to_block(block_x, block_y, px_sum);
    });

    gen_hash!(I, u32, blocks, hwidth, block_width, block_height, |l, r| l == r)
}

#[inline(always)]
fn sum_px(chans: &[u8]) -> u32 {
    // Branch prediction should eliminate the match after a few iterations
    match chans.len() {
        4 => if chans[3] == 0 { 255 * 3 } else { sum_px(&chans[..3]) },
        3 => chans.iter().map(|&x| x as u32).sum(),
        2 => if chans[1] == 0 { 255 } else { chans[0] as u32 },
        1 => chans[0] as u32,
        channels => panic!("Unsupported channel count in image: {}", channels),
    }
}

fn get_median<T: PartialOrd + Copy>(data: &[T]) -> T {
    let mut scratch = data.to_owned();
    let median = scratch.len() / 2;
    *qselect_inplace(&mut scratch, median)
}

const SORT_THRESH: usize = 8;

fn qselect_inplace<T: PartialOrd>(data: &mut [T], k: usize) -> &mut T {
    let len = data.len();

    assert!(k < len, "Called qselect_inplace with k = {} and data length: {}", k, len);

    if len < SORT_THRESH {
        data.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Less));
        return &mut data[k];
    }

    let pivot_idx = partition(data);

    if k == pivot_idx {
        &mut data[pivot_idx]
    } else if k < pivot_idx {
        qselect_inplace(&mut data[..pivot_idx], k)
    } else {
        qselect_inplace(&mut data[pivot_idx + 1..], k - pivot_idx - 1)
    }
}

fn partition<T: PartialOrd>(data: &mut [T]) -> usize {
    let len = data.len();

    let pivot_idx = {
        let first = (&data[0], 0);
        let mid = (&data[len / 2], len / 2);
        let last = (&data[len - 1], len - 1);

        median_of_3(&first, &mid, &last).1
    };

    data.swap(pivot_idx, len - 1);

    let mut curr = 0;

    for i in 0 .. len - 1 {
        if &data[i] < &data[len - 1] {
            data.swap(i, curr);
            curr += 1;
        }
    }

    data.swap(curr, len - 1);

    curr
}

fn median_of_3<T: PartialOrd>(mut x: T, mut y: T, mut z: T) -> T {
    if x > y {
        mem::swap(&mut x, &mut y);
    }

    if x > z {
        mem::swap(&mut x, &mut z);
    }

    if x > z {
        mem::swap(&mut y, &mut z);
    }

    y
}
