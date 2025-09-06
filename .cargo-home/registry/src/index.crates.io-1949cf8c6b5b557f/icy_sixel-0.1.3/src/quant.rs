#![allow(clippy::erasing_op)]
/*****************************************************************************
 *
 * quantization
 *
 *****************************************************************************/

use std::cmp::Ordering;
use std::vec;

#[derive(Clone)]
struct bbox {
    pub ind: i32,
    pub colors: i32,
    pub sum: i32,
}

/*
typedef struct box* boxVector;

typedef unsigned long sample;
typedef sample * tuple;

struct tupleint {
    /* An ordered pair of a tuple value and an integer, such as you
       would find in a tuple table or tuple hash.
       Note that this is a variable length structure.
    */
    unsigned int value;
    sample tuple[1];
    /* This is actually a variable size array -- its size is the
       depth of the tuple in question.  Some compilers do not let us
       declare a variable length array.
    */
};
typedef struct tupleint ** tupletable;

typedef struct {
    unsigned int size;
    tupletable table;
} tupletable2;

static unsigned int compareplanePlane;

*/
/* This is a parameter to compareplane().  We use this global variable
   so that compareplane() can be called by qsort(), to compare two
   tuples.  qsort() doesn't pass any arguments except the two tuples.
*/
/*
static int
compareplane(const void * const arg1,
             const void * const arg2)
{
    int lhs, rhs;

    typedef const struct tupleint * const * const sortarg;
    sortarg comparandPP  = (sortarg) arg1;
    sortarg comparatorPP = (sortarg) arg2;
    lhs = (int)(*comparandPP)->tuple[compareplanePlane];
    rhs = (int)(*comparatorPP)->tuple[compareplanePlane];

    return lhs - rhs;
}*/

fn sumcompare(b1: &bbox, b2: &bbox) -> Ordering {
    b2.sum.cmp(&b1.sum)
}

/*
 ** Here is the fun part, the median-cut colormap generator.  This is based
 ** on Paul Heckbert's paper "Color Image Quantization for Frame Buffer
 ** Display", SIGGRAPH '82 Proceedings, page 297.
 */

pub fn newColorMap(newcolors: i32, depth: i32) -> HashMap<i32, Tuple> {
    let mut colormap = HashMap::new();
    for i in 0..newcolors {
        colormap.insert(
            i,
            Tuple {
                value: 0,
                tuple: vec![0; depth as usize],
            },
        );
    }
    colormap
}

fn newBoxVector(colors: i32, sum: i32, newcolors: i32) -> Vec<bbox> {
    let mut result = vec![
        bbox {
            ind: 0,
            colors: 0,
            sum: 0
        };
        newcolors as usize
    ];

    /* Set up the initial box. */
    result[0].ind = 0;
    result[0].colors = colors;
    result[0].sum = sum;

    result
}

pub fn findBoxBoundaries(
    colorfreqtable: &mut HashMap<i32, Tuple>,
    depth: i32,
    boxStart: i32,
    boxSize: i32,
    minval: &mut [i32],
    maxval: &mut [i32],
) {
    /*----------------------------------------------------------------------------
      Go through the box finding the minimum and maximum of each
      component - the boundaries of the box.
    -----------------------------------------------------------------------------*/

    for plane in 0..depth {
        minval[plane as usize] = colorfreqtable.get(&(boxStart)).unwrap().tuple[plane as usize];
        maxval[plane as usize] = minval[plane as usize];
    }
    for i in 1..boxSize {
        for plane in 0..depth {
            let v = colorfreqtable.get(&(boxStart + i)).unwrap().tuple[plane as usize];
            minval[plane as usize] = minval[plane as usize].min(v);
            maxval[plane as usize] = maxval[plane as usize].max(v);
        }
    }
}

pub fn largestByNorm(minval: &[i32], maxval: &[i32], depth: i32) -> i32 {
    let mut largestSpreadSoFar = 0;
    let mut largestDimension = 0;
    for plane in 0..depth as usize {
        let spread = maxval[plane] - minval[plane];
        if spread > largestSpreadSoFar {
            largestDimension = plane;
            largestSpreadSoFar = spread;
        }
    }
    largestDimension as i32
}

pub fn largestByLuminosity(minval: &[i32], maxval: &[i32], depth: i32) -> i32 {
    /*----------------------------------------------------------------------------
       This subroutine presumes that the tuple type is either
       BLACKANDWHITE, GRAYSCALE, or RGB (which implies pamP->depth is 1 or 3).
       To save time, we don't actually check it.
    -----------------------------------------------------------------------------*/
    let retval;

    let lumin_factor = [0.2989, 0.5866, 0.1145];

    if depth == 1 {
        retval = 0;
    } else {
        /* An RGB tuple */
        let mut largestSpreadSoFar = 0.0;
        let mut largestDimension = 0;

        for plane in 0..3 {
            let spread = lumin_factor[plane] * (maxval[plane] - minval[plane]) as f32;
            if spread > largestSpreadSoFar {
                largestDimension = plane;
                largestSpreadSoFar = spread;
            }
        }
        retval = largestDimension;
    }
    retval as i32
}

pub fn centerBox(
    boxStart: i32,
    boxSize: i32,
    colorfreqtable: &mut HashMap<i32, Tuple>,
    depth: i32,
    newTuple: &mut [i32],
) {
    for plane in 0..depth {
        let mut maxval = colorfreqtable.get(&(boxStart)).unwrap().tuple[plane as usize];
        let mut minval = maxval;

        for i in 1..boxSize {
            let v = colorfreqtable.get(&(boxStart + i)).unwrap().tuple[plane as usize];
            minval = minval.min(v);
            maxval = maxval.max(v);
        }
        newTuple[plane as usize] = (minval + maxval) / 2;
    }
}

pub fn averageColors(
    boxStart: i32,
    boxSize: i32,
    colorfreqtable: &mut HashMap<i32, Tuple>,
    depth: i32,
    newTuple: &mut [i32],
) {
    for plane in 0..depth {
        let mut sum = 0;

        for i in 0..boxSize {
            sum += colorfreqtable.get(&(boxStart + i)).unwrap().tuple[plane as usize];
        }

        newTuple[plane as usize] = sum / boxSize;
    }
}

pub fn averagePixels(
    boxStart: i32,
    boxSize: i32,
    colorfreqtable: &mut HashMap<i32, Tuple>,
    depth: i32,
    newTuple: &mut [i32],
) {
    /* Number of tuples represented by the box */
    /* Count the tuples in question */
    let mut n = 0; /* initial value */
    for i in 0..boxSize {
        n += colorfreqtable.get(&(boxStart + i)).unwrap().value;
    }

    for plane in 0..depth {
        let mut sum = 0;
        for i in 0..boxSize {
            sum += colorfreqtable.get(&(boxStart + i)).unwrap().tuple[plane as usize]
                * colorfreqtable.get(&(boxStart + i)).unwrap().value;
        }
        newTuple[plane as usize] = sum / n;
    }
}

fn colormapFromBv(
    newcolors: i32,
    bv: &[bbox],
    boxes: i32,
    colorfreqtable: &mut HashMap<i32, Tuple>,
    depth: i32,
    methodForRep: MethodForRep,
) -> HashMap<i32, Tuple> {
    /*
     ** Ok, we've got enough boxes.  Now choose a representative color for
     ** each box.  There are a number of possible ways to make this choice.
     ** One would be to choose the center of the box; this ignores any structure
     ** within the boxes.  Another method would be to average all the colors in
     ** the box - this is the method specified in Heckbert's paper.  A third
     ** method is to average all the pixels in the box.
     */
    let mut colormap = newColorMap(newcolors, depth);

    for bi in 0..boxes {
        match methodForRep {
            MethodForRep::CenterBox => {
                centerBox(
                    bv[bi as usize].ind,
                    bv[bi as usize].colors,
                    colorfreqtable,
                    depth,
                    &mut colormap.get_mut(&bi).unwrap().tuple,
                );
            }
            MethodForRep::AverageColors => {
                averageColors(
                    bv[bi as usize].ind,
                    bv[bi as usize].colors,
                    colorfreqtable,
                    depth,
                    &mut colormap.get_mut(&bi).unwrap().tuple,
                );
            }
            MethodForRep::Auto | MethodForRep::Pixels => {
                averagePixels(
                    bv[bi as usize].ind,
                    bv[bi as usize].colors,
                    colorfreqtable,
                    depth,
                    &mut colormap.get_mut(&bi).unwrap().tuple,
                );
            }
        }
    }
    colormap
}

fn splitBox(
    bv: &mut [bbox],
    boxesP: &mut i32,
    bi: usize,
    colorfreqtable: &mut HashMap<i32, Tuple>,
    depth: i32,
    methodForLargest: MethodForLargest,
) -> SixelResult<()> {
    /*----------------------------------------------------------------------------
       Split Box 'bi' in the box vector bv (so that bv contains one more box
       than it did as input).  Split it so that each new box represents about
       half of the pixels in the distribution given by 'colorfreqtable' for
       the colors in the original box, but with distinct colors in each of the
       two new boxes.

       Assume the box contains at least two colors.
    -----------------------------------------------------------------------------*/
    let boxStart = bv[bi].ind;
    let boxSize = bv[bi].colors;
    let sm = bv[bi].sum;

    let max_depth = 16;
    let mut minval = vec![0; max_depth];
    let mut maxval = vec![0; max_depth];

    /* assert(max_depth >= depth); */

    findBoxBoundaries(
        colorfreqtable,
        depth,
        boxStart,
        boxSize,
        &mut minval,
        &mut maxval,
    );

    /* Find the largest dimension, and sort by that component.  I have
       included two methods for determining the "largest" dimension;
       first by simply comparing the range in RGB space, and second by
       transforming into luminosities before the comparison.
    */
    let _largestDimension = match methodForLargest {
        MethodForLargest::Auto | MethodForLargest::Norm => largestByNorm(&minval, &maxval, depth),
        MethodForLargest::Lum => largestByLuminosity(&minval, &maxval, depth),
    };

    /* TODO: I think this sort should go after creating a box,
       not before splitting.  Because you need the sort to use
       the SIXEL_REP_CENTER_BOX method of choosing a color to
       represent the final boxes
    */

    /* Set the gross global variable 'compareplanePlane' as a
       parameter to compareplane(), which is called by qsort().
    */

    /* Sholdn't be needed - I use a stupid hasmap - should be refactored.
    compareplanePlane = largestDimension;
    qsort((char*) &colorfreqtable.table[boxStart], boxSize,
          sizeof(colorfreqtable.table[boxStart]),
          compareplane);*/

    /* Now find the median based on the counts, so that about half
    the pixels (not colors, pixels) are in each subdivision.  */
    let mut lowersum = colorfreqtable.get(&boxStart).unwrap().value; /* initial value */
    let mut i = 1;
    while i < boxSize - 1 && lowersum < sm / 2 {
        lowersum += colorfreqtable.get(&(boxStart + i)).unwrap().value;
        i += 1;
    }
    let medianIndex = i;
    /* Split the box, and sort to bring the biggest boxes to the top.  */

    bv[bi].colors = medianIndex;
    bv[bi].sum = lowersum;
    bv[*boxesP as usize].ind = boxStart + medianIndex;
    bv[*boxesP as usize].colors = boxSize - medianIndex;
    bv[*boxesP as usize].sum = sm - lowersum;
    (*boxesP) += 1;

    bv[0..*boxesP as usize].sort_by(sumcompare);
    Ok(())
}

pub fn mediancut(
    colorfreqtable: &mut HashMap<i32, Tuple>,
    depth: i32,
    newcolors: i32,
    methodForLargest: MethodForLargest,
    methodForRep: MethodForRep,
    colormapP: &mut HashMap<i32, Tuple>,
) -> SixelResult<()> {
    /*----------------------------------------------------------------------------
       Compute a set of only 'newcolors' colors that best represent an
       image whose pixels are summarized by the histogram
       'colorfreqtable'.  Each tuple in that table has depth 'depth'.
       colorfreqtable.table[i] tells the number of pixels in the subject image
       have a particular color.

       As a side effect, sort 'colorfreqtable'.
    -----------------------------------------------------------------------------*/
    let mut sum = 0;

    for i in 0..colorfreqtable.len() {
        sum += colorfreqtable.get(&(i as i32)).unwrap().value;
    }

    /* There is at least one box that contains at least 2 colors; ergo,
    there is more splitting we can do.  */
    let mut bv = newBoxVector(colorfreqtable.len() as i32, sum, newcolors);
    let mut boxes = 1;
    let mut multicolorBoxesExist = colorfreqtable.len() > 1;

    /* Main loop: split boxes until we have enough. */
    while boxes < newcolors && multicolorBoxesExist {
        /* Find the first splittable box. */
        let mut bi = 0;
        while bi < boxes && bv[bi as usize].colors < 2 {
            bi += 1;
        }

        if bi >= boxes {
            multicolorBoxesExist = false;
        } else {
            splitBox(
                &mut bv,
                &mut boxes,
                bi as usize,
                colorfreqtable,
                depth,
                methodForLargest,
            )?;
        }
    }
    *colormapP = colormapFromBv(newcolors, &bv, boxes, colorfreqtable, depth, methodForRep);

    Ok(())
}

pub fn computeHash(data: &[u8], i: usize, depth: i32) -> i32 {
    let mut hash = 0;
    for n in 0..depth {
        hash |= (data[i + depth as usize - 1 - n as usize] as i32 >> 3) << (n * 5);
    }
    hash
}

#[derive(Clone)]
pub struct Tuple {
    pub value: i32,
    pub tuple: Vec<i32>,
}

pub fn computeHistogram(
    data: &[u8],
    length: i32,
    depth: i32,
    qualityMode: Quality,
) -> SixelResult<HashMap<i32, Tuple>> {
    let (max_sample, mut step) = match qualityMode {
        Quality::LOW => (18383, length / depth / 18383 * depth),
        Quality::HIGH => (18383, length / depth / 18383 * depth),
        Quality::AUTO | Quality::HIGHCOLOR | Quality::FULL => {
            (4003079, length / depth / 4003079 * depth)
        }
    };

    if length < max_sample * depth {
        step = 6 * depth;
    }

    if step <= 0 {
        step = depth;
    }

    let mut histogram = vec![0; 1 << (depth * 5)];

    let mut memory = vec![0; 1 << (depth * 5)];
    let mut it = 0;
    let mut refe = 0;
    let _refmap = 0;

    let mut i = 0;
    while i < length {
        let bucket_index = computeHash(data, i as usize, 3) as usize;
        if histogram[bucket_index] == 0 {
            memory[refe] = bucket_index;
            refe += 1;
        }
        if histogram[bucket_index] < (1 << (2 * 8)) - 1 {
            histogram[bucket_index] += 1;
        }

        i += step;
    }
    let mut colorfreqtable = HashMap::new();

    for i in 0..refe {
        if histogram[memory[i]] > 0 {
            let mut tuple: Vec<i32> = vec![0; depth as usize];
            for n in 0..depth {
                tuple[(depth - 1 - n) as usize] = ((memory[it] >> (n * 5) & 0x1f) << 3) as i32;
            }
            colorfreqtable.insert(
                i as i32,
                Tuple {
                    value: histogram[memory[i]],
                    tuple,
                },
            );
        }
        it += 1;
    }
    Ok(colorfreqtable)
}

#[allow(clippy::too_many_arguments)]
pub fn computeColorMapFromInput(
    data: &[u8],
    length: i32,
    depth: i32,
    reqColors: i32,
    methodForLargest: MethodForLargest,
    methodForRep: MethodForRep,
    qualityMode: Quality,
    colormapP: &mut HashMap<i32, Tuple>,
    origcolors: &mut i32,
) -> SixelResult<()> {
    /*----------------------------------------------------------------------------
       Produce a colormap containing the best colors to represent the
       image stream in file 'ifP'.  Figure it out using the median cut
       technique.

       The colormap will have 'reqcolors' or fewer colors in it, unless
       'allcolors' is true, in which case it will have all the colors that
       are in the input.

       The colormap has the same maxval as the input.

       Put the colormap in newly allocated storage as a tupletable2
       and return its address as *colormapP.  Return the number of colors in
       it as *colorsP and its maxval as *colormapMaxvalP.

       Return the characteristics of the input file as
       *formatP and *freqPamP.  (This information is not really
       relevant to our colormap mission; just a fringe benefit).
    -----------------------------------------------------------------------------*/

    let mut colorfreqtable = computeHistogram(data, length, depth, qualityMode)?;
    *origcolors = colorfreqtable.len() as i32;

    if colorfreqtable.len() as i32 <= reqColors {
        /*
        for i in colorfreqtable.len() as i32..=reqColors {
            let mut tuple: Vec<i32> = vec![0; depth as usize];
            for n in 0..depth {
                tuple[n as usize] = (i * depth) + n;
            }
            colorfreqtable.insert(i, Tuple { value: i, tuple });
        }*/

        for i in 0..colorfreqtable.len() as i32 {
            colormapP.insert(i, colorfreqtable.get(&i).unwrap().clone());
        }
    } else {
        mediancut(
            &mut colorfreqtable,
            depth,
            reqColors,
            methodForLargest,
            methodForRep,
            colormapP,
        )?;
    }
    Ok(())
}

/* diffuse error energy to surround pixels */
pub fn error_diffuse(
    data: &mut [u8],  /* base address of pixel buffer */
    pos: i32,         /* address of the destination pixel */
    depth: i32,       /* color depth in bytes */
    error: i32,       /* error energy */
    numerator: i32,   /* numerator of diffusion coefficient */
    denominator: i32, /* denominator of diffusion coefficient */
) {
    let offset = (pos * depth) as usize;

    let mut c = data[offset] as i32 + error * numerator / denominator;
    if c < 0 {
        c = 0;
    }
    if c >= 1 << 8 {
        c = (1 << 8) - 1;
    }
    data[offset] = c as u8;
}

pub fn diffuse_none(
    _data: &mut [u8],
    _width: i32,
    _height: i32,
    _x: i32,
    _y: i32,
    _depth: i32,
    _error: i32,
) {
}

pub fn diffuse_fs(
    data: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    depth: i32,
    error: i32,
) {
    let pos = y * width + x;

    /* Floyd Steinberg Method
     *          curr    7/16
     *  3/16    5/48    1/16
     */
    if x < width - 1 && y < height - 1 {
        /* add error to the right cell */
        error_diffuse(data, pos + width * 0 + 1, depth, error, 7, 16);
        /* add error to the left-bottom cell */
        error_diffuse(data, pos + width * 1 - 1, depth, error, 3, 16);
        /* add error to the bottom cell */
        error_diffuse(data, pos + width * 1 + 0, depth, error, 5, 16);
        /* add error to the right-bottom cell */
        error_diffuse(data, pos + width * 1 + 1, depth, error, 1, 16);
    }
}

pub fn diffuse_atkinson(
    data: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    depth: i32,
    error: i32,
) {
    let pos = y * width + x;

    /* Atkinson's Method
     *          curr    1/8    1/8
     *   1/8     1/8    1/8
     *           1/8
     */
    if y < height - 2 {
        /* add error to the right cell */
        error_diffuse(data, pos + width * 0 + 1, depth, error, 1, 8);
        /* add error to the 2th right cell */
        error_diffuse(data, pos + width * 0 + 2, depth, error, 1, 8);
        /* add error to the left-bottom cell */
        error_diffuse(data, pos + width * 1 - 1, depth, error, 1, 8);
        /* add error to the bottom cell */
        error_diffuse(data, pos + width * 1 + 0, depth, error, 1, 8);
        /* add error to the right-bottom cell */
        error_diffuse(data, pos + width * 1 + 1, depth, error, 1, 8);
        /* add error to the 2th bottom cell */
        error_diffuse(data, pos + width * 2 + 0, depth, error, 1, 8);
    }
}

pub fn diffuse_jajuni(
    data: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    depth: i32,
    error: i32,
) {
    let pos = y * width + x;

    /* Jarvis, Judice & Ninke Method
     *                  curr    7/48    5/48
     *  3/48    5/48    7/48    5/48    3/48
     *  1/48    3/48    5/48    3/48    1/48
     */
    if pos < (height - 2) * width - 2 {
        error_diffuse(data, pos + width * 0 + 1, depth, error, 7, 48);
        error_diffuse(data, pos + width * 0 + 2, depth, error, 5, 48);
        error_diffuse(data, pos + width * 1 - 2, depth, error, 3, 48);
        error_diffuse(data, pos + width * 1 - 1, depth, error, 5, 48);
        error_diffuse(data, pos + width * 1 + 0, depth, error, 7, 48);
        error_diffuse(data, pos + width * 1 + 1, depth, error, 5, 48);
        error_diffuse(data, pos + width * 1 + 2, depth, error, 3, 48);
        error_diffuse(data, pos + width * 2 - 2, depth, error, 1, 48);
        error_diffuse(data, pos + width * 2 - 1, depth, error, 3, 48);
        error_diffuse(data, pos + width * 2 + 0, depth, error, 5, 48);
        error_diffuse(data, pos + width * 2 + 1, depth, error, 3, 48);
        error_diffuse(data, pos + width * 2 + 2, depth, error, 1, 48);
    }
}

pub fn diffuse_stucki(
    data: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    depth: i32,
    error: i32,
) {
    let pos = y * width + x;

    /* Stucki's Method
     *                  curr    8/48    4/48
     *  2/48    4/48    8/48    4/48    2/48
     *  1/48    2/48    4/48    2/48    1/48
     */
    if pos < (height - 2) * width - 2 {
        error_diffuse(data, pos + width * 0 + 1, depth, error, 1, 6);
        error_diffuse(data, pos + width * 0 + 2, depth, error, 1, 12);
        error_diffuse(data, pos + width * 1 - 2, depth, error, 1, 24);
        error_diffuse(data, pos + width * 1 - 1, depth, error, 1, 12);
        error_diffuse(data, pos + width * 1 + 0, depth, error, 1, 6);
        error_diffuse(data, pos + width * 1 + 1, depth, error, 1, 12);
        error_diffuse(data, pos + width * 1 + 2, depth, error, 1, 24);
        error_diffuse(data, pos + width * 2 - 2, depth, error, 1, 48);
        error_diffuse(data, pos + width * 2 - 1, depth, error, 1, 24);
        error_diffuse(data, pos + width * 2 + 0, depth, error, 1, 12);
        error_diffuse(data, pos + width * 2 + 1, depth, error, 1, 24);
        error_diffuse(data, pos + width * 2 + 2, depth, error, 1, 48);
    }
}

pub fn diffuse_burkes(
    data: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    depth: i32,
    error: i32,
) {
    let pos = y * width + x;

    /* Burkes' Method
     *                  curr    4/16    2/16
     *  1/16    2/16    4/16    2/16    1/16
     */
    if pos < (height - 1) * width - 2 {
        error_diffuse(data, pos + width * 0 + 1, depth, error, 1, 4);
        error_diffuse(data, pos + width * 0 + 2, depth, error, 1, 8);
        error_diffuse(data, pos + width * 1 - 2, depth, error, 1, 16);
        error_diffuse(data, pos + width * 1 - 1, depth, error, 1, 8);
        error_diffuse(data, pos + width * 1 + 0, depth, error, 1, 4);
        error_diffuse(data, pos + width * 1 + 1, depth, error, 1, 8);
        error_diffuse(data, pos + width * 1 + 2, depth, error, 1, 16);
    }
}

pub fn mask_a(x: i32, y: i32, c: i32) -> f32 {
    ((((x + c * 67) + y * 236) * 119) & 255) as f32 / 128.0 - 1.0
}

pub fn mask_x(x: i32, y: i32, c: i32) -> f32 {
    ((((x + c * 29) ^ (y * 149)) * 1234) & 511) as f32 / 256.0 - 1.0
}

use std::collections::HashMap;

use crate::{
    pixelformat::sixel_helper_compute_depth, MethodForLargest, PixelFormat, Quality, SixelResult,
};
use crate::{DiffusionMethod, MethodForRep, SixelError};

/* lookup closest color from palette with "normal" strategy */
pub fn lookup_normal(
    pixel: &[u8],
    depth: i32,
    palette: &[u8],
    reqcolor: i32,
    _cachetable: &mut [u16],
    complexion: i32,
) -> i32 {
    let mut result = -1;
    let mut diff = i32::MAX;

    /* don't use cachetable in 'normal' strategy */

    for i in 0..reqcolor {
        let mut distant = 0;
        let mut r = pixel[0] as i32 - palette[(i * depth + 0) as usize] as i32;
        distant += r * r * complexion;
        for n in 1..depth {
            r = pixel[n as usize] as i32 - palette[(i * depth + n) as usize] as i32;
            distant += r * r;
        }
        if distant < diff {
            diff = distant;
            result = i;
        }
    }

    result
}

/* lookup closest color from palette with "fast" strategy */
pub fn lookup_fast(
    pixel: &[u8],
    _depth: i32,
    palette: &[u8],
    reqcolor: i32,
    cachetable: &mut [u16],
    complexion: i32,
) -> i32 {
    let mut result: i32 = -1;
    let mut diff = i32::MAX;
    let hash = computeHash(pixel, 0, 3);

    let cache = cachetable[hash as usize];
    if cache != 0 {
        /* fast lookup */
        return cache as i32 - 1;
    }
    /* collision */
    for i in 0..reqcolor {
        /*          distant = 0;
         #if 0
                for (n = 0; n < 3; ++n) {
                    r = pixel[n] - palette[i * 3 + n];
                    distant += r * r;
                }
        #elif 1*/
        /* complexion correction */
        let i = i as usize;
        let distant = (pixel[0] as i32 - palette[i * 3 + 0] as i32)
            * (pixel[0] as i32 - palette[i * 3 + 0] as i32)
            * complexion
            + (pixel[1] as i32 - palette[i * 3 + 1] as i32)
                * (pixel[1] as i32 - palette[i * 3 + 1] as i32)
            + (pixel[2] as i32 - palette[i * 3 + 2] as i32)
                * (pixel[2] as i32 - palette[i * 3 + 2] as i32);
        //  #endif
        if distant < diff {
            diff = distant;
            result = i as i32;
        }
    }
    cachetable[hash as usize] = (result + 1) as u16;

    result
}

pub fn lookup_mono_darkbg(
    pixel: &[u8],
    depth: i32,
    _palette: &[u8],
    reqcolor: i32,
    _cachetable: &mut [u16],
    _complexion: i32,
) -> i32 {
    let mut distant = 0;
    for n in 0..depth {
        distant += pixel[n as usize] as i32;
    }
    if distant >= 128 * reqcolor {
        1
    } else {
        0
    }
}

pub fn lookup_mono_lightbg(
    pixel: &[u8],
    depth: i32,
    _palette: &[u8],
    reqcolor: i32,
    _cachetable: &mut [u16],
    _complexion: i32,
) -> i32 {
    let mut distant = 0;
    for n in 0..depth {
        distant += pixel[n as usize] as i32;
    }
    if distant < 128 * reqcolor {
        1
    } else {
        0
    }
}

/* choose colors using median-cut method */
#[allow(clippy::too_many_arguments)]
pub fn sixel_quant_make_palette(
    data: &[u8],
    length: i32,
    pixelformat: PixelFormat,
    reqcolors: i32,
    ncolors: &mut i32,
    origcolors: &mut i32,
    methodForLargest: MethodForLargest,
    methodForRep: MethodForRep,
    qualityMode: Quality,
) -> SixelResult<Vec<u8>> {
    let result_depth = sixel_helper_compute_depth(pixelformat);
    /*if (result_depth <= 0) {
        *result = NULL;
        goto end;
    }*/

    let depth = result_depth as usize;
    let mut colormap = HashMap::new();
    let _ = computeColorMapFromInput(
        data,
        length,
        depth as i32,
        reqcolors,
        methodForLargest,
        methodForRep,
        qualityMode,
        &mut colormap,
        origcolors,
    );
    *ncolors = colormap.len() as i32;
    let mut result = vec![0; colormap.len() * depth as usize];
    for i in 0..colormap.len() {
        for n in 0..depth {
            result[i * depth + n] = colormap.get(&(i as i32)).unwrap().tuple[n] as u8;
        }
    }
    Ok(result)
}

/* apply color palette into specified pixel buffers */
#[allow(clippy::too_many_arguments)]
pub fn sixel_quant_apply_palette(
    result: &mut [u8],
    data: &mut [u8],
    width: i32,
    height: i32,
    depth: i32,
    palette: &mut Vec<u8>,
    reqcolor: i32,
    methodForDiffuse: DiffusionMethod,
    foptimize: bool,
    foptimize_palette: bool,
    complexion: i32,
    cachetable: Option<&mut [u16]>,
) -> SixelResult<i32> {
    let mut ncolors: i32;
    /* check bad reqcolor */
    if reqcolor < 1 {
        /*
                sixel_helper_set_additional_message(
            "sixel_quant_apply_palette: "
            "a bad argument is detected, reqcolor < 0.");
        */
        return Err(Box::new(SixelError::BadArgument));
    }

    let mut f_mask = false;

    let f_diffuse = if depth != 3 {
        diffuse_none
    } else {
        match methodForDiffuse {
            DiffusionMethod::Auto | DiffusionMethod::None => diffuse_none,
            DiffusionMethod::Atkinson => diffuse_atkinson,
            DiffusionMethod::FS => diffuse_fs,
            DiffusionMethod::JaJuNi => diffuse_jajuni,
            DiffusionMethod::Stucki => diffuse_stucki,
            DiffusionMethod::Burkes => diffuse_burkes,
            DiffusionMethod::ADither => {
                f_mask = true;
                diffuse_none
            }
            DiffusionMethod::XDither => {
                f_mask = true;
                diffuse_none
            }
        }
    };
    type LookupFunc = fn(&[u8], i32, &[u8], i32, &mut [u16], i32) -> i32;
    let mut f_lookup: Option<LookupFunc> = None;
    if reqcolor == 2 {
        let mut sum1 = 0;
        let mut sum2 = 0;
        for n in 0..depth {
            sum1 += palette[n as usize] as i32;
        }
        for n in depth..(depth + depth) {
            sum2 += palette[n as usize] as i32;
        }
        if sum1 == 0 && sum2 == 255 * 3 {
            f_lookup = Some(lookup_mono_darkbg);
        } else if sum1 == 255 * 3 && sum2 == 0 {
            f_lookup = Some(lookup_mono_lightbg);
        }
    }
    if f_lookup.is_none() {
        if foptimize && depth == 3 {
            f_lookup = Some(lookup_fast);
        } else {
            f_lookup = Some(lookup_normal);
        }
    }

    let mut cc = vec![0u16, 1 << (depth * 5)];
    let indextable = match cachetable {
        Some(table) => table,
        None => &mut cc,
    };

    if foptimize_palette {
        ncolors = 0;
        let mut new_palette = vec![0; crate::SIXEL_PALETTE_MAX * depth as usize];
        let mut migration_map = vec![0; crate::SIXEL_PALETTE_MAX];

        if f_mask {
            for y in 0..height {
                for x in 0..width {
                    let mut copy: Vec<u8> = Vec::new();

                    let pos = y * width + x;
                    for d in 0..depth {
                        let mut val = data[(pos * depth + d) as usize] as i32;
                        if matches!(methodForDiffuse, DiffusionMethod::ADither) {
                            val += (mask_a(x, y, d) * 32.0) as i32;
                        } else {
                            val += (mask_x(x, y, d) * 32.0) as i32;
                        }
                        copy.push(val.clamp(0, 255) as u8);
                    }
                    //                   &[u8], i32, &[u8], i32, &mut [u16], i32
                    let color_index =
                        f_lookup.unwrap()(&copy, depth, palette, reqcolor, indextable, complexion)
                            as usize;
                    if migration_map[color_index] == 0 {
                        result[pos as usize] = ncolors as u8;
                        for n in 0..depth {
                            new_palette[(ncolors * depth + n) as usize] =
                                palette[color_index * depth as usize + n as usize];
                        }
                        ncolors += 1;
                        migration_map[color_index] = ncolors;
                    } else {
                        result[pos as usize] = migration_map[color_index] as u8 - 1;
                    }
                }
            }
            *palette = new_palette;
        } else {
            for y in 0..height {
                for x in 0..width {
                    let pos = y * width + x;
                    let color_index = f_lookup.unwrap()(
                        &data[(pos * depth) as usize..],
                        depth,
                        palette,
                        reqcolor,
                        indextable,
                        complexion,
                    ) as usize;
                    if migration_map[color_index] == 0 {
                        result[pos as usize] = ncolors as u8;
                        for n in 0..depth {
                            new_palette[(ncolors * depth + n) as usize] =
                                palette[color_index * depth as usize + n as usize];
                        }
                        ncolors += 1;
                        migration_map[color_index] = ncolors;
                    } else {
                        result[pos as usize] = migration_map[color_index] as u8 - 1;
                    }
                    for n in 0..depth {
                        let offset = data[(pos * depth + n) as usize] as i32
                            - palette[color_index * depth as usize + n as usize] as i32;
                        f_diffuse(&mut data[n as usize..], width, height, x, y, depth, offset);
                    }
                }
            }
            *palette = new_palette;
        }
    } else {
        if f_mask {
            for y in 0..height {
                for x in 0..width {
                    let mut copy = Vec::new();
                    let pos = y * width + x;
                    for d in 0..depth {
                        let mut val = data[(pos * depth + d) as usize] as i32;
                        if matches!(methodForDiffuse, DiffusionMethod::ADither) {
                            val += (mask_a(x, y, d) * 32.0) as i32;
                        } else {
                            val += (mask_x(x, y, d) * 32.0) as i32;
                        }

                        copy.push(val.clamp(0, 255) as u8);
                    }
                    result[pos as usize] = f_lookup.unwrap()(
                        &mut copy, depth, palette, reqcolor, indextable, complexion,
                    ) as u8;
                }
            }
        } else {
            for y in 0..height {
                for x in 0..width {
                    let pos = y * width + x;
                    let color_index = f_lookup.unwrap()(
                        &mut data[(pos * depth) as usize..],
                        depth,
                        palette,
                        reqcolor,
                        indextable,
                        complexion,
                    ) as usize;
                    result[pos as usize] = color_index as u8;
                    for n in 0..depth {
                        let offset = data[(pos * depth + n) as usize] as i32
                            - palette[color_index * depth as usize + n as usize] as i32;
                        f_diffuse(&mut data[n as usize..], width, height, x, y, depth, offset);
                    }
                }
            }
        }
        ncolors = reqcolor;
    }

    Ok(ncolors)
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
 *
 * mediancut algorithm implementation is imported from pnmcolormap.c
 * in netpbm library.
 * http://netpbm.sourceforge.net/
 *
 * *******************************************************************************
 *                  original license block of pnmcolormap.c
 * *******************************************************************************
 *
 *   Derived from ppmquant, originally by Jef Poskanzer.
 *
 *   Copyright (C) 1989, 1991 by Jef Poskanzer.
 *   Copyright (C) 2001 by Bryan Henderson.
 *
 *   Permission to use, copy, modify, and distribute this software and its
 *   documentation for any purpose and without fee is hereby granted, provided
 *   that the above copyright notice appear in all copies and that both that
 *   copyright notice and this permission notice appear in supporting
 *   documentation.  This software is provided "as is" without express or
 *   implied warranty.
 *
 * ******************************************************************************
 *
 * Copyright (c) 2014-2018 Hayaki Saito
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
 *
 *
 */
