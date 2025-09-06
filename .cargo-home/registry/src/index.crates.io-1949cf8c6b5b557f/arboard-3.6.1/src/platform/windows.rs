/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2022 The Arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

#[cfg(feature = "image-data")]
use crate::common::ImageData;
use crate::common::{private, Error};
use std::{
	borrow::Cow,
	io,
	marker::PhantomData,
	os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
	path::{Path, PathBuf},
	thread,
	time::Duration,
};
use windows_sys::Win32::{
	Foundation::{GetLastError, GlobalFree, HANDLE, HGLOBAL, POINT, S_OK},
	Storage::FileSystem::{GetFinalPathNameByHandleW, FILE_FLAG_BACKUP_SEMANTICS, VOLUME_NAME_DOS},
	System::{
		DataExchange::SetClipboardData,
		Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GHND},
		Ole::CF_HDROP,
	},
	UI::Shell::{PathCchStripPrefix, DROPFILES},
};

#[cfg(feature = "image-data")]
mod image_data {
	use super::*;
	use crate::common::ScopeGuard;
	use image::codecs::bmp::BmpDecoder;
	use image::codecs::png::PngDecoder;
	use image::codecs::png::PngEncoder;
	use image::DynamicImage;
	use image::ExtendedColorType;
	use image::ImageDecoder;
	use image::ImageEncoder;
	use std::{convert::TryInto, mem::size_of, ptr::copy_nonoverlapping};
	use windows_sys::Win32::{
		Graphics::Gdi::{
			DeleteObject, BITMAPV5HEADER, BI_BITFIELDS, BI_RGB, HGDIOBJ, LCS_GM_IMAGES,
		},
		System::Ole::CF_DIBV5,
	};

	pub(super) fn add_cf_dibv5(
		_open_clipboard: OpenClipboard,
		image: ImageData,
	) -> Result<(), Error> {
		// This constant is missing in windows-rs
		// https://github.com/microsoft/windows-rs/issues/2711
		#[allow(non_upper_case_globals)]
		const LCS_sRGB: u32 = 0x7352_4742;

		let header_size = size_of::<BITMAPV5HEADER>();
		let header = BITMAPV5HEADER {
			bV5Size: header_size as u32,
			bV5Width: image.width as i32,
			bV5Height: image.height as i32,
			bV5Planes: 1,
			bV5BitCount: 32,
			bV5Compression: BI_BITFIELDS,
			bV5SizeImage: (4 * image.width * image.height) as u32,
			bV5XPelsPerMeter: 0,
			bV5YPelsPerMeter: 0,
			bV5ClrUsed: 0,
			bV5ClrImportant: 0,
			bV5RedMask: 0x00ff0000,
			bV5GreenMask: 0x0000ff00,
			bV5BlueMask: 0x000000ff,
			bV5AlphaMask: 0xff000000,
			bV5CSType: LCS_sRGB,
			// SAFETY: Windows ignores this field because `bV5CSType` is not set to `LCS_CALIBRATED_RGB`.
			bV5Endpoints: unsafe { std::mem::zeroed() },
			bV5GammaRed: 0,
			bV5GammaGreen: 0,
			bV5GammaBlue: 0,
			bV5Intent: LCS_GM_IMAGES as u32, // I'm not sure about this.
			bV5ProfileData: 0,
			bV5ProfileSize: 0,
			bV5Reserved: 0,
		};

		// In theory we don't need to flip the image because we could just specify
		// a negative height in the header, which according to the documentation, indicates that the
		// image rows are in top-to-bottom order. HOWEVER: MS Word (and WordPad) cannot paste an image
		// that has a negative height in its header.
		let image = flip_v(image);

		let data_size = header_size + image.bytes.len();
		let hdata = unsafe { global_alloc(data_size)? };
		unsafe {
			let data_ptr = global_lock(hdata)?;
			let _unlock = ScopeGuard::new(|| global_unlock_checked(hdata));

			copy_nonoverlapping::<u8>(
				(&header as *const BITMAPV5HEADER).cast(),
				data_ptr,
				header_size,
			);

			// Not using the `add` function, because that has a restriction, that the result cannot overflow isize
			let pixels_dst = data_ptr.add(header_size);
			copy_nonoverlapping::<u8>(image.bytes.as_ptr(), pixels_dst, image.bytes.len());

			let dst_pixels_slice = std::slice::from_raw_parts_mut(pixels_dst, image.bytes.len());

			// If the non-allocating version of the function failed, we need to assign the new bytes to
			// the global allocation.
			if let Cow::Owned(new_pixels) = rgba_to_win(dst_pixels_slice) {
				// SAFETY: `data_ptr` is valid to write to and has no outstanding mutable borrows, and
				// `new_pixels` will be the same length as the original bytes.
				copy_nonoverlapping::<u8>(new_pixels.as_ptr(), data_ptr, new_pixels.len())
			}
		}

		if unsafe { SetClipboardData(CF_DIBV5 as u32, hdata as HANDLE) }.failure() {
			unsafe { DeleteObject(hdata as HGDIOBJ) };
			Err(last_error("SetClipboardData failed with error"))
		} else {
			Ok(())
		}
	}

	pub(super) fn add_png_file(image: &ImageData) -> Result<(), Error> {
		// Try encoding the image as PNG.
		let mut buf = Vec::new();
		let encoder = PngEncoder::new(&mut buf);

		encoder
			.write_image(
				&image.bytes,
				image.width as u32,
				image.height as u32,
				ExtendedColorType::Rgba8,
			)
			.map_err(|_| Error::ConversionFailure)?;

		// Register PNG format.
		let format_id = match clipboard_win::register_format("PNG") {
			Some(format_id) => format_id.into(),
			None => return Err(last_error("Cannot register PNG clipboard format.")),
		};

		let data_size = buf.len();
		let hdata = unsafe { global_alloc(data_size)? };

		unsafe {
			let pixels_dst = global_lock(hdata)?;
			copy_nonoverlapping::<u8>(buf.as_ptr(), pixels_dst, data_size);
			global_unlock_checked(hdata);
		}

		if unsafe { SetClipboardData(format_id, hdata as HANDLE) }.failure() {
			unsafe { DeleteObject(hdata as HGDIOBJ) };
			Err(last_error("SetClipboardData failed with error"))
		} else {
			Ok(())
		}
	}

	// https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-bitmapv5header
	// According to the docs, when bV5Compression is BI_RGB, "the high byte in each DWORD
	// is not used".
	// This seems to not be respected in the real world. For example, Chrome, and Chromium
	// & Electron-based programs send us BI_RGB headers, but with bitCount=32 - and important
	// transparency bytes in the alpha channel.
	//
	// Apparently, it's our job as the consumer to do the right thing. This method fiddles
	// with the header a bit in these cases, then `image` handles the rest.
	fn maybe_tweak_header(dibv5: &mut [u8]) {
		assert!(dibv5.len() >= size_of::<BITMAPV5HEADER>());
		let src = dibv5.as_mut_ptr().cast::<BITMAPV5HEADER>();
		let mut header = unsafe { std::ptr::read_unaligned(src) };

		if header.bV5BitCount == 32
			&& header.bV5Compression == BI_RGB
			&& header.bV5AlphaMask == 0xff000000
		{
			header.bV5Compression = BI_BITFIELDS;
			if header.bV5RedMask == 0 && header.bV5GreenMask == 0 && header.bV5BlueMask == 0 {
				header.bV5RedMask = 0xff0000;
				header.bV5GreenMask = 0xff00;
				header.bV5BlueMask = 0xff;
			}

			unsafe { std::ptr::write_unaligned(src, header) };
		}
	}

	pub(super) fn read_cf_dibv5(dibv5: &mut [u8]) -> Result<ImageData<'static>, Error> {
		// The DIBV5 format is a BITMAPV5HEADER followed by the pixel data according to
		// https://docs.microsoft.com/en-us/windows/win32/dataxchg/standard-clipboard-formats

		let header_size = size_of::<BITMAPV5HEADER>();
		if dibv5.len() < header_size {
			return Err(Error::unknown("When reading the DIBV5 data, it contained fewer bytes than the BITMAPV5HEADER size. This is invalid."));
		}
		maybe_tweak_header(dibv5);

		let decoder = BmpDecoder::new_without_file_header(std::io::Cursor::new(&*dibv5))
			.map_err(|_| Error::ConversionFailure)?;
		let (width, height) = decoder.dimensions();
		let bytes = DynamicImage::from_decoder(decoder)
			.map_err(|_| Error::ConversionFailure)?
			.into_rgba8()
			.into_raw();

		Ok(ImageData { width: width as usize, height: height as usize, bytes: bytes.into() })
	}

	pub(super) fn read_png(data: &[u8]) -> Result<ImageData<'static>, Error> {
		let decoder =
			PngDecoder::new(std::io::Cursor::new(data)).map_err(|_| Error::ConversionFailure)?;
		let (width, height) = decoder.dimensions();

		let bytes = DynamicImage::from_decoder(decoder)
			.map_err(|_| Error::ConversionFailure)?
			.into_rgba8()
			.into_raw();

		Ok(ImageData { width: width as usize, height: height as usize, bytes: bytes.into() })
	}

	/// Converts the RGBA (u8) pixel data into the bitmap-native ARGB (u32)
	/// format in-place.
	///
	/// Safety: the `bytes` slice must have a length that's a multiple of 4
	#[allow(clippy::identity_op, clippy::erasing_op)]
	#[must_use]
	unsafe fn rgba_to_win(bytes: &mut [u8]) -> Cow<'_, [u8]> {
		// Check safety invariants to catch obvious bugs.
		debug_assert_eq!(bytes.len() % 4, 0);

		let mut u32pixels_buffer = convert_bytes_to_u32s(bytes);
		let u32pixels = match u32pixels_buffer {
			ImageDataCow::Borrowed(ref mut b) => b,
			ImageDataCow::Owned(ref mut b) => b.as_mut_slice(),
		};

		for p in u32pixels.iter_mut() {
			let [mut r, mut g, mut b, mut a] = p.to_ne_bytes().map(u32::from);
			r <<= 2 * 8;
			g <<= 1 * 8;
			b <<= 0 * 8;
			a <<= 3 * 8;

			*p = r | g | b | a;
		}

		match u32pixels_buffer {
			ImageDataCow::Borrowed(_) => Cow::Borrowed(bytes),
			ImageDataCow::Owned(bytes) => {
				Cow::Owned(bytes.into_iter().flat_map(|b| b.to_ne_bytes()).collect())
			}
		}
	}

	/// Vertically flips the image pixels in memory
	fn flip_v(image: ImageData) -> ImageData<'static> {
		let w = image.width;
		let h = image.height;

		let mut bytes = image.bytes.into_owned();

		let rowsize = w * 4; // each pixel is 4 bytes
		let mut tmp_a = vec![0; rowsize];
		// I believe this could be done safely with `as_chunks_mut`, but that's not stable yet
		for a_row_id in 0..(h / 2) {
			let b_row_id = h - a_row_id - 1;

			// swap rows `first_id` and `second_id`
			let a_byte_start = a_row_id * rowsize;
			let a_byte_end = a_byte_start + rowsize;
			let b_byte_start = b_row_id * rowsize;
			let b_byte_end = b_byte_start + rowsize;
			tmp_a.copy_from_slice(&bytes[a_byte_start..a_byte_end]);
			bytes.copy_within(b_byte_start..b_byte_end, a_byte_start);
			bytes[b_byte_start..b_byte_end].copy_from_slice(&tmp_a);
		}

		ImageData { width: image.width, height: image.height, bytes: bytes.into() }
	}

	/// Converts the ARGB (u32) pixel data into the RGBA (u8) format in-place
	///
	/// Safety: the `bytes` slice must have a length that's a multiple of 4
	#[allow(clippy::identity_op, clippy::erasing_op)]
	#[must_use]
	#[cfg(test)]
	unsafe fn win_to_rgba(bytes: &mut [u8]) -> Vec<u8> {
		// Check safety invariants to catch obvious bugs.
		debug_assert_eq!(bytes.len() % 4, 0);

		let mut u32pixels_buffer = convert_bytes_to_u32s(bytes);
		let u32pixels = match u32pixels_buffer {
			ImageDataCow::Borrowed(ref mut b) => b,
			ImageDataCow::Owned(ref mut b) => b.as_mut_slice(),
		};

		for p in u32pixels {
			let mut bytes = p.to_ne_bytes();
			bytes[0] = (*p >> (2 * 8)) as u8;
			bytes[1] = (*p >> (1 * 8)) as u8;
			bytes[2] = (*p >> (0 * 8)) as u8;
			bytes[3] = (*p >> (3 * 8)) as u8;
			*p = u32::from_ne_bytes(bytes);
		}

		match u32pixels_buffer {
			ImageDataCow::Borrowed(_) => bytes.to_vec(),
			ImageDataCow::Owned(bytes) => bytes.into_iter().flat_map(|b| b.to_ne_bytes()).collect(),
		}
	}

	// XXX: std's Cow is not usable here because it does not allow mutably
	// borrowing data.
	enum ImageDataCow<'a> {
		Borrowed(&'a mut [u32]),
		Owned(Vec<u32>),
	}

	/// Safety: the `bytes` slice must have a length that's a multiple of 4
	unsafe fn convert_bytes_to_u32s(bytes: &mut [u8]) -> ImageDataCow<'_> {
		// When the correct conditions are upheld, `std` should return everything in the well-aligned slice.
		let (prefix, _, suffix) = bytes.align_to::<u32>();

		// Check if `align_to` gave us the optimal result.
		//
		// If it didn't, use the slow path with more allocations
		if prefix.is_empty() && suffix.is_empty() {
			// We know that the newly-aligned slice will contain all the values
			ImageDataCow::Borrowed(bytes.align_to_mut::<u32>().1)
		} else {
			// XXX: Use `as_chunks` when it stabilizes.
			let u32pixels_buffer = bytes
				.chunks(4)
				.map(|chunk| u32::from_ne_bytes(chunk.try_into().unwrap()))
				.collect();
			ImageDataCow::Owned(u32pixels_buffer)
		}
	}

	#[test]
	fn conversion_between_win_and_rgba() {
		const DATA: [u8; 16] =
			[100, 100, 255, 100, 0, 0, 0, 255, 255, 100, 100, 255, 100, 255, 100, 100];

		let mut data = DATA;
		let _converted = unsafe { win_to_rgba(&mut data) };

		let mut data = DATA;
		let _converted = unsafe { rgba_to_win(&mut data) };

		let mut data = DATA;
		let _converted = unsafe { win_to_rgba(&mut data) };
		let _converted = unsafe { rgba_to_win(&mut data) };
		assert_eq!(data, DATA);

		let mut data = DATA;
		let _converted = unsafe { rgba_to_win(&mut data) };
		let _converted = unsafe { win_to_rgba(&mut data) };
		assert_eq!(data, DATA);
	}

	#[test]
	fn firefox_dibv5() {
		// A 5x5 sample of https://commons.wikimedia.org/wiki/File:PNG_transparency_demonstration_1.png
		let mut raw = vec![
			124, 0, 0, 0, 5, 0, 0, 0, 5, 0, 0, 0, 1, 0, 24, 0, 0, 0, 0, 0, 80, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 0, 0,
			255, 66, 71, 82, 115, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 36, 47, 144, 42, 68, 110, 48, 74, 66, 52, 74,
			49, 57, 80, 55, 0, 36, 53, 138, 45, 79, 98, 52, 82, 58, 56, 84, 52, 62, 91, 58, 0, 37,
			64, 129, 48, 88, 88, 54, 90, 54, 60, 96, 55, 66, 104, 62, 0, 40, 75, 120, 50, 96, 74,
			55, 99, 51, 62, 106, 57, 68, 113, 62, 0, 42, 89, 107, 50, 104, 60, 57, 108, 49, 64,
			114, 56, 71, 123, 65, 0,
		];

		let before = raw.clone();
		let image = read_cf_dibv5(&mut raw).unwrap();

		// Not expecting any header fiddling to happen here. This is a bitmap in 24-bit format, with a header
		// that says as much
		assert_eq!(raw, before);

		assert_eq!(image.width, 5);
		assert_eq!(image.height, 5);

		const EXPECTED: &[u8] = &[
			107, 89, 42, 255, 60, 104, 50, 255, 49, 108, 57, 255, 56, 114, 64, 255, 65, 123, 71,
			255, 120, 75, 40, 255, 74, 96, 50, 255, 51, 99, 55, 255, 57, 106, 62, 255, 62, 113, 68,
			255, 129, 64, 37, 255, 88, 88, 48, 255, 54, 90, 54, 255, 55, 96, 60, 255, 62, 104, 66,
			255, 138, 53, 36, 255, 98, 79, 45, 255, 58, 82, 52, 255, 52, 84, 56, 255, 58, 91, 62,
			255, 144, 47, 36, 255, 110, 68, 42, 255, 66, 74, 48, 255, 49, 74, 52, 255, 55, 80, 57,
			255,
		];
		assert_eq!(image.bytes, EXPECTED);
	}

	#[test]
	fn chrome_dibv5() {
		// A 5x5 sample of https://commons.wikimedia.org/wiki/File:PNG_transparency_demonstration_1.png
		// (interestingly, the same sample as in the Firefox test - despite the pixel data being
		// materially different!)
		let mut raw = vec![
			124, 0, 0, 0, 5, 0, 0, 0, 5, 0, 0, 0, 1, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255,
			32, 110, 105, 87, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 38, 145, 192, 38, 65, 111, 158, 46, 73, 68,
			107, 50, 73, 50, 92, 55, 79, 55, 100, 31, 46, 139, 190, 41, 76, 100, 152, 49, 81, 60,
			110, 53, 83, 53, 108, 60, 91, 60, 118, 32, 59, 131, 187, 44, 86, 89, 150, 51, 89, 56,
			121, 57, 95, 57, 127, 63, 103, 63, 139, 35, 71, 122, 186, 46, 95, 76, 150, 52, 99, 54,
			136, 59, 105, 59, 146, 65, 113, 65, 156, 37, 86, 109, 184, 46, 103, 63, 155, 52, 107,
			53, 152, 60, 114, 60, 162, 68, 123, 68, 174,
		];

		let before = raw.clone();
		let image = read_cf_dibv5(&mut raw).unwrap();

		// Chrome's header is dodgy. Expect that we fiddled with it.
		assert_ne!(raw, before);

		assert_eq!(image.width, 5);
		assert_eq!(image.height, 5);

		const EXPECTED: &[u8] = &[
			109, 86, 37, 184, 63, 103, 46, 155, 53, 107, 52, 152, 60, 114, 60, 162, 68, 123, 68,
			174, 122, 71, 35, 186, 76, 95, 46, 150, 54, 99, 52, 136, 59, 105, 59, 146, 65, 113, 65,
			156, 131, 59, 32, 187, 89, 86, 44, 150, 56, 89, 51, 121, 57, 95, 57, 127, 63, 103, 63,
			139, 139, 46, 31, 190, 100, 76, 41, 152, 60, 81, 49, 110, 53, 83, 53, 108, 60, 91, 60,
			118, 145, 38, 32, 192, 111, 65, 38, 158, 68, 73, 46, 107, 50, 73, 50, 92, 55, 79, 55,
			100,
		];
		assert_eq!(image.bytes, EXPECTED);
	}
}

unsafe fn global_alloc(bytes: usize) -> Result<HGLOBAL, Error> {
	let hdata = GlobalAlloc(GHND, bytes);
	if hdata.is_null() {
		Err(last_error("Could not allocate global memory object"))
	} else {
		Ok(hdata)
	}
}

unsafe fn global_lock(hmem: HGLOBAL) -> Result<*mut u8, Error> {
	let data_ptr = GlobalLock(hmem).cast::<u8>();
	if data_ptr.is_null() {
		Err(last_error("Could not lock the global memory object"))
	} else {
		Ok(data_ptr)
	}
}

unsafe fn global_unlock_checked(hdata: HGLOBAL) {
	// If the memory object is unlocked after decrementing the lock count, the function
	// returns zero and GetLastError returns NO_ERROR. If it fails, the return value is
	// zero and GetLastError returns a value other than NO_ERROR.
	if GlobalUnlock(hdata) == 0 {
		let err = io::Error::last_os_error();
		if err.raw_os_error() != Some(0) {
			log::error!("Failed calling GlobalUnlock when writing data: {}", err);
		}
	}
}

fn last_error(message: &str) -> Error {
	let os_error = io::Error::last_os_error();
	Error::unknown(format!("{message}: {os_error}"))
}

/// An abstraction trait over the different ways a Win32 function may return
/// a value with a failure marker.
///
/// This trait helps unify error handling across varying `windows-sys` versions,
/// providing a consistent interface for representing NULL values.
trait ResultValue: Sized {
	const NULL: Self;
	fn failure(self) -> bool;
}

// windows-sys >= 0.59
impl<T> ResultValue for *mut T {
	const NULL: Self = core::ptr::null_mut();
	fn failure(self) -> bool {
		self == Self::NULL
	}
}

// `windows-sys` 0.52
impl ResultValue for isize {
	const NULL: Self = 0;
	fn failure(self) -> bool {
		self == Self::NULL
	}
}

/// A shim clipboard type that can have operations performed with it, but
/// does not represent an open clipboard itself.
///
/// Windows only allows one thread on the entire system to have the clipboard
/// open at once, so we have to open it very sparingly or risk causing the rest
/// of the system to be unresponsive. Instead, the clipboard is opened for
/// every operation and then closed afterwards.
pub(crate) struct Clipboard(());

// The other platforms have `Drop` implementation on their
// clipboard, so Windows should too for consistently.
impl Drop for Clipboard {
	fn drop(&mut self) {}
}

struct OpenClipboard<'clipboard> {
	_inner: clipboard_win::Clipboard,
	// The Windows clipboard can not be sent between threads once
	// open.
	_marker: PhantomData<*const ()>,
	_for_shim: &'clipboard mut Clipboard,
}

impl Clipboard {
	const DEFAULT_OPEN_ATTEMPTS: usize = 5;

	pub(crate) fn new() -> Result<Self, Error> {
		Ok(Self(()))
	}

	fn open(&mut self) -> Result<OpenClipboard<'_>, Error> {
		// Attempt to open the clipboard multiple times. On Windows, its common for something else to temporarily
		// be using it during attempts.
		//
		// For past work/evidence, see Firefox(https://searchfox.org/mozilla-central/source/widget/windows/nsClipboard.cpp#421) and
		// Chromium(https://source.chromium.org/chromium/chromium/src/+/main:ui/base/clipboard/clipboard_win.cc;l=86).
		//
		// Note: This does not use `Clipboard::new_attempts` because its implementation sleeps for `0ms`, which can
		// cause race conditions between closing/opening the clipboard in single-threaded apps.
		let mut attempts = Self::DEFAULT_OPEN_ATTEMPTS;
		let clipboard = loop {
			match clipboard_win::Clipboard::new() {
				Ok(this) => break Ok(this),
				Err(err) => match attempts {
					0 => break Err(err),
					_ => attempts -= 1,
				},
			}

			// The default value matches Chromium's implementation, but could be tweaked later.
			thread::sleep(Duration::from_millis(5));
		}
		.map_err(|_| Error::ClipboardOccupied)?;

		Ok(OpenClipboard { _inner: clipboard, _marker: PhantomData, _for_shim: self })
	}
}

// Note: In all of the builders, a clipboard opening result is stored.
// This is done for a few reasons:
// 1. consistently with the other platforms which can have an occupied clipboard.
// 	It is better if the operation fails at the most similar place on all platforms.
// 2. `{Get, Set, Clear}::new()` don't return a `Result`. Windows is the only case that
// 	needs this kind of handling, so it doesn't need to affect the other APIs.
// 3. Due to how the clipboard works on Windows, we need to open it for every operation
// and keep it open until its finished. This approach allows RAII to still be applicable.

pub(crate) struct Get<'clipboard> {
	clipboard: Result<OpenClipboard<'clipboard>, Error>,
}

impl<'clipboard> Get<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self { clipboard: clipboard.open() }
	}

	pub(crate) fn text(self) -> Result<String, Error> {
		const FORMAT: u32 = clipboard_win::formats::CF_UNICODETEXT;

		let _clipboard_assertion = self.clipboard?;

		// XXX: ToC/ToU race conditions are not possible because we are the sole owners of the clipboard currently.
		if !clipboard_win::is_format_avail(FORMAT) {
			return Err(Error::ContentNotAvailable);
		}

		// NB: Its important that whatever functionality decodes the text buffer from the clipboard
		// uses `WideCharToMultiByte` with `CP_UTF8` (or an equivalent) in order to handle when both "text"
		// and a locale identifier were placed on the clipboard. It is probable this occurs when an application
		// is running with a codepage that isn't the current system's, such as under a locale emulator.
		//
		// In these cases, Windows decodes the text buffer with whatever codepage that identifier is for
		// when creating the `CF_UNICODETEXT` buffer. Therefore, the buffer could then be in any format,
		// not nessecarily wide UTF-16. We need to then undo that, taking the wide data and mapping it into
		// the UTF-8 space as best as possible.
		//
		// (locale-specific text data, locale id) -> app -> system -> arboard (locale-specific text data) -> UTF-8
		let mut out = Vec::new();
		clipboard_win::raw::get_string(&mut out).map_err(|_| Error::ContentNotAvailable)?;
		String::from_utf8(out).map_err(|_| Error::ConversionFailure)
	}

	pub(crate) fn html(self) -> Result<String, Error> {
		let _clipboard_assertion = self.clipboard?;

		let format = clipboard_win::register_format("HTML Format")
			.ok_or_else(|| Error::unknown("unable to register HTML format"))?;

		let mut out: Vec<u8> = Vec::new();
		clipboard_win::raw::get_html(format.get(), &mut out)
			.map_err(|_| Error::unknown("failed to read clipboard string"))?;

		String::from_utf8(out).map_err(|_| Error::ConversionFailure)
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self) -> Result<ImageData<'static>, Error> {
		let _clipboard_assertion = self.clipboard?;
		let mut data = Vec::new();

		let png_format: Option<u32> = clipboard_win::register_format("PNG").map(From::from);
		if let Some(id) = png_format.filter(|&id| clipboard_win::is_format_avail(id)) {
			// Looks like PNG is available! Let's try it
			clipboard_win::raw::get_vec(id, &mut data)
				.map_err(|_| Error::unknown("failed to read clipboard PNG data"))?;
			return image_data::read_png(&data);
		}

		if !clipboard_win::is_format_avail(clipboard_win::formats::CF_DIBV5) {
			return Err(Error::ContentNotAvailable);
		}

		clipboard_win::raw::get_vec(clipboard_win::formats::CF_DIBV5, &mut data)
			.map_err(|_| Error::unknown("failed to read clipboard image data"))?;
		image_data::read_cf_dibv5(&mut data)
	}

	pub(crate) fn file_list(self) -> Result<Vec<PathBuf>, Error> {
		let _clipboard_assertion = self.clipboard?;

		let mut file_list = Vec::new();
		clipboard_win::raw::get_file_list_path(&mut file_list)
			.map_err(|_| Error::ContentNotAvailable)?;

		Ok(file_list)
	}
}

pub(crate) struct Set<'clipboard> {
	clipboard: Result<OpenClipboard<'clipboard>, Error>,
	exclude_from_monitoring: bool,
	exclude_from_cloud: bool,
	exclude_from_history: bool,
}

impl<'clipboard> Set<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self {
			clipboard: clipboard.open(),
			exclude_from_monitoring: false,
			exclude_from_cloud: false,
			exclude_from_history: false,
		}
	}

	pub(crate) fn text(self, data: Cow<'_, str>) -> Result<(), Error> {
		let open_clipboard = self.clipboard?;

		clipboard_win::raw::set_string(&data)
			.map_err(|_| Error::unknown("Could not place the specified text to the clipboard"))?;

		add_clipboard_exclusions(
			open_clipboard,
			self.exclude_from_monitoring,
			self.exclude_from_cloud,
			self.exclude_from_history,
		)
	}

	pub(crate) fn html(self, html: Cow<'_, str>, alt: Option<Cow<'_, str>>) -> Result<(), Error> {
		let open_clipboard = self.clipboard?;

		let alt = match alt {
			Some(s) => s.into(),
			None => String::new(),
		};
		clipboard_win::raw::set_string(&alt)
			.map_err(|_| Error::unknown("Could not place the specified text to the clipboard"))?;

		if let Some(format) = clipboard_win::register_format("HTML Format") {
			let html = wrap_html(&html);
			clipboard_win::raw::set_without_clear(format.get(), html.as_bytes())
				.map_err(|e| Error::unknown(e.to_string()))?;
		}

		add_clipboard_exclusions(
			open_clipboard,
			self.exclude_from_monitoring,
			self.exclude_from_cloud,
			self.exclude_from_history,
		)
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self, image: ImageData) -> Result<(), Error> {
		let open_clipboard = self.clipboard?;

		if let Err(e) = clipboard_win::raw::empty() {
			return Err(Error::unknown(format!(
				"Failed to empty the clipboard. Got error code: {e}"
			)));
		};

		// XXX: The ordering of these functions is important, as some programs will grab the
		// first format available. PNGs tend to have better compatibility on Windows, so it is set first.
		image_data::add_png_file(&image)?;
		image_data::add_cf_dibv5(open_clipboard, image)?;
		Ok(())
	}

	pub(crate) fn file_list(self, file_list: &[impl AsRef<Path>]) -> Result<(), Error> {
		const DROPFILES_HEADER_SIZE: usize = std::mem::size_of::<DROPFILES>();

		let clipboard_assertion = self.clipboard?;

		// https://learn.microsoft.com/en-us/windows/win32/shell/clipboard#cf_hdrop
		// CF_HDROP consists of an STGMEDIUM structure that contains a global memory object.
		// The structure's hGlobal member points to the resulting data:
		// | DROPFILES | FILENAME | NULL | ... | nth FILENAME | NULL | NULL |
		let dropfiles = DROPFILES {
			pFiles: DROPFILES_HEADER_SIZE as u32,
			pt: POINT { x: 0, y: 0 },
			fNC: 0,
			fWide: 1,
		};

		let mut data_len = DROPFILES_HEADER_SIZE;

		let paths: Vec<_> = file_list
			.iter()
			.filter_map(|path| {
				to_final_path_wide(path.as_ref()).map(|wide| {
					// Windows uses wchar_t which is 16 bit
					data_len += wide.len() * std::mem::size_of::<u16>();
					wide
				})
			})
			.collect();

		if paths.is_empty() {
			return Err(Error::ConversionFailure);
		}

		// Add space for the final null character
		data_len += std::mem::size_of::<u16>();

		unsafe {
			let h_global = global_alloc(data_len)?;
			let data_ptr = global_lock(h_global)?;

			(data_ptr as *mut DROPFILES).write(dropfiles);

			let mut ptr = data_ptr.add(DROPFILES_HEADER_SIZE) as *mut u16;

			for wide_path in paths {
				std::ptr::copy_nonoverlapping::<u16>(wide_path.as_ptr(), ptr, wide_path.len());
				ptr = ptr.add(wide_path.len());
			}

			// Write final null character
			ptr.write(0);

			global_unlock_checked(h_global);

			if SetClipboardData(CF_HDROP.into(), h_global as HANDLE).failure() {
				GlobalFree(h_global);
				return Err(last_error("SetClipboardData failed with error"));
			}
		}

		add_clipboard_exclusions(
			clipboard_assertion,
			self.exclude_from_monitoring,
			self.exclude_from_cloud,
			self.exclude_from_history,
		)
	}
}

fn add_clipboard_exclusions(
	_open_clipboard: OpenClipboard<'_>,
	exclude_from_monitoring: bool,
	exclude_from_cloud: bool,
	exclude_from_history: bool,
) -> Result<(), Error> {
	/// `set` should be called with the registered format and a DWORD value of 0.
	///
	/// See https://docs.microsoft.com/en-us/windows/win32/dataxchg/clipboard-formats#cloud-clipboard-and-clipboard-history-formats
	const CLIPBOARD_EXCLUSION_DATA: &[u8] = &0u32.to_ne_bytes();

	// Clipboard exclusions are applied retroactively (we still have the clipboard lock) to the item that is currently in the clipboard.
	// See the MS docs on `CLIPBOARD_EXCLUSION_DATA` for specifics. Once the item is added to the clipboard,
	// tell Windows to remove it from cloud syncing and history.

	if exclude_from_monitoring {
		if let Some(format) =
			clipboard_win::register_format("ExcludeClipboardContentFromMonitorProcessing")
		{
			// The documentation states "place any data on the clipboard in this format to prevent...", and using the zero bytes
			// like the others for consistency works.
			clipboard_win::raw::set_without_clear(format.get(), CLIPBOARD_EXCLUSION_DATA)
				.map_err(|_| Error::unknown("Failed to exclude data from clipboard monitoring"))?;
		}
	}

	if exclude_from_cloud {
		if let Some(format) = clipboard_win::register_format("CanUploadToCloudClipboard") {
			// We believe that it would be a logic error if this call failed, since we've validated the format is supported,
			// we still have full ownership of the clipboard and aren't moving it to another thread, and this is a well-documented operation.
			// Due to these reasons, `Error::Unknown` is used because we never expect the error path to be taken.
			clipboard_win::raw::set_without_clear(format.get(), CLIPBOARD_EXCLUSION_DATA)
				.map_err(|_| Error::unknown("Failed to exclude data from cloud clipboard"))?;
		}
	}

	if exclude_from_history {
		if let Some(format) = clipboard_win::register_format("CanIncludeInClipboardHistory") {
			// See above for reasoning about using `Error::Unknown`.
			clipboard_win::raw::set_without_clear(format.get(), CLIPBOARD_EXCLUSION_DATA)
				.map_err(|_| Error::unknown("Failed to exclude data from clipboard history"))?;
		}
	}

	Ok(())
}

/// Windows-specific extensions to the [`Set`](crate::Set) builder.
pub trait SetExtWindows: private::Sealed {
	/// Exclude the data which will be set on the clipboard from being processed
	/// at all, either in the local clipboard history or getting uploaded to the cloud.
	///
	/// If this is set, it is not recommended to call [exclude_from_cloud](SetExtWindows::exclude_from_cloud) or [exclude_from_history](SetExtWindows::exclude_from_history).
	fn exclude_from_monitoring(self) -> Self;

	/// Excludes the data which will be set on the clipboard from being uploaded to
	/// the Windows 10/11 [cloud clipboard].
	///
	/// [cloud clipboard]: https://support.microsoft.com/en-us/windows/clipboard-in-windows-c436501e-985d-1c8d-97ea-fe46ddf338c6
	fn exclude_from_cloud(self) -> Self;

	/// Excludes the data which will be set on the clipboard from being added to
	/// the system's [clipboard history] list.
	///
	/// [clipboard history]: https://support.microsoft.com/en-us/windows/get-help-with-clipboard-30375039-ce71-9fe4-5b30-21b7aab6b13f
	fn exclude_from_history(self) -> Self;
}

impl SetExtWindows for crate::Set<'_> {
	fn exclude_from_monitoring(mut self) -> Self {
		self.platform.exclude_from_monitoring = true;
		self
	}

	fn exclude_from_cloud(mut self) -> Self {
		self.platform.exclude_from_cloud = true;
		self
	}

	fn exclude_from_history(mut self) -> Self {
		self.platform.exclude_from_history = true;
		self
	}
}

pub(crate) struct Clear<'clipboard> {
	clipboard: Result<OpenClipboard<'clipboard>, Error>,
}

impl<'clipboard> Clear<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self { clipboard: clipboard.open() }
	}

	pub(crate) fn clear(self) -> Result<(), Error> {
		let _clipboard_assertion = self.clipboard?;
		clipboard_win::empty().map_err(|_| Error::unknown("failed to clear clipboard"))
	}
}

fn wrap_html(ctn: &str) -> String {
	let h_version = "Version:0.9";
	let h_start_html = "\r\nStartHTML:";
	let h_end_html = "\r\nEndHTML:";
	let h_start_frag = "\r\nStartFragment:";
	let h_end_frag = "\r\nEndFragment:";
	let c_start_frag = "\r\n<html>\r\n<body>\r\n<!--StartFragment-->\r\n";
	let c_end_frag = "\r\n<!--EndFragment-->\r\n</body>\r\n</html>";
	let h_len = h_version.len()
		+ h_start_html.len()
		+ 10 + h_end_html.len()
		+ 10 + h_start_frag.len()
		+ 10 + h_end_frag.len()
		+ 10;
	let n_start_html = h_len + 2;
	let n_start_frag = h_len + c_start_frag.len();
	let n_end_frag = n_start_frag + ctn.len();
	let n_end_html = n_end_frag + c_end_frag.len();
	format!(
		"{h_version}{h_start_html}{n_start_html:010}{h_end_html}{n_end_html:010}{h_start_frag}{n_start_frag:010}{h_end_frag}{n_end_frag:010}{c_start_frag}{ctn}{c_end_frag}"
	)
}

/// Given a file path attempt to open it and call GetFinalPathNameByHandleW,
/// on success return the final path as a NULL terminated u16 Vec
fn to_final_path_wide(p: &Path) -> Option<Vec<u16>> {
	let file = std::fs::OpenOptions::new()
		// No read or write permissions are necessary
		.access_mode(0)
		// This flag is so we can open directories too
		.custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
		.open(p)
		.ok()?;

	fill_utf16_buf(
		|buf, sz| unsafe {
			GetFinalPathNameByHandleW(file.as_raw_handle() as HANDLE, buf, sz, VOLUME_NAME_DOS)
		},
		|buf| {
			let mut wide = Vec::with_capacity(buf.len() + 1);
			wide.extend_from_slice(buf);
			wide.push(0);

			let hr = unsafe { PathCchStripPrefix(wide.as_mut_ptr(), wide.len()) };
			// On success truncate invalid data
			if hr == S_OK {
				if let Some(end) = wide.iter().position(|c| *c == 0) {
					// Retain NULL character
					wide.truncate(end + 1)
				}
			}
			wide
		},
	)
}

/// <https://github.com/rust-lang/rust/blob/f34ba774c78ea32b7c40598b8ad23e75cdac42a6/library/std/src/sys/pal/windows/mod.rs#L211>
fn fill_utf16_buf<F1, F2, T>(mut f1: F1, f2: F2) -> Option<T>
where
	F1: FnMut(*mut u16, u32) -> u32,
	F2: FnOnce(&[u16]) -> T,
{
	// Start off with a stack buf but then spill over to the heap if we end up
	// needing more space.
	//
	// This initial size also works around `GetFullPathNameW` returning
	// incorrect size hints for some short paths:
	// https://github.com/dylni/normpath/issues/5
	let mut stack_buf: [std::mem::MaybeUninit<u16>; 512] = [std::mem::MaybeUninit::uninit(); 512];
	let mut heap_buf: Vec<std::mem::MaybeUninit<u16>> = Vec::new();
	unsafe {
		let mut n = stack_buf.len();
		loop {
			let buf = if n <= stack_buf.len() {
				&mut stack_buf[..]
			} else {
				let extra = n - heap_buf.len();
				heap_buf.reserve(extra);
				// We used `reserve` and not `reserve_exact`, so in theory we
				// may have gotten more than requested. If so, we'd like to use
				// it... so long as we won't cause overflow.
				n = heap_buf.capacity().min(u32::MAX as usize);
				// Safety: MaybeUninit<u16> does not need initialization
				heap_buf.set_len(n);
				&mut heap_buf[..]
			};

			// This function is typically called on windows API functions which
			// will return the correct length of the string, but these functions
			// also return the `0` on error. In some cases, however, the
			// returned "correct length" may actually be 0!
			//
			// To handle this case we call `SetLastError` to reset it to 0 and
			// then check it again if we get the "0 error value". If the "last
			// error" is still 0 then we interpret it as a 0 length buffer and
			// not an actual error.
			windows_sys::Win32::Foundation::SetLastError(0);
			let k = match f1(buf.as_mut_ptr().cast::<u16>(), n as u32) {
				0 if GetLastError() == 0 => 0,
				0 => return None,
				n => n,
			} as usize;
			if k == n && GetLastError() == windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER
			{
				n = n.saturating_mul(2).min(u32::MAX as usize);
			} else if k > n {
				n = k;
			} else if k == n {
				// It is impossible to reach this point.
				// On success, k is the returned string length excluding the null.
				// On failure, k is the required buffer length including the null.
				// Therefore k never equals n.
				unreachable!();
			} else {
				// Safety: First `k` values are initialized.
				let slice = std::slice::from_raw_parts(buf.as_ptr() as *const u16, k);
				return Some(f2(slice));
			}
		}
	}
}
