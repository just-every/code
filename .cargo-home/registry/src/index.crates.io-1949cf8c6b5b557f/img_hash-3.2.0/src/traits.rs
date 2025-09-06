use image::{imageops, DynamicImage, GenericImageView, GrayImage, ImageBuffer, Pixel};

use std::borrow::Cow;
use std::ops;

/// Interface for types used for storing hash data.
///
/// This is implemented for `Vec<u8>`, `Box<[u8]>` and arrays that are multiples/combinations of
/// useful x86 bytewise SIMD register widths (64, 128, 256, 512 bits).
///
/// Please feel free to open a pull request [on Github](https://github.com/abonander/img_hash)
/// if you need this implemented for a different array size.
pub trait HashBytes {
    /// Construct this type from an iterator of bytes.
    ///
    /// If this type has a finite capacity (i.e. an array) then it can ignore extra data
    /// (the hash API will not create a hash larger than this type can contain). Unused capacity
    /// **must** be zeroed.
    fn from_iter<I: Iterator<Item = u8>>(iter: I) -> Self where Self: Sized;

    /// Return the maximum capacity of this type, in bits.
    ///
    /// If this type has an arbitrary/theoretically infinite capacity, return `usize::max_value()`.
    fn max_bits() -> usize;

    /// Get the hash bytes as a slice.
    fn as_slice(&self) -> &[u8];
}

impl HashBytes for Box<[u8]> {
    fn from_iter<I: Iterator<Item = u8>>(iter: I) -> Self {
        // stable in 1.32, effectively the same thing
        // iter.collect()
        iter.collect::<Vec<u8>>().into_boxed_slice()
    }

    fn max_bits() -> usize {
        usize::max_value()
    }

    fn as_slice(&self) -> &[u8] { self }
}

impl HashBytes for Vec<u8> {
    fn from_iter<I: Iterator<Item=u8>>(iter: I) -> Self {
        iter.collect()
    }

    fn max_bits() -> usize {
        usize::max_value()
    }

    fn as_slice(&self) -> &[u8] { self }
}

macro_rules! hash_bytes_array {
    ($($n:expr),*) => {$(
        impl HashBytes for [u8; $n] {
            fn from_iter<I: Iterator<Item=u8>>(mut iter: I) -> Self {
                // optimizer should eliminate this zeroing
                let mut out = [0; $n];

                for (src, dest) in iter.by_ref().zip(out.as_mut()) {
                    *dest = src;
                }

                out
            }

            fn max_bits() -> usize {
                $n * 8
            }

            fn as_slice(&self) -> &[u8] { self }
        }
    )*}
}

hash_bytes_array!(8, 16, 24, 32, 40, 48, 56, 64);

struct BoolsToBytes<I> {
    iter: I,
}

impl<I> Iterator for BoolsToBytes<I> where I: Iterator<Item=bool> {
    type Item = u8;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        // starts at the LSB and works up
        self.iter.by_ref().take(8).enumerate().fold(None, |accum, (n, val)| {
            accum.or(Some(0)).map(|accum| accum | ((val as u8) << n))
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, upper) = self.iter.size_hint();
        (
            lower / 8,
            // if the upper bound doesn't evenly divide by `8` then we will yield an extra item
            upper.map(|upper| if upper % 8 == 0 { upper / 8 } else { upper / 8 + 1})
        )
    }
}

pub(crate) trait BitSet: HashBytes {
    fn from_bools<I: Iterator<Item = bool>>(iter: I) -> Self where Self: Sized {
        Self::from_iter(BoolsToBytes { iter })
    }

    fn hamming(&self, other: &Self) -> u32 {
        self.as_slice().iter().zip(other.as_slice()).map(|(l, r)| (l ^ r).count_ones()).sum()
    }
}

impl<T: HashBytes> BitSet for T {}

/// Shorthand trait bound for APIs in this crate.
///
/// Currently only implemented for the types provided by `image` with 8-bit channels.
pub trait Image: GenericImageView + 'static {
    /// The equivalent `ImageBuffer` type for this container.
    type Buf: Image + DiffImage;

    /// Grayscale the image, reducing to 8 bit depth and dropping the alpha channel.
    fn to_grayscale(&self) -> Cow<GrayImage>;

    /// Blur the image with the given `Gaussian` sigma.
    fn blur(&self, sigma: f32) -> Self::Buf;

    /// Iterate over the image, passing each pixel's coordinates and values in `u8` to the closure.
    ///
    /// The iteration order is unspecified but each pixel **must** be visited exactly _once_.
    ///
    /// If the pixel's channels are wider than 8 bits then the values should be scaled to
    /// `[0, 255]`, not truncated.
    ///
    /// ### Note
    /// If the pixel data length is 2 or 4, the last index is assumed to be the alpha channel.
    /// A pixel data length outside of `[1, 4]` will cause a panic.
    fn foreach_pixel8<F>(&self, foreach: F) where F: FnMut(u32, u32, &[u8]);
}

/// Image types that can be diffed.
pub trait DiffImage {
    /// Subtract the pixel values of `other` from `self` in-place.
    fn diff_inplace(&mut self, other: &Self);
}

#[cfg(not(feature = "nightly"))]
impl<P: 'static, C: 'static> Image for ImageBuffer<P, C>
    where P: Pixel<Subpixel = u8>, C: ops::Deref<Target=[u8]> {
    type Buf = ImageBuffer<P, Vec<u8>>;

    fn to_grayscale(&self) -> Cow<GrayImage> {
        Cow::Owned(imageops::grayscale(self))
    }

    fn blur(&self, sigma: f32) -> Self::Buf { imageops::blur(self, sigma) }

    fn foreach_pixel8<F>(&self, mut foreach: F) where F: FnMut(u32, u32, &[u8]) {
        self.enumerate_pixels().for_each(|(x, y, px)| foreach(x, y, px.channels()))
    }
}

#[cfg(feature = "nightly")]
impl<P: 'static, C: 'static> Image for ImageBuffer<P, C>
    where P: Pixel<Subpixel = u8>, C: ops::Deref<Target=[u8]> {
    type Buf = ImageBuffer<P, Vec<u8>>;

    default fn to_grayscale(&self) -> Cow<GrayImage> {
        Cow::Owned(imageops::grayscale(self))
    }

    default fn blur(&self, sigma: f32) -> Self::Buf { imageops::blur(self, sigma) }

    default fn foreach_pixel8<F>(&self, mut foreach: F) where F: FnMut(u32, u32, &[u8]) {
        self.enumerate_pixels().for_each(|(x, y, px)| foreach(x, y, px.channels()))
    }
}

impl<P: 'static> DiffImage for ImageBuffer<P, Vec<u8>> where P: Pixel<Subpixel = u8> {
    fn diff_inplace(&mut self, other: &Self) {
        self.iter_mut().zip(other.iter()).for_each(|(l, r)| *l -= r);
    }
}

impl Image for DynamicImage {
    type Buf = image::RgbaImage;

    fn to_grayscale(&self) -> Cow<GrayImage> {
        self.as_luma8().map_or_else(|| Cow::Owned(self.to_luma()), Cow::Borrowed)
    }

    fn blur(&self, sigma: f32) -> Self::Buf { imageops::blur(self, sigma) }

    fn foreach_pixel8<F>(&self, mut foreach: F) where F: FnMut(u32, u32, &[u8]) {
        self.pixels().for_each(|(x, y, px)| foreach(x, y, px.channels()))
    }
}

#[cfg(feature = "nightly")]
impl Image for GrayImage {
    // type Buf = GrayImage;

    // Avoids copying
    fn to_grayscale(&self) -> Cow<GrayImage> {
        Cow::Borrowed(self)
    }
}

#[test]
fn test_bools_to_bytes() {
    let bools = (0 .. 16).map(|x| x & 1 == 0);
    let bytes = Vec::from_bools(bools.clone());
    assert_eq!(*bytes, [0b01010101; 2]);

    let bools_to_bytes = BoolsToBytes { iter: bools };
    assert_eq!(bools_to_bytes.size_hint(), (2, Some(2)));
}
