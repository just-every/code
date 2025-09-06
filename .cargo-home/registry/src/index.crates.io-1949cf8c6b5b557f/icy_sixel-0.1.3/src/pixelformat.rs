use crate::{PixelFormat, SixelError, SixelResult};

pub fn get_rgb(data: &[u8], pixelformat: PixelFormat, depth: usize) -> (u8, u8, u8) {
    let mut count = 0;
    let mut pixels: u32 = 0;
    while count < depth {
        pixels = data[count] as u32 | (pixels << 8);
        count += 1;
    }
    /*
        /* TODO: we should swap bytes (only necessary on LSByte first hardware?) */
    #if SWAP_BYTES
        if (depth == 2) {
            low    = pixels & 0xff;
            high   = (pixels >> 8) & 0xff;
            pixels = (low << 8) | high;
        }
    #endif*/
    let (r, g, b) = match pixelformat {
        PixelFormat::RGB555 => (
            ((pixels >> 10) & 0x1f) << 3,
            ((pixels >> 5) & 0x1f) << 3,
            ((pixels >> 0) & 0x1f) << 3,
        ),
        PixelFormat::RGB565 => (
            ((pixels >> 11) & 0x1f) << 3,
            ((pixels >> 5) & 0x3f) << 2,
            ((pixels >> 0) & 0x1f) << 3,
        ),
        PixelFormat::RGB888 => (
            (pixels >> 16) & 0xff,
            (pixels >> 8) & 0xff,
            (pixels >> 0) & 0xff,
        ),
        PixelFormat::BGR555 => (
            ((pixels >> 0) & 0x1f) << 3,
            ((pixels >> 5) & 0x1f) << 3,
            ((pixels >> 10) & 0x1f) << 3,
        ),
        PixelFormat::BGR565 => (
            ((pixels >> 0) & 0x1f) << 3,
            ((pixels >> 5) & 0x3f) << 2,
            ((pixels >> 11) & 0x1f) << 3,
        ),
        PixelFormat::BGR888 => (
            (pixels >> 0) & 0xff,
            (pixels >> 8) & 0xff,
            (pixels >> 16) & 0xff,
        ),
        PixelFormat::ARGB8888 => (
            (pixels >> 16) & 0xff,
            (pixels >> 8) & 0xff,
            (pixels >> 0) & 0xff,
        ),
        PixelFormat::RGBA8888 => (
            (pixels >> 24) & 0xff,
            (pixels >> 16) & 0xff,
            (pixels >> 8) & 0xff,
        ),
        PixelFormat::ABGR8888 => (
            (pixels >> 0) & 0xff,
            (pixels >> 8) & 0xff,
            (pixels >> 16) & 0xff,
        ),
        PixelFormat::BGRA8888 => (
            (pixels >> 8) & 0xff,
            (pixels >> 16) & 0xff,
            (pixels >> 24) & 0xff,
        ),
        PixelFormat::G8 | PixelFormat::AG88 => (pixels & 0xff, pixels & 0xff, pixels & 0xff),
        PixelFormat::GA88 => (
            (pixels >> 8) & 0xff,
            (pixels >> 8) & 0xff,
            (pixels >> 8) & 0xff,
        ),
        _ => (0, 0, 0),
    };
    (r as u8, g as u8, b as u8)
}

pub fn sixel_helper_compute_depth(pixelformat: PixelFormat) -> i32 {
    match pixelformat {
        PixelFormat::ARGB8888
        | PixelFormat::RGBA8888
        | PixelFormat::ABGR8888
        | PixelFormat::BGRA8888 => 4,

        PixelFormat::RGB888 | PixelFormat::BGR888 => 3,

        PixelFormat::RGB555
        | PixelFormat::RGB565
        | PixelFormat::BGR555
        | PixelFormat::BGR565
        | PixelFormat::AG88
        | PixelFormat::GA88 => 2,

        PixelFormat::G1
        | PixelFormat::G2
        | PixelFormat::G4
        | PixelFormat::G8
        | PixelFormat::PAL1
        | PixelFormat::PAL2
        | PixelFormat::PAL4
        | PixelFormat::PAL8 => 1,
    }
}

pub fn expand_rgb(
    dst: &mut [u8],
    src: &[u8],
    width: i32,
    height: i32,
    pixelformat: PixelFormat,
    depth: usize,
) {
    for y in 0..height {
        for x in 0..width {
            let src_offset = depth * (y * width + x) as usize;
            let dst_offset: usize = 3 * (y * width + x) as usize;
            let (r, g, b) = get_rgb(&src[src_offset..], pixelformat, depth);

            dst[dst_offset + 0] = r;
            dst[dst_offset + 1] = g;
            dst[dst_offset + 2] = b;
        }
    }
}

pub fn expand_palette(
    dst: &mut [u8],
    src: &[u8],
    width: i32,
    height: i32,
    pixelformat: PixelFormat,
) -> SixelResult<()> {
    let bpp = match pixelformat {
        PixelFormat::PAL1 | PixelFormat::G1 => 1,

        PixelFormat::PAL2 | PixelFormat::G2 => 2,

        PixelFormat::PAL4 | PixelFormat::G4 => 4,

        PixelFormat::PAL8 | PixelFormat::G8 => {
            dst[..((width * height) as usize)].copy_from_slice(&src[..((width * height) as usize)]);
            return Ok(());
        }

        //          sixel_helper_set_additional_message(    "expand_palette: invalid pixelformat.");
        _ => return Err(Box::new(SixelError::BadArgument)),
    };
    let mut dst_offset = 0;
    let mut src_offset = 0;

    let max_x = width * bpp / 8;
    for _y in 0..height {
        for _x in 0..max_x {
            for i in 0..8 / bpp {
                let shift = ((8 / bpp) - 1 - i) * (bpp & (1 << (bpp - 1)));
                dst[dst_offset] = ((src[src_offset] as i32) >> shift) as u8;
                dst_offset += 1;
            }
            src_offset += 1;
        }

        let x = width - max_x * 8 / bpp;
        if x > 0 {
            for i in 0..x {
                dst[dst_offset] = src[src_offset] >> ((8 - (i + 1) * bpp) & ((1 << bpp) - 1));
                dst_offset += 1;
            }
            src_offset += 1;
        }
    }
    Ok(())
}

/// returns dst_pixelformat: PixelFormat,
pub fn sixel_helper_normalize_pixelformat(
    dst: &mut [u8],
    src: &[u8],
    src_pixelformat: PixelFormat,
    width: i32,
    height: i32,
) -> SixelResult<PixelFormat> /* height of source image */ {
    match src_pixelformat {
        PixelFormat::G8 => {
            expand_rgb(dst, src, width, height, src_pixelformat, 1);
            Ok(PixelFormat::RGB888)
        }

        PixelFormat::RGB565
        | PixelFormat::RGB555
        | PixelFormat::BGR565
        | PixelFormat::BGR555
        | PixelFormat::GA88
        | PixelFormat::AG88 => {
            expand_rgb(dst, src, width, height, src_pixelformat, 2);
            Ok(PixelFormat::RGB888)
        }

        PixelFormat::RGB888 | PixelFormat::BGR888 => {
            expand_rgb(dst, src, width, height, src_pixelformat, 3);
            Ok(PixelFormat::RGB888)
        }

        PixelFormat::RGBA8888
        | PixelFormat::ARGB8888
        | PixelFormat::BGRA8888
        | PixelFormat::ABGR8888 => {
            expand_rgb(dst, src, width, height, src_pixelformat, 4);
            Ok(PixelFormat::RGB888)
        }

        PixelFormat::PAL1 | PixelFormat::PAL2 | PixelFormat::PAL4 => {
            expand_palette(dst, src, width, height, src_pixelformat)?;
            Ok(PixelFormat::PAL8)
        }

        PixelFormat::G1 | PixelFormat::G2 | PixelFormat::G4 => {
            expand_palette(dst, src, width, height, src_pixelformat)?;
            Ok(PixelFormat::G8)
        }
        PixelFormat::PAL8 => {
            dst[..((width * height) as usize)].copy_from_slice(&src[..((width * height) as usize)]);
            Ok(src_pixelformat)
        }
    }
}

/* emacs Local Variables:      */
/* emacs mode: c               */
/* emacs tab-width: 4          */
/* emacs indent-tabs-mode: nil */
/* emacs c-basic-offset: 4     */
/* emacs End:                  */
/* vim: set expandtab ts=4 sts=4 sw=4 : */
/* EOF */

/*
 * Copyright (c) 2014-2019 Hayaki Saito
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy of
 * this software and associated documentation files (the "Software"), to deal in
 * the Software without restriction, including without limitation the rights to
 * use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
 * the Software, and to permit persons to whom the Software is furnished to do so,
 * subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
 * FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
 * COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
 * IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
 * CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */
