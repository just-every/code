use std::borrow::Cow;
use std::io;
use std::iter::FusedIterator;
use std::mem;

use std::io::prelude::*;
use std::num::NonZeroU64;
use std::convert::{TryFrom, TryInto};

use crate::Repeat;
use crate::common::{Block, Frame};

mod decoder;
mod converter;

pub use self::decoder::{
    PLTE_CHANNELS, StreamingDecoder, Decoded, DecodingError, DecodingFormatError,
    Version, FrameDataType, OutputBuffer, FrameDecoder
};

use self::converter::PixelConverter;
pub use self::converter::ColorOutput;

#[derive(Clone, Debug)]
/// The maximum amount of memory the decoder is allowed to use for each frame
pub enum MemoryLimit {
    /// Enforce no memory limit.
    ///
    /// If you intend to process images from unknown origins this is a potentially dangerous
    /// constant to use, as your program could be vulnerable to decompression bombs. That is,
    /// malicious images crafted specifically to require an enormous amount of memory to process
    /// while having a disproportionately small file size.
    ///
    /// The risks for modern machines are a bit smaller as the size of each frame cannot
    /// exceed 16GiB, but this is still a significant amount of memory.
    Unlimited,
    /// Limit the amount of memory that can be used for a single frame to this many bytes.
    ///
    /// It may not be enforced precisely due to allocator overhead
    /// and the decoder potentially allocating small auxiliary buffers,
    /// but it will precisely limit the size of the output buffer for each frame.
    //
    // The `NonZero` type is used to make FFI simpler.
    // Due to the guaranteed niche optimization, `Unlimited` will be represented as `0`,
    // and the whole enum as a simple `u64`.
    Bytes(NonZeroU64),
}

impl MemoryLimit {
    fn check_size(&self, size: usize) -> Result<(), DecodingError> {
        match self {
            Self::Unlimited => Ok(()),
            Self::Bytes(limit) => {
                if size as u64 <= limit.get() {
                    Ok(())
                } else {
                    Err(DecodingError::format("memory limit reached"))
                }
            },
        }
    }

    fn buffer_size(&self, color: ColorOutput, width: u16, height: u16) -> Option<usize> {
        let pixels = u64::from(width) * u64::from(height);

        let bytes_per_pixel = match color {
            ColorOutput::Indexed => 1,
            ColorOutput::RGBA => 4,
        };

        // This cannot overflow because the maximum possible value is 16GiB, well within u64 range
        let total_bytes = pixels * bytes_per_pixel;

        // On 32-bit platforms the size of the output buffer may not be representable
        let usize_bytes = usize::try_from(total_bytes).ok()?;

        match self {
            Self::Unlimited => Some(usize_bytes),
            Self::Bytes(limit) => {
                if total_bytes > limit.get() {
                    None
                } else {
                    Some(usize_bytes)
                }
            },
        }
    }
}

/// Options for opening a GIF decoder. [`DecodeOptions::read_info`] will start the decoder.
#[derive(Clone, Debug)]
pub struct DecodeOptions {
    memory_limit: MemoryLimit,
    color_output: ColorOutput,
    check_frame_consistency: bool,
    skip_frame_decoding: bool,
    check_for_end_code: bool,
    allow_unknown_blocks: bool,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl DecodeOptions {
    /// Creates a new decoder builder
    #[must_use]
    #[inline]
    pub fn new() -> Self {
        Self {
            memory_limit: MemoryLimit::Bytes(50_000_000.try_into().unwrap()), // 50 MB
            color_output: ColorOutput::Indexed,
            check_frame_consistency: false,
            skip_frame_decoding: false,
            check_for_end_code: false,
            allow_unknown_blocks: false,
        }
    }

    /// Configure how color data is decoded.
    #[inline]
    pub fn set_color_output(&mut self, color: ColorOutput) {
        self.color_output = color;
    }

    /// Configure a memory limit for decoding.
    pub fn set_memory_limit(&mut self, limit: MemoryLimit) {
        self.memory_limit = limit;
    }

    /// Configure if frames must be within the screen descriptor.
    ///
    /// The default is `false`.
    ///
    /// When turned on, all frame descriptors being read must fit within the screen descriptor or
    /// otherwise an error is returned and the stream left in an unspecified state.
    ///
    /// When turned off, frames may be arbitrarily larger or offset in relation to the screen. Many
    /// other decoder libraries handle this in highly divergent ways. This moves all checks to the
    /// caller, for example to emulate a specific style.
    pub fn check_frame_consistency(&mut self, check: bool) {
        self.check_frame_consistency = check;
    }

    /// Configure whether to skip decoding frames.
    ///
    /// The default is false.
    ///
    /// When turned on, LZW decoding is skipped. [`Decoder::read_next_frame`] will return
    /// compressed LZW bytes in frame's data.
    /// [`Decoder::next_frame_info`] will return the metadata of the next frame as usual.
    /// This is useful to count frames without incurring the overhead of decoding.
    pub fn skip_frame_decoding(&mut self, skip: bool) {
        self.skip_frame_decoding = skip;
    }

    /// Configure if LZW encoded blocks must end with a marker end code.
    ///
    /// The default is `false`.
    ///
    /// When turned on, all image data blocks—which are LZW encoded—must contain a special bit
    /// sequence signalling the end of the data. LZW processing terminates when this code is
    /// encountered. The specification states that it must be the last code output by the encoder
    /// for an image.
    ///
    /// When turned off then image data blocks can simply end. Note that this might silently ignore
    /// some bits of the last or second to last byte.
    pub fn check_lzw_end_code(&mut self, check: bool) {
        self.check_for_end_code = check;
    }

    /// Configure if unknown blocks are allowed to be decoded.
    ///
    /// The default is `false`.
    ///
    /// When turned on, the decoder will allow unknown blocks to be in the
    /// `BlockStart` position.
    ///
    /// When turned off, decoded block starts must mark an `Image`, `Extension`,
    /// or `Trailer` block. Otherwise, the decoded image will return an error.
    /// If an unknown block error is returned from decoding, enabling this
    /// setting may allow for a further state of decoding on the next attempt.
    ///
    /// This option also allows unknown extension blocks. The decoder assumes the follow the same
    /// block layout, i.e. a sequence of zero-length terminated sub-blocks immediately follow the
    /// extension introducer.
    pub fn allow_unknown_blocks(&mut self, check: bool) {
        self.allow_unknown_blocks = check;
    }

    /// Reads the logical screen descriptor including the global color palette
    ///
    /// Returns a [`Decoder`]. All decoder configuration has to be done beforehand.
    pub fn read_info<R: Read>(self, r: R) -> Result<Decoder<R>, DecodingError> {
        Decoder::with_no_init(r, StreamingDecoder::with_options(&self), self).init()
    }
}

struct ReadDecoder<R: Read> {
    reader: io::BufReader<R>,
    decoder: StreamingDecoder,
    at_eof: bool,
}

impl<R: Read> ReadDecoder<R> {
    #[inline(never)]
    fn decode_next(&mut self, write_into: &mut OutputBuffer<'_>) -> Result<Option<Decoded>, DecodingError> {
        while !self.at_eof {
            let (consumed, result) = {
                let buf = self.reader.fill_buf()?;
                if buf.is_empty() {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }

                self.decoder.update(buf, write_into)?
            };
            self.reader.consume(consumed);
            match result {
                Decoded::Nothing => (),
                Decoded::BlockStart(Block::Trailer) => {
                    self.at_eof = true;
                },
                result => return Ok(Some(result)),
            }
        }
        Ok(None)
    }

    fn into_inner(self) -> io::BufReader<R> {
        self.reader
    }

    fn decode_next_bytes(&mut self, out: &mut OutputBuffer<'_>) -> Result<usize, DecodingError> {
        match self.decode_next(out)? {
            Some(Decoded::BytesDecoded(len)) => Ok(len.get()),
            Some(Decoded::DataEnd) => Ok(0),
            _ => Err(DecodingError::format("unexpected data")),
        }
    }
}

#[allow(dead_code)]
/// GIF decoder. Create [`DecodeOptions`] to get started, and call [`DecodeOptions::read_info`].
pub struct Decoder<R: Read> {
    decoder: ReadDecoder<R>,
    pixel_converter: PixelConverter,
    bg_color: Option<u8>,
    repeat: Repeat,
    current_frame: Frame<'static>,
    current_frame_data_type: FrameDataType,
}

impl<R> Decoder<R> where R: Read {
    /// Create a new decoder with default options.
    #[inline]
    pub fn new(reader: R) -> Result<Self, DecodingError> {
        DecodeOptions::new().read_info(reader)
    }

    /// Return a builder that allows configuring limits etc.
    #[must_use]
    #[inline]
    pub fn build() -> DecodeOptions {
        DecodeOptions::new()
    }

    fn with_no_init(reader: R, decoder: StreamingDecoder, options: DecodeOptions) -> Self {
        Self {
            decoder: ReadDecoder {
                reader: io::BufReader::new(reader),
                decoder,
                at_eof: false,
            },
            bg_color: None,
            pixel_converter: PixelConverter::new(options.color_output, options.memory_limit),
            repeat: Repeat::default(),
            current_frame: Frame::default(),
            current_frame_data_type: FrameDataType::Pixels,
        }
    }

    fn init(mut self) -> Result<Self, DecodingError> {
        loop {
            match self.decoder.decode_next(&mut OutputBuffer::None)? {
                Some(Decoded::BackgroundColor(bg_color)) => {
                    self.bg_color = Some(bg_color);
                }
                Some(Decoded::GlobalPalette(palette)) => {
                    self.pixel_converter.set_global_palette(palette.into());
                },
                Some(Decoded::Repetitions(repeat)) => {
                    self.repeat = repeat;
                },
                Some(Decoded::HeaderEnd) => break,
                Some(_) => {
                    // There will be extra events when parsing application extension
                    continue;
                },
                None => return Err(DecodingError::format(
                    "file does not contain any image data"
                ))
            }
        }
        // If the background color is invalid, ignore it
        if let Some(palette) = self.pixel_converter.global_palette() {
            if self.bg_color.unwrap_or(0) as usize >= (palette.len() / PLTE_CHANNELS) {
                self.bg_color = None;
            }
        }
        Ok(self)
    }

    /// Returns the next frame info
    pub fn next_frame_info(&mut self) -> Result<Option<&Frame<'static>>, DecodingError> {
        loop {
            match self.decoder.decode_next(&mut OutputBuffer::None)? {
                Some(Decoded::FrameMetadata(frame_data_type)) => {
                    self.current_frame = self.decoder.decoder.current_frame_mut().take();
                    self.current_frame_data_type = frame_data_type;
                    if self.current_frame.palette.is_none() && self.global_palette().is_none() {
                        return Err(DecodingError::format(
                            "no color table available for current frame",
                        ));
                    }
                    break;
                }
                Some(_) => (),
                None => return Ok(None),
            }
        }
        Ok(Some(&self.current_frame))
    }

    /// Reads the next frame from the image.
    ///
    /// Do not call `Self::next_frame_info` beforehand.
    /// Deinterlaces the result.
    ///
    /// You can also call `.into_iter()` on the decoder to use it as a regular iterator.
    pub fn read_next_frame(&mut self) -> Result<Option<&Frame<'static>>, DecodingError> {
        if self.next_frame_info()?.is_some() {
            match self.current_frame_data_type {
                FrameDataType::Pixels => {
                    self.pixel_converter.read_frame(&mut self.current_frame, &mut |out| self.decoder.decode_next_bytes(out))?;
                },
                FrameDataType::Lzw { min_code_size } => {
                    let mut vec = if matches!(self.current_frame.buffer, Cow::Owned(_)) {
                        let mut vec = mem::replace(&mut self.current_frame.buffer, Cow::Borrowed(&[])).into_owned();
                        vec.clear();
                        vec
                    } else {
                        Vec::new()
                    };
                    // Guesstimate 2bpp
                    vec.try_reserve(usize::from(self.current_frame.width) * usize::from(self.current_frame.height) / 4)
                        .map_err(|_| io::Error::from(io::ErrorKind::OutOfMemory))?;
                    self.copy_lzw_into_buffer(min_code_size, &mut vec)?;
                    self.current_frame.buffer = Cow::Owned(vec);
                },
            }
            Ok(Some(&self.current_frame))
        } else {
            Ok(None)
        }
    }

    /// This is private for iterator's use
    fn take_current_frame(&mut self) -> Option<Frame<'static>> {
        if self.current_frame.buffer.is_empty() {
            return None;
        }
        Some(self.current_frame.take())
    }

    /// Reads the data of the current frame into a pre-allocated buffer.
    ///
    /// `Self::next_frame_info` needs to be called beforehand.
    /// The length of `buf` must be at least `Self::buffer_size`.
    /// Deinterlaces the result.
    pub fn read_into_buffer(&mut self, buf: &mut [u8]) -> Result<(), DecodingError> {
        self.pixel_converter.read_into_buffer(&self.current_frame, buf, &mut |out| self.decoder.decode_next_bytes(out))
    }

    fn copy_lzw_into_buffer(&mut self, min_code_size: u8, buf: &mut Vec<u8>) -> Result<(), DecodingError> {
        // `write_lzw_pre_encoded_frame` smuggles `min_code_size` in the first byte.
        buf.push(min_code_size);
        loop {
            match self.decoder.decode_next(&mut OutputBuffer::Vec(buf))? {
                Some(Decoded::LzwDataCopied(_len)) => {},
                Some(Decoded::DataEnd) => return Ok(()),
                _ => return Err(DecodingError::format("unexpected data")),
            }
        }
    }

    /// Reads data of the current frame into a pre-allocated buffer until the buffer has been
    /// filled completely.
    ///
    /// The buffer length must be an even number of pixels (multiple of 4 if decoding RGBA).
    ///
    /// `Self::next_frame_info` needs to be called beforehand. Returns `true` if the supplied
    /// buffer could be filled completely. Should not be called after `false` had been returned.
    pub fn fill_buffer(&mut self, buf: &mut [u8]) -> Result<bool, DecodingError> {
        self.pixel_converter.fill_buffer(&self.current_frame, buf, &mut |out| self.decoder.decode_next_bytes(out))
    }

    /// Output buffer size
    pub fn buffer_size(&self) -> usize {
        self.pixel_converter.buffer_size(&self.current_frame).unwrap()
    }

    /// Line length of the current frame
    pub fn line_length(&self) -> usize {
        self.pixel_converter.line_length(&self.current_frame)
    }

    /// Returns the color palette relevant for the frame that has been decoded
    #[inline]
    pub fn palette(&self) -> Result<&[u8], DecodingError> {
        Ok(match self.current_frame.palette {
            Some(ref table) => table,
            None => self.global_palette().ok_or(DecodingError::format(
                "no color table available for current frame",
            ))?,
        })
    }

    /// The global color palette
    pub fn global_palette(&self) -> Option<&[u8]> {
        self.pixel_converter.global_palette()
    }

    /// Width of the image
    #[inline]
    pub fn width(&self) -> u16 {
        self.decoder.decoder.width()
    }

    /// Height of the image
    #[inline]
    pub fn height(&self) -> u16 {
        self.decoder.decoder.height()
    }

    /// Abort decoding and recover the `io::Read` instance
    pub fn into_inner(self) -> io::BufReader<R> {
        self.decoder.into_inner()
    }

    /// Index of the background color in the global palette
    ///
    /// In practice this is not used, and the background is
    /// always transparent
    pub fn bg_color(&self) -> Option<usize> {
        self.bg_color.map(|v| v as usize)
    }

    /// Number of loop repetitions
    #[inline]
    pub fn repeat(&self) -> Repeat {
        self.repeat
    }
}

impl<R: Read> IntoIterator for Decoder<R> {
    type Item = Result<Frame<'static>, DecodingError>;
    type IntoIter = DecoderIter<R>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        DecoderIter {
            inner: self,
            ended: false,
        }
    }
}

/// Use `decoder.into_iter()` to iterate over the frames
pub struct DecoderIter<R: Read> {
    inner: Decoder<R>,
    ended: bool,
}

impl<R: Read> DecoderIter<R> {
    /// Abort decoding and recover the `io::Read` instance
    ///
    /// Use `for frame in iter.by_ref()` to be able to call this afterwards.
    pub fn into_inner(self) -> io::BufReader<R> {
        self.inner.into_inner()
    }
}

impl<R: Read> FusedIterator for DecoderIter<R> {}

impl<R: Read> Iterator for DecoderIter<R> {
    type Item = Result<Frame<'static>, DecodingError>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.ended {
            match self.inner.read_next_frame() {
                Ok(Some(_)) => self.inner.take_current_frame().map(Ok),
                Ok(None) => {
                    self.ended = true;
                    None
                },
                Err(err) => {
                    self.ended = true;
                    Some(Err(err))
                },
            }
        } else {
            None
        }
    }
}
