// Copyright 2017 Brian Langenberger
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Traits and implementations for writing bits to a stream.
//!
//! ## Example
//!
//! Writing the initial STREAMINFO block to a FLAC file,
//! as documented in its
//! [specification](https://xiph.org/flac/format.html#stream).
//!
//! ```
//! use std::convert::TryInto;
//! use std::io::Write;
//! use bitstream_io::{BigEndian, BitWriter, BitWrite, ByteWriter, ByteWrite, LittleEndian, ToBitStream};
//!
//! #[derive(Debug, PartialEq, Eq)]
//! struct BlockHeader {
//!     last_block: bool,  // 1 bit
//!     block_type: u8,    // 7 bits
//!     block_size: u32,   // 24 bits
//! }
//!
//! impl ToBitStream for BlockHeader {
//!     type Error = std::io::Error;
//!
//!     fn to_writer<W: BitWrite + ?Sized>(&self, w: &mut W) -> std::io::Result<()> {
//!         w.write_bit(self.last_block)?;
//!         w.write_out::<7, _>(self.block_type)?;
//!         w.write_out::<24, _>(self.block_size)
//!     }
//! }
//!
//! #[derive(Debug, PartialEq, Eq)]
//! struct Streaminfo {
//!     minimum_block_size: u16,  // 16 bits
//!     maximum_block_size: u16,  // 16 bits
//!     minimum_frame_size: u32,  // 24 bits
//!     maximum_frame_size: u32,  // 24 bits
//!     sample_rate: u32,         // 20 bits
//!     channels: u8,             // 3 bits
//!     bits_per_sample: u8,      // 5 bits
//!     total_samples: u64,       // 36 bits
//!     md5: [u8; 16],            // 16 bytes
//! }
//!
//! impl ToBitStream for Streaminfo {
//!     type Error = std::io::Error;
//!
//!     fn to_writer<W: BitWrite + ?Sized>(&self, w: &mut W) -> std::io::Result<()> {
//!         w.write_from(self.minimum_block_size)?;
//!         w.write_from(self.maximum_block_size)?;
//!         w.write_out::<24, _>(self.minimum_frame_size)?;
//!         w.write_out::<24, _>(self.maximum_frame_size)?;
//!         w.write_out::<20, _>(self.sample_rate)?;
//!         w.write_out::<3,  _>(self.channels - 1)?;
//!         w.write_out::<5,  _>(self.bits_per_sample - 1)?;
//!         w.write_out::<36, _>(self.total_samples)?;
//!         w.write_bytes(&self.md5)
//!     }
//! }
//!
//! #[derive(Debug, PartialEq, Eq)]
//! struct VorbisComment {
//!     vendor: String,
//!     comment: Vec<String>,
//! }
//!
//! impl VorbisComment {
//!     fn len(&self) -> usize {
//!         4 + self.vendor.len() + 4 + self.comment.iter().map(|c| 4 + c.len()).sum::<usize>()
//!     }
//!
//!     fn write<W: std::io::Write>(&self, w: &mut ByteWriter<W, LittleEndian>) -> std::io::Result<()> {
//!         use std::convert::TryInto;
//!
//!         fn write_entry<W: std::io::Write>(
//!             w: &mut ByteWriter<W, LittleEndian>,
//!             s: &str,
//!         ) -> std::io::Result<()> {
//!             w.write::<u32>(s.len().try_into().unwrap())?;
//!             w.write_bytes(s.as_bytes())
//!         }
//!
//!         write_entry(w, &self.vendor)?;
//!         w.write::<u32>(self.comment.len().try_into().unwrap())?;
//!         self.comment.iter().try_for_each(|s| write_entry(w, s))
//!     }
//! }
//!
//! let mut flac: Vec<u8> = Vec::new();
//!
//! let mut writer = BitWriter::endian(&mut flac, BigEndian);
//!
//! // stream marker
//! writer.write_bytes(b"fLaC").unwrap();
//!
//! // metadata block header
//! writer.build(&BlockHeader { last_block: false, block_type: 0, block_size: 34 }).unwrap();
//!
//! // STREAMINFO block
//! writer.build(&Streaminfo {
//!     minimum_block_size: 4096,
//!     maximum_block_size: 4096,
//!     minimum_frame_size: 1542,
//!     maximum_frame_size: 8546,
//!     sample_rate: 44100,
//!     channels: 2,
//!     bits_per_sample: 16,
//!     total_samples: 304844,
//!     md5: *b"\xFA\xF2\x69\x2F\xFD\xEC\x2D\x5B\x30\x01\x76\xB4\x62\x88\x7D\x92",
//! }).unwrap();
//!
//! let comment = VorbisComment {
//!     vendor: "reference libFLAC 1.1.4 20070213".to_string(),
//!     comment: vec![
//!         "title=2ch 44100  16bit".to_string(),
//!         "album=Test Album".to_string(),
//!         "artist=Assorted".to_string(),
//!         "tracknumber=1".to_string(),
//!     ],
//! };
//!
//! // metadata block header
//! writer.build(
//!     &BlockHeader {
//!         last_block: false,
//!         block_type: 4,
//!         block_size: comment.len().try_into().unwrap(),
//!     }
//! ).unwrap();
//!
//! // VORBIS_COMMENT block (little endian)
//! comment.write(&mut ByteWriter::new(writer.writer().unwrap())).unwrap();
//!
//! assert_eq!(flac, vec![0x66,0x4c,0x61,0x43,0x00,0x00,0x00,0x22,
//!                       0x10,0x00,0x10,0x00,0x00,0x06,0x06,0x00,
//!                       0x21,0x62,0x0a,0xc4,0x42,0xf0,0x00,0x04,
//!                       0xa6,0xcc,0xfa,0xf2,0x69,0x2f,0xfd,0xec,
//!                       0x2d,0x5b,0x30,0x01,0x76,0xb4,0x62,0x88,
//!                       0x7d,0x92,0x04,0x00,0x00,0x7a,0x20,0x00,
//!                       0x00,0x00,0x72,0x65,0x66,0x65,0x72,0x65,
//!                       0x6e,0x63,0x65,0x20,0x6c,0x69,0x62,0x46,
//!                       0x4c,0x41,0x43,0x20,0x31,0x2e,0x31,0x2e,
//!                       0x34,0x20,0x32,0x30,0x30,0x37,0x30,0x32,
//!                       0x31,0x33,0x04,0x00,0x00,0x00,0x16,0x00,
//!                       0x00,0x00,0x74,0x69,0x74,0x6c,0x65,0x3d,
//!                       0x32,0x63,0x68,0x20,0x34,0x34,0x31,0x30,
//!                       0x30,0x20,0x20,0x31,0x36,0x62,0x69,0x74,
//!                       0x10,0x00,0x00,0x00,0x61,0x6c,0x62,0x75,
//!                       0x6d,0x3d,0x54,0x65,0x73,0x74,0x20,0x41,
//!                       0x6c,0x62,0x75,0x6d,0x0f,0x00,0x00,0x00,
//!                       0x61,0x72,0x74,0x69,0x73,0x74,0x3d,0x41,
//!                       0x73,0x73,0x6f,0x72,0x74,0x65,0x64,0x0d,
//!                       0x00,0x00,0x00,0x74,0x72,0x61,0x63,0x6b,
//!                       0x6e,0x75,0x6d,0x62,0x65,0x72,0x3d,0x31]);
//! ```

#![warn(missing_docs)]

#[cfg(feature = "alloc")]
use alloc::boxed::Box;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
#[cfg(feature = "alloc")]
use core2::io;

#[cfg(not(feature = "alloc"))]
use std::io;

use core::convert::From;
use core::ops::{AddAssign, Rem};

use super::{
    huffman::WriteHuffmanTree, BitQueue, Endianness, Numeric, PhantomData, Primitive, SignedNumeric,
};

/// For writing bit values to an underlying stream in a given endianness.
///
/// Because this only writes whole bytes to the underlying stream,
/// it is important that output is byte-aligned before the bitstream
/// writer's lifetime ends.
/// **Partial bytes will be lost** if the writer is disposed of
/// before they can be written.
pub struct BitWriter<W: io::Write, E: Endianness> {
    writer: W,
    bitqueue: BitQueue<E, u8>,
}

impl<W: io::Write, E: Endianness> BitWriter<W, E> {
    /// Wraps a BitWriter around something that implements `Write`
    pub fn new(writer: W) -> BitWriter<W, E> {
        BitWriter {
            writer,
            bitqueue: BitQueue::new(),
        }
    }

    /// Wraps a BitWriter around something that implements `Write`
    /// with the given endianness.
    pub fn endian(writer: W, _endian: E) -> BitWriter<W, E> {
        BitWriter {
            writer,
            bitqueue: BitQueue::new(),
        }
    }

    /// Unwraps internal writer and disposes of BitWriter.
    ///
    /// # Warning
    ///
    /// Any unwritten partial bits are discarded.
    #[inline]
    pub fn into_writer(self) -> W {
        self.writer
    }

    /// If stream is byte-aligned, provides mutable reference
    /// to internal writer.  Otherwise returns `None`
    #[inline]
    pub fn writer(&mut self) -> Option<&mut W> {
        if self.byte_aligned() {
            Some(&mut self.writer)
        } else {
            None
        }
    }

    /// Converts `BitWriter` to `ByteWriter` in the same endianness.
    ///
    /// # Warning
    ///
    /// Any written partial bits are discarded.
    #[inline]
    pub fn into_bytewriter(self) -> ByteWriter<W, E> {
        ByteWriter::new(self.into_writer())
    }

    /// If stream is byte-aligned, provides temporary `ByteWriter`
    /// in the same endianness.  Otherwise returns `None`
    ///
    /// # Warning
    ///
    /// Any unwritten bits left over when `ByteWriter` is dropped are lost.
    #[inline]
    pub fn bytewriter(&mut self) -> Option<ByteWriter<&mut W, E>> {
        self.writer().map(ByteWriter::new)
    }

    /// Consumes writer and returns any un-written partial byte
    /// as a `(bits, value)` tuple.
    ///
    /// # Examples
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, BitWriter, BitWrite};
    /// let mut data = Vec::new();
    /// let (bits, value) = {
    ///     let mut writer = BitWriter::endian(&mut data, BigEndian);
    ///     writer.write(15, 0b1010_0101_0101_101).unwrap();
    ///     writer.into_unwritten()
    /// };
    /// assert_eq!(data, [0b1010_0101]);
    /// assert_eq!(bits, 7);
    /// assert_eq!(value, 0b0101_101);
    /// ```
    ///
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, BitWriter, BitWrite};
    /// let mut data = Vec::new();
    /// let (bits, value) = {
    ///     let mut writer = BitWriter::endian(&mut data, BigEndian);
    ///     writer.write(8, 0b1010_0101).unwrap();
    ///     writer.into_unwritten()
    /// };
    /// assert_eq!(data, [0b1010_0101]);
    /// assert_eq!(bits, 0);
    /// assert_eq!(value, 0);
    /// ```
    #[inline(always)]
    pub fn into_unwritten(self) -> (u32, u8) {
        (self.bitqueue.len(), self.bitqueue.value())
    }

    /// Flushes output stream to disk, if necessary.
    /// Any partial bytes are not flushed.
    ///
    /// # Errors
    ///
    /// Passes along any errors from the underlying stream.
    #[inline(always)]
    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// A trait for anything that can write a variable number of
/// potentially un-aligned values to an output stream
pub trait BitWrite {
    /// Writes a single bit to the stream.
    /// `true` indicates 1, `false` indicates 0
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underlying stream.
    fn write_bit(&mut self, bit: bool) -> io::Result<()>;

    /// Writes an unsigned value to the stream using the given
    /// number of bits.
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underlying stream.
    /// Returns an error if the input type is too small
    /// to hold the given number of bits.
    /// Returns an error if the value is too large
    /// to fit the given number of bits.
    fn write<U>(&mut self, bits: u32, value: U) -> io::Result<()>
    where
        U: Numeric;

    /// Writes an unsigned value to the stream using the given
    /// const number of bits.
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underlying stream.
    /// Returns an error if the value is too large
    /// to fit the given number of bits.
    /// A compile-time error occurs if the given number of bits
    /// is larger than the output type.
    fn write_out<const BITS: u32, U>(&mut self, value: U) -> io::Result<()>
    where
        U: Numeric,
    {
        self.write(BITS, value)
    }

    /// Writes a twos-complement signed value to the stream
    /// with the given number of bits.
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underlying stream.
    /// Returns an error if the input type is too small
    /// to hold the given number of bits.
    /// Returns an error if the number of bits is 0,
    /// since one bit is always needed for the sign.
    /// Returns an error if the value is too large
    /// to fit the given number of bits.
    fn write_signed<S>(&mut self, bits: u32, value: S) -> io::Result<()>
    where
        S: SignedNumeric;

    /// Writes a twos-complement signed value to the stream
    /// with the given const number of bits.
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underlying stream.
    /// Returns an error if the value is too large
    /// to fit the given number of bits.
    /// A compile-time error occurs if the number of bits is 0,
    /// since one bit is always needed for the sign.
    /// A compile-time error occurs if the given number of bits
    /// is larger than the output type.
    fn write_signed_out<const BITS: u32, S>(&mut self, value: S) -> io::Result<()>
    where
        S: SignedNumeric,
    {
        self.write_signed(BITS, value)
    }

    /// Writes whole value to the stream whose size in bits
    /// is equal to its type's size.
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underlying stream.
    fn write_from<V>(&mut self, value: V) -> io::Result<()>
    where
        V: Primitive;

    /// Writes whole value to the stream whose size in bits
    /// is equal to its type's size in an endianness that may
    /// be different from the stream's endianness.
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underlying stream.
    fn write_as_from<F, V>(&mut self, value: V) -> io::Result<()>
    where
        F: Endianness,
        V: Primitive;

    /// Writes the entirety of a byte buffer to the stream.
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underlying stream.
    ///
    /// # Example
    ///
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), BigEndian);
    /// writer.write(8, 0x66).unwrap();
    /// writer.write(8, 0x6F).unwrap();
    /// writer.write(8, 0x6F).unwrap();
    /// writer.write_bytes(b"bar").unwrap();
    /// assert_eq!(writer.into_writer(), b"foobar");
    /// ```
    #[inline]
    fn write_bytes(&mut self, buf: &[u8]) -> io::Result<()> {
        buf.iter().try_for_each(|b| self.write_out::<8, _>(*b))
    }

    /// Writes `value` number of 1 bits to the stream
    /// and then writes a 0 bit.  This field is variably-sized.
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underyling stream.
    ///
    /// # Examples
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), BigEndian);
    /// writer.write_unary0(0).unwrap();
    /// writer.write_unary0(3).unwrap();
    /// writer.write_unary0(10).unwrap();
    /// assert_eq!(writer.into_writer(), [0b01110111, 0b11111110]);
    /// ```
    ///
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{LittleEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), LittleEndian);
    /// writer.write_unary0(0).unwrap();
    /// writer.write_unary0(3).unwrap();
    /// writer.write_unary0(10).unwrap();
    /// assert_eq!(writer.into_writer(), [0b11101110, 0b01111111]);
    /// ```
    fn write_unary0(&mut self, value: u32) -> io::Result<()> {
        match value {
            0 => self.write_bit(false),
            bits @ 1..=31 => self
                .write(value, (1u32 << bits) - 1)
                .and_then(|()| self.write_bit(false)),
            32 => self
                .write(value, 0xFFFF_FFFFu32)
                .and_then(|()| self.write_bit(false)),
            bits @ 33..=63 => self
                .write(value, (1u64 << bits) - 1)
                .and_then(|()| self.write_bit(false)),
            64 => self
                .write(value, 0xFFFF_FFFF_FFFF_FFFFu64)
                .and_then(|()| self.write_bit(false)),
            mut bits => {
                while bits > 64 {
                    self.write(64, 0xFFFF_FFFF_FFFF_FFFFu64)?;
                    bits -= 64;
                }
                self.write_unary0(bits)
            }
        }
    }

    /// Writes `value` number of 0 bits to the stream
    /// and then writes a 1 bit.  This field is variably-sized.
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underyling stream.
    ///
    /// # Example
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), BigEndian);
    /// writer.write_unary1(0).unwrap();
    /// writer.write_unary1(3).unwrap();
    /// writer.write_unary1(10).unwrap();
    /// assert_eq!(writer.into_writer(), [0b10001000, 0b00000001]);
    /// ```
    ///
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{LittleEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), LittleEndian);
    /// writer.write_unary1(0).unwrap();
    /// writer.write_unary1(3).unwrap();
    /// writer.write_unary1(10).unwrap();
    /// assert_eq!(writer.into_writer(), [0b00010001, 0b10000000]);
    /// ```
    fn write_unary1(&mut self, value: u32) -> io::Result<()> {
        match value {
            0 => self.write_bit(true),
            1..=32 => self.write(value, 0u32).and_then(|()| self.write_bit(true)),
            33..=64 => self.write(value, 0u64).and_then(|()| self.write_bit(true)),
            mut bits => {
                while bits > 64 {
                    self.write(64, 0u64)?;
                    bits -= 64;
                }
                self.write_unary1(bits)
            }
        }
    }

    /// Builds and writes complex type
    fn build<T: ToBitStream>(&mut self, build: &T) -> Result<(), T::Error> {
        build.to_writer(self)
    }

    /// Builds and writes complex type with context
    fn build_with<'a, T: ToBitStreamWith<'a>>(
        &mut self,
        build: &T,
        context: &T::Context,
    ) -> Result<(), T::Error> {
        build.to_writer(self, context)
    }

    /// Returns true if the stream is aligned at a whole byte.
    fn byte_aligned(&self) -> bool;

    /// Pads the stream with 0 bits until it is aligned at a whole byte.
    /// Does nothing if the stream is already aligned.
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underyling stream.
    ///
    /// # Example
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), BigEndian);
    /// writer.write(1, 0).unwrap();
    /// writer.byte_align().unwrap();
    /// writer.write(8, 0xFF).unwrap();
    /// assert_eq!(writer.into_writer(), [0x00, 0xFF]);
    /// ```
    fn byte_align(&mut self) -> io::Result<()> {
        while !self.byte_aligned() {
            self.write_bit(false)?;
        }
        Ok(())
    }
}

/// A trait for anything that can write Huffman codes
/// of a given endianness to an output stream
pub trait HuffmanWrite<E: Endianness> {
    /// Writes Huffman code for the given symbol to the stream.
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underlying stream.
    fn write_huffman<T>(&mut self, tree: &WriteHuffmanTree<E, T>, symbol: T) -> io::Result<()>
    where
        T: Ord + Copy;
}

impl<W: io::Write, E: Endianness> BitWrite for BitWriter<W, E> {
    /// # Examples
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), BigEndian);
    /// writer.write_bit(true).unwrap();
    /// writer.write_bit(false).unwrap();
    /// writer.write_bit(true).unwrap();
    /// writer.write_bit(true).unwrap();
    /// writer.write_bit(false).unwrap();
    /// writer.write_bit(true).unwrap();
    /// writer.write_bit(true).unwrap();
    /// writer.write_bit(true).unwrap();
    /// assert_eq!(writer.into_writer(), [0b10110111]);
    /// ```
    ///
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{LittleEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), LittleEndian);
    /// writer.write_bit(true).unwrap();
    /// writer.write_bit(true).unwrap();
    /// writer.write_bit(true).unwrap();
    /// writer.write_bit(false).unwrap();
    /// writer.write_bit(true).unwrap();
    /// writer.write_bit(true).unwrap();
    /// writer.write_bit(false).unwrap();
    /// writer.write_bit(true).unwrap();
    /// assert_eq!(writer.into_writer(), [0b10110111]);
    /// ```
    fn write_bit(&mut self, bit: bool) -> io::Result<()> {
        self.bitqueue.push(1, u8::from(bit));
        if self.bitqueue.is_full() {
            write_byte(&mut self.writer, self.bitqueue.pop(8))
        } else {
            Ok(())
        }
    }

    /// # Examples
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), BigEndian);
    /// writer.write(1, 0b1).unwrap();
    /// writer.write(2, 0b01).unwrap();
    /// writer.write(5, 0b10111).unwrap();
    /// assert_eq!(writer.into_writer(), [0b10110111]);
    /// ```
    ///
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{LittleEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), LittleEndian);
    /// writer.write(1, 0b1).unwrap();
    /// writer.write(2, 0b11).unwrap();
    /// writer.write(5, 0b10110).unwrap();
    /// assert_eq!(writer.into_writer(), [0b10110111]);
    /// ```
    ///
    /// ```
    /// use std::io::{Write, sink};
    /// use bitstream_io::{BigEndian, BitWriter, BitWrite};
    /// let mut w = BitWriter::endian(sink(), BigEndian);
    /// assert!(w.write(9, 0u8).is_err());    // can't write  u8 in 9 bits
    /// assert!(w.write(17, 0u16).is_err());  // can't write u16 in 17 bits
    /// assert!(w.write(33, 0u32).is_err());  // can't write u32 in 33 bits
    /// assert!(w.write(65, 0u64).is_err());  // can't write u64 in 65 bits
    /// assert!(w.write(1, 2).is_err());      // can't write   2 in 1 bit
    /// assert!(w.write(2, 4).is_err());      // can't write   4 in 2 bits
    /// assert!(w.write(3, 8).is_err());      // can't write   8 in 3 bits
    /// assert!(w.write(4, 16).is_err());     // can't write  16 in 4 bits
    /// ```
    fn write<U>(&mut self, bits: u32, value: U) -> io::Result<()>
    where
        U: Numeric,
    {
        if bits > U::BITS_SIZE {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "excessive bits for type written",
            ))
        } else if (bits < U::BITS_SIZE) && (value >= (U::ONE << bits)) {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "excessive value for bits written",
            ))
        } else if bits < self.bitqueue.remaining_len() {
            self.bitqueue.push(bits, value.to_u8());
            Ok(())
        } else {
            let mut acc = BitQueue::from_value(value, bits);
            write_unaligned(&mut self.writer, &mut acc, &mut self.bitqueue)?;
            write_aligned(&mut self.writer, &mut acc)?;
            self.bitqueue.push(acc.len(), acc.value().to_u8());
            Ok(())
        }
    }

    /// # Examples
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), BigEndian);
    /// writer.write_out::<1, _>(0b1).unwrap();
    /// writer.write_out::<2, _>(0b01).unwrap();
    /// writer.write_out::<5, _>(0b10111).unwrap();
    /// assert_eq!(writer.into_writer(), [0b10110111]);
    /// ```
    ///
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{LittleEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), LittleEndian);
    /// writer.write_out::<1, _>(0b1).unwrap();
    /// writer.write_out::<2, _>(0b11).unwrap();
    /// writer.write_out::<5, _>(0b10110).unwrap();
    /// assert_eq!(writer.into_writer(), [0b10110111]);
    /// ```
    ///
    /// ```
    /// use std::io::{Write, sink};
    /// use bitstream_io::{BigEndian, BitWriter, BitWrite};
    /// let mut w = BitWriter::endian(sink(), BigEndian);
    /// assert!(w.write_out::<1, _>(2).is_err());      // can't write   2 in 1 bit
    /// assert!(w.write_out::<2, _>(4).is_err());      // can't write   4 in 2 bits
    /// assert!(w.write_out::<3, _>(8).is_err());      // can't write   8 in 3 bits
    /// assert!(w.write_out::<4, _>(16).is_err());     // can't write  16 in 4 bits
    /// ```
    fn write_out<const BITS: u32, U>(&mut self, value: U) -> io::Result<()>
    where
        U: Numeric,
    {
        const {
            assert!(BITS <= U::BITS_SIZE, "excessive bits for type written");
        }

        if (BITS < U::BITS_SIZE) && (value >= (U::ONE << BITS)) {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "excessive value for bits written",
            ))
        } else if BITS < self.bitqueue.remaining_len() {
            self.bitqueue.push_fixed::<BITS>(value.to_u8());
            Ok(())
        } else {
            let mut acc = BitQueue::from_value(value, BITS);
            write_unaligned(&mut self.writer, &mut acc, &mut self.bitqueue)?;
            write_aligned(&mut self.writer, &mut acc)?;
            self.bitqueue.push(acc.len(), acc.value().to_u8());
            Ok(())
        }
    }

    /// # Examples
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), BigEndian);
    /// writer.write_signed(4, -5).unwrap();
    /// writer.write_signed(4, 7).unwrap();
    /// assert_eq!(writer.into_writer(), [0b10110111]);
    /// ```
    ///
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{LittleEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), LittleEndian);
    /// writer.write_signed(4, 7).unwrap();
    /// writer.write_signed(4, -5).unwrap();
    /// assert_eq!(writer.into_writer(), [0b10110111]);
    /// ```
    #[inline]
    fn write_signed<S>(&mut self, bits: u32, value: S) -> io::Result<()>
    where
        S: SignedNumeric,
    {
        match bits {
            0 => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "signed writes need at least 1 bit for sign",
            )),
            bits if bits <= S::BITS_SIZE => E::write_signed(self, bits, value),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "excessive bits for type written",
            )),
        }
    }

    /// # Examples
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), BigEndian);
    /// writer.write_signed_out::<4, _>(-5).unwrap();
    /// writer.write_signed_out::<4, _>(7).unwrap();
    /// assert_eq!(writer.into_writer(), [0b10110111]);
    /// ```
    ///
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{LittleEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(Vec::new(), LittleEndian);
    /// writer.write_signed_out::<4, _>(7).unwrap();
    /// writer.write_signed_out::<4, _>(-5).unwrap();
    /// assert_eq!(writer.into_writer(), [0b10110111]);
    /// ```
    #[inline]
    fn write_signed_out<const BITS: u32, S>(&mut self, value: S) -> io::Result<()>
    where
        S: SignedNumeric,
    {
        const {
            assert!(BITS > 0, "signed writes need at least 1 bit for sign");
            assert!(BITS <= S::BITS_SIZE, "excessive bits for type written");
        }
        E::write_signed_fixed::<_, BITS, S>(self, value)
    }

    #[inline]
    fn write_from<V>(&mut self, value: V) -> io::Result<()>
    where
        V: Primitive,
    {
        E::write_primitive(self, value)
    }

    #[inline]
    fn write_as_from<F, V>(&mut self, value: V) -> io::Result<()>
    where
        F: Endianness,
        V: Primitive,
    {
        F::write_primitive(self, value)
    }

    #[inline]
    fn write_bytes(&mut self, buf: &[u8]) -> io::Result<()> {
        if self.byte_aligned() {
            self.writer.write_all(buf)
        } else {
            buf.iter().try_for_each(|b| self.write_out::<8, _>(*b))
        }
    }

    /// # Example
    /// ```
    /// use std::io::{Write, sink};
    /// use bitstream_io::{BigEndian, BitWriter, BitWrite};
    /// let mut writer = BitWriter::endian(sink(), BigEndian);
    /// assert_eq!(writer.byte_aligned(), true);
    /// writer.write(1, 0).unwrap();
    /// assert_eq!(writer.byte_aligned(), false);
    /// writer.write(7, 0).unwrap();
    /// assert_eq!(writer.byte_aligned(), true);
    /// ```
    #[inline(always)]
    fn byte_aligned(&self) -> bool {
        self.bitqueue.is_empty()
    }
}

impl<W: io::Write, E: Endianness> HuffmanWrite<E> for BitWriter<W, E> {
    /// # Example
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, BitWriter, HuffmanWrite};
    /// use bitstream_io::huffman::compile_write_tree;
    /// let tree = compile_write_tree(
    ///     vec![('a', vec![0]),
    ///          ('b', vec![1, 0]),
    ///          ('c', vec![1, 1, 0]),
    ///          ('d', vec![1, 1, 1])]).unwrap();
    /// let mut writer = BitWriter::endian(Vec::new(), BigEndian);
    /// writer.write_huffman(&tree, 'b').unwrap();
    /// writer.write_huffman(&tree, 'c').unwrap();
    /// writer.write_huffman(&tree, 'd').unwrap();
    /// assert_eq!(writer.into_writer(), [0b10110111]);
    /// ```
    #[inline]
    fn write_huffman<T>(&mut self, tree: &WriteHuffmanTree<E, T>, symbol: T) -> io::Result<()>
    where
        T: Ord + Copy,
    {
        tree.get(&symbol)
            .try_for_each(|(bits, value)| self.write(*bits, *value))
    }
}

/// For counting the number of bits written but generating no output.
///
/// # Example
/// ```
/// use bitstream_io::{BigEndian, BitWrite, BitCounter};
/// let mut writer: BitCounter<u32, BigEndian> = BitCounter::new();
/// writer.write(1, 0b1).unwrap();
/// writer.write(2, 0b01).unwrap();
/// writer.write(5, 0b10111).unwrap();
/// assert_eq!(writer.written(), 8);
/// ```
#[derive(Default)]
pub struct BitCounter<N, E: Endianness> {
    bits: N,
    phantom: PhantomData<E>,
}

impl<N: Default + Copy, E: Endianness> BitCounter<N, E> {
    /// Creates new counter
    #[inline]
    pub fn new() -> Self {
        BitCounter {
            bits: N::default(),
            phantom: PhantomData,
        }
    }

    /// Returns number of bits written
    #[inline]
    pub fn written(&self) -> N {
        self.bits
    }
}

impl<N, E> BitWrite for BitCounter<N, E>
where
    E: Endianness,
    N: Copy + AddAssign + From<u32> + Rem<Output = N> + PartialEq,
{
    #[inline]
    fn write_bit(&mut self, _bit: bool) -> io::Result<()> {
        self.bits += 1.into();
        Ok(())
    }

    #[inline]
    fn write<U>(&mut self, bits: u32, value: U) -> io::Result<()>
    where
        U: Numeric,
    {
        if bits > U::BITS_SIZE {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "excessive bits for type written",
            ))
        } else if (bits < U::BITS_SIZE) && (value >= (U::ONE << bits)) {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "excessive value for bits written",
            ))
        } else {
            self.bits += bits.into();
            Ok(())
        }
    }

    fn write_out<const BITS: u32, U>(&mut self, value: U) -> io::Result<()>
    where
        U: Numeric,
    {
        const {
            assert!(BITS <= U::BITS_SIZE, "excessive bits for type written");
        }

        if (BITS < U::BITS_SIZE) && (value >= (U::ONE << BITS)) {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "excessive value for bits written",
            ))
        } else {
            self.bits += BITS.into();
            Ok(())
        }
    }

    #[inline]
    fn write_signed<S>(&mut self, bits: u32, value: S) -> io::Result<()>
    where
        S: SignedNumeric,
    {
        match bits {
            0 => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "signed writes need at least 1 bit for sign",
            )),
            bits if bits <= S::BITS_SIZE => E::write_signed(self, bits, value),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "excessive bits for type written",
            )),
        }
    }

    #[inline]
    fn write_signed_out<const BITS: u32, S>(&mut self, value: S) -> io::Result<()>
    where
        S: SignedNumeric,
    {
        const {
            assert!(BITS > 0, "signed writes need at least 1 bit for sign");
            assert!(BITS <= S::BITS_SIZE, "excessive bits for type written");
        }
        E::write_signed_fixed::<_, BITS, S>(self, value)
    }

    #[inline]
    fn write_from<V>(&mut self, value: V) -> io::Result<()>
    where
        V: Primitive,
    {
        E::write_primitive(self, value)
    }

    #[inline]
    fn write_as_from<F, V>(&mut self, value: V) -> io::Result<()>
    where
        F: Endianness,
        V: Primitive,
    {
        F::write_primitive(self, value)
    }

    #[inline]
    fn write_unary1(&mut self, value: u32) -> io::Result<()> {
        self.bits += (value + 1).into();
        Ok(())
    }

    #[inline]
    fn write_unary0(&mut self, value: u32) -> io::Result<()> {
        self.bits += (value + 1).into();
        Ok(())
    }

    #[inline]
    fn write_bytes(&mut self, buf: &[u8]) -> io::Result<()> {
        self.bits += (buf.len() as u32 * 8).into();
        Ok(())
    }

    #[inline]
    fn byte_aligned(&self) -> bool {
        self.bits % 8.into() == 0.into()
    }
}

impl<N, E> HuffmanWrite<E> for BitCounter<N, E>
where
    E: Endianness,
    N: AddAssign + From<u32>,
{
    fn write_huffman<T>(&mut self, tree: &WriteHuffmanTree<E, T>, symbol: T) -> io::Result<()>
    where
        T: Ord + Copy,
    {
        for &(bits, _) in tree.get(&symbol) {
            let bits: N = bits.into();
            self.bits += bits;
        }
        Ok(())
    }
}

/// A generic unsigned value for stream recording purposes
pub struct UnsignedValue(InnerUnsignedValue);

enum InnerUnsignedValue {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
}

macro_rules! define_unsigned_value {
    ($t:ty, $n:ident) => {
        impl From<$t> for UnsignedValue {
            #[inline]
            fn from(v: $t) -> Self {
                UnsignedValue(InnerUnsignedValue::$n(v))
            }
        }
    };
}
define_unsigned_value!(u8, U8);
define_unsigned_value!(u16, U16);
define_unsigned_value!(u32, U32);
define_unsigned_value!(u64, U64);
define_unsigned_value!(u128, U128);
define_unsigned_value!(i8, I8);
define_unsigned_value!(i16, I16);
define_unsigned_value!(i32, I32);
define_unsigned_value!(i64, I64);
define_unsigned_value!(i128, I128);

/// A generic signed value for stream recording purposes
pub struct SignedValue(InnerSignedValue);

enum InnerSignedValue {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
}

macro_rules! define_signed_value {
    ($t:ty, $n:ident) => {
        impl From<$t> for SignedValue {
            #[inline]
            fn from(v: $t) -> Self {
                SignedValue(InnerSignedValue::$n(v))
            }
        }
    };
}
define_signed_value!(i8, I8);
define_signed_value!(i16, I16);
define_signed_value!(i32, I32);
define_signed_value!(i64, I64);
define_signed_value!(i128, I128);

enum WriteRecord {
    Bit(bool),
    Unsigned { bits: u32, value: UnsignedValue },
    Signed { bits: u32, value: SignedValue },
    Unary0(u32),
    Unary1(u32),
    Bytes(Box<[u8]>),
}

impl WriteRecord {
    fn playback<W: BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        match self {
            WriteRecord::Bit(v) => writer.write_bit(*v),
            WriteRecord::Unsigned {
                bits,
                value: UnsignedValue(value),
            } => match value {
                InnerUnsignedValue::U8(v) => writer.write(*bits, *v),
                InnerUnsignedValue::U16(v) => writer.write(*bits, *v),
                InnerUnsignedValue::U32(v) => writer.write(*bits, *v),
                InnerUnsignedValue::U64(v) => writer.write(*bits, *v),
                InnerUnsignedValue::U128(v) => writer.write(*bits, *v),
                InnerUnsignedValue::I8(v) => writer.write(*bits, *v),
                InnerUnsignedValue::I16(v) => writer.write(*bits, *v),
                InnerUnsignedValue::I32(v) => writer.write(*bits, *v),
                InnerUnsignedValue::I64(v) => writer.write(*bits, *v),
                InnerUnsignedValue::I128(v) => writer.write(*bits, *v),
            },
            WriteRecord::Signed {
                bits,
                value: SignedValue(value),
            } => match value {
                InnerSignedValue::I8(v) => writer.write_signed(*bits, *v),
                InnerSignedValue::I16(v) => writer.write_signed(*bits, *v),
                InnerSignedValue::I32(v) => writer.write_signed(*bits, *v),
                InnerSignedValue::I64(v) => writer.write_signed(*bits, *v),
                InnerSignedValue::I128(v) => writer.write_signed(*bits, *v),
            },
            WriteRecord::Unary0(v) => writer.write_unary0(*v),
            WriteRecord::Unary1(v) => writer.write_unary1(*v),
            WriteRecord::Bytes(bytes) => writer.write_bytes(bytes),
        }
    }
}

/// For recording writes in order to play them back on another writer
/// # Example
/// ```
/// use std::io::Write;
/// use bitstream_io::{BigEndian, BitWriter, BitWrite, BitRecorder};
/// let mut recorder: BitRecorder<u32, BigEndian> = BitRecorder::new();
/// recorder.write(1, 0b1).unwrap();
/// recorder.write(2, 0b01).unwrap();
/// recorder.write(5, 0b10111).unwrap();
/// assert_eq!(recorder.written(), 8);
/// let mut writer = BitWriter::endian(Vec::new(), BigEndian);
/// recorder.playback(&mut writer);
/// assert_eq!(writer.into_writer(), [0b10110111]);
/// ```
#[derive(Default)]
pub struct BitRecorder<N, E: Endianness> {
    counter: BitCounter<N, E>,
    records: Vec<WriteRecord>,
}

impl<N: Default + Copy, E: Endianness> BitRecorder<N, E> {
    /// Creates new recorder
    #[inline]
    pub fn new() -> Self {
        BitRecorder {
            counter: BitCounter::new(),
            records: Vec::new(),
        }
    }

    /// Creates new recorder sized for the given number of writes
    #[inline]
    pub fn with_capacity(writes: usize) -> Self {
        BitRecorder {
            counter: BitCounter::new(),
            records: Vec::with_capacity(writes),
        }
    }

    /// Creates new recorder with the given endianness
    #[inline]
    pub fn endian(_endian: E) -> Self {
        BitRecorder {
            counter: BitCounter::new(),
            records: Vec::new(),
        }
    }

    /// Returns number of bits written
    #[inline]
    pub fn written(&self) -> N {
        self.counter.written()
    }

    /// Plays recorded writes to the given writer
    #[inline]
    pub fn playback<W: BitWrite>(&self, writer: &mut W) -> io::Result<()> {
        self.records
            .iter()
            .try_for_each(|record| record.playback(writer))
    }
}

impl<N, E> BitWrite for BitRecorder<N, E>
where
    E: Endianness,
    N: Copy + From<u32> + AddAssign + Rem<Output = N> + Eq,
{
    #[inline]
    fn write_bit(&mut self, bit: bool) -> io::Result<()> {
        self.records.push(WriteRecord::Bit(bit));
        self.counter.write_bit(bit)
    }

    #[inline]
    fn write<U>(&mut self, bits: u32, value: U) -> io::Result<()>
    where
        U: Numeric,
    {
        self.counter.write(bits, value)?;
        self.records.push(WriteRecord::Unsigned {
            bits,
            value: value.unsigned_value(),
        });
        Ok(())
    }

    #[inline]
    fn write_out<const BITS: u32, U>(&mut self, value: U) -> io::Result<()>
    where
        U: Numeric,
    {
        self.counter.write_out::<BITS, U>(value)?;
        self.records.push(WriteRecord::Unsigned {
            bits: BITS,
            value: value.unsigned_value(),
        });
        Ok(())
    }

    #[inline]
    fn write_signed<S>(&mut self, bits: u32, value: S) -> io::Result<()>
    where
        S: SignedNumeric,
    {
        self.counter.write_signed(bits, value)?;
        self.records.push(WriteRecord::Signed {
            bits,
            value: value.signed_value(),
        });
        Ok(())
    }

    #[inline]
    fn write_signed_out<const BITS: u32, S>(&mut self, value: S) -> io::Result<()>
    where
        S: SignedNumeric,
    {
        self.counter.write_signed_out::<BITS, S>(value)?;
        self.records.push(WriteRecord::Signed {
            bits: BITS,
            value: value.signed_value(),
        });
        Ok(())
    }

    #[inline]
    fn write_from<V>(&mut self, value: V) -> io::Result<()>
    where
        V: Primitive,
    {
        E::write_primitive(self, value)
    }

    #[inline]
    fn write_as_from<F, V>(&mut self, value: V) -> io::Result<()>
    where
        F: Endianness,
        V: Primitive,
    {
        F::write_primitive(self, value)
    }

    #[inline]
    fn write_unary0(&mut self, value: u32) -> io::Result<()> {
        self.records.push(WriteRecord::Unary0(value));
        self.counter.write_unary0(value)
    }

    #[inline]
    fn write_unary1(&mut self, value: u32) -> io::Result<()> {
        self.records.push(WriteRecord::Unary1(value));
        self.counter.write_unary1(value)
    }

    #[inline]
    fn write_bytes(&mut self, buf: &[u8]) -> io::Result<()> {
        self.records.push(WriteRecord::Bytes(buf.into()));
        self.counter.write_bytes(buf)
    }

    #[inline]
    fn byte_aligned(&self) -> bool {
        self.counter.byte_aligned()
    }
}

impl<N, E> HuffmanWrite<E> for BitRecorder<N, E>
where
    E: Endianness,
    N: Copy + From<u32> + AddAssign + Rem<Output = N> + Eq,
{
    #[inline]
    fn write_huffman<T>(&mut self, tree: &WriteHuffmanTree<E, T>, symbol: T) -> io::Result<()>
    where
        T: Ord + Copy,
    {
        tree.get(&symbol)
            .try_for_each(|(bits, value)| self.write(*bits, *value))
    }
}

#[inline]
fn write_byte<W>(mut writer: W, byte: u8) -> io::Result<()>
where
    W: io::Write,
{
    writer.write_all(core::slice::from_ref(&byte))
}

fn write_unaligned<W, E, N>(
    writer: W,
    acc: &mut BitQueue<E, N>,
    rem: &mut BitQueue<E, u8>,
) -> io::Result<()>
where
    W: io::Write,
    E: Endianness,
    N: Numeric,
{
    if rem.is_empty() {
        Ok(())
    } else {
        use core::cmp::min;
        let bits_to_transfer = min(8 - rem.len(), acc.len());
        rem.push(bits_to_transfer, acc.pop(bits_to_transfer).to_u8());
        if rem.len() == 8 {
            write_byte(writer, rem.pop(8))
        } else {
            Ok(())
        }
    }
}

fn write_aligned<W, E, N>(mut writer: W, acc: &mut BitQueue<E, N>) -> io::Result<()>
where
    W: io::Write,
    E: Endianness,
    N: Numeric,
{
    let to_write = (acc.len() / 8) as usize;
    if to_write > 0 {
        let mut buf = N::buffer();
        let buf_ref: &mut [u8] = buf.as_mut();
        for b in buf_ref[0..to_write].iter_mut() {
            *b = acc.pop_fixed::<8>().to_u8();
        }
        writer.write_all(&buf_ref[0..to_write])
    } else {
        Ok(())
    }
}

/// For writing aligned bytes to a stream of bytes in a given endianness.
///
/// This only writes aligned values and maintains no internal state.
pub struct ByteWriter<W: io::Write, E: Endianness> {
    phantom: PhantomData<E>,
    writer: W,
}

impl<W: io::Write, E: Endianness> ByteWriter<W, E> {
    /// Wraps a ByteWriter around something that implements `Write`
    pub fn new(writer: W) -> ByteWriter<W, E> {
        ByteWriter {
            phantom: PhantomData,
            writer,
        }
    }

    /// Wraps a BitWriter around something that implements `Write`
    /// with the given endianness.
    pub fn endian(writer: W, _endian: E) -> ByteWriter<W, E> {
        ByteWriter {
            phantom: PhantomData,
            writer,
        }
    }

    /// Unwraps internal writer and disposes of `ByteWriter`.
    /// Any unwritten partial bits are discarded.
    #[inline]
    pub fn into_writer(self) -> W {
        self.writer
    }

    /// Provides mutable reference to internal writer.
    #[inline]
    pub fn writer(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Converts `ByteWriter` to `BitWriter` in the same endianness.
    #[inline]
    pub fn into_bitwriter(self) -> BitWriter<W, E> {
        BitWriter::new(self.into_writer())
    }

    /// Provides temporary `BitWriter` in the same endianness.
    ///
    /// # Warning
    ///
    /// Any unwritten bits left over when `BitWriter` is dropped are lost.
    #[inline]
    pub fn bitwriter(&mut self) -> BitWriter<&mut W, E> {
        BitWriter::new(self.writer())
    }
}

/// A trait for anything that can write aligned values to an output stream
pub trait ByteWrite {
    /// Writes whole numeric value to stream
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underlying stream.
    /// # Examples
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, ByteWriter, ByteWrite};
    /// let mut writer = ByteWriter::endian(Vec::new(), BigEndian);
    /// writer.write(0b0000000011111111u16).unwrap();
    /// assert_eq!(writer.into_writer(), [0b00000000, 0b11111111]);
    /// ```
    ///
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{LittleEndian, ByteWriter, ByteWrite};
    /// let mut writer = ByteWriter::endian(Vec::new(), LittleEndian);
    /// writer.write(0b0000000011111111u16).unwrap();
    /// assert_eq!(writer.into_writer(), [0b11111111, 0b00000000]);
    /// ```
    fn write<V>(&mut self, value: V) -> io::Result<()>
    where
        V: Primitive;

    /// Writes whole numeric value to stream in a potentially different endianness
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underlying stream.
    ///
    /// # Examples
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, ByteWriter, ByteWrite, LittleEndian};
    /// let mut writer = ByteWriter::endian(Vec::new(), BigEndian);
    /// writer.write_as::<LittleEndian, u16>(0b0000000011111111).unwrap();
    /// assert_eq!(writer.into_writer(), [0b11111111, 0b00000000]);
    /// ```
    ///
    /// ```
    /// use std::io::Write;
    /// use bitstream_io::{BigEndian, ByteWriter, ByteWrite, LittleEndian};
    /// let mut writer = ByteWriter::endian(Vec::new(), LittleEndian);
    /// writer.write_as::<BigEndian, u16>(0b0000000011111111).unwrap();
    /// assert_eq!(writer.into_writer(), [0b00000000, 0b11111111]);
    /// ```
    fn write_as<F, V>(&mut self, value: V) -> io::Result<()>
    where
        F: Endianness,
        V: Primitive;

    /// Writes the entirety of a byte buffer to the stream.
    ///
    /// # Errors
    ///
    /// Passes along any I/O error from the underlying stream.
    fn write_bytes(&mut self, buf: &[u8]) -> io::Result<()>;

    /// Builds and writes complex type
    fn build<T: ToByteStream>(&mut self, build: &T) -> Result<(), T::Error> {
        build.to_writer(self)
    }

    /// Builds and writes complex type with context
    fn build_with<'a, T: ToByteStreamWith<'a>>(
        &mut self,
        build: &T,
        context: &T::Context,
    ) -> Result<(), T::Error> {
        build.to_writer(self, context)
    }

    /// Returns mutable reference to underlying writer
    fn writer_ref(&mut self) -> &mut dyn io::Write;
}

impl<W: io::Write, E: Endianness> ByteWrite for ByteWriter<W, E> {
    #[inline]
    fn write<V>(&mut self, value: V) -> io::Result<()>
    where
        V: Primitive,
    {
        E::write_numeric(&mut self.writer, value)
    }

    #[inline]
    fn write_as<F, V>(&mut self, value: V) -> io::Result<()>
    where
        F: Endianness,
        V: Primitive,
    {
        F::write_numeric(&mut self.writer, value)
    }

    #[inline]
    fn write_bytes(&mut self, buf: &[u8]) -> io::Result<()> {
        self.writer.write_all(buf)
    }

    #[inline]
    fn writer_ref(&mut self) -> &mut dyn io::Write {
        &mut self.writer
    }
}

/// Implemented by complex types that don't require any additional context
/// to build themselves to a writer
///
/// # Example
/// ```
/// use std::io::{Cursor, Read};
/// use bitstream_io::{BigEndian, BitWrite, BitWriter, ToBitStream};
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct BlockHeader {
///     last_block: bool,
///     block_type: u8,
///     block_size: u32,
/// }
///
/// impl ToBitStream for BlockHeader {
///     type Error = std::io::Error;
///
///     fn to_writer<W: BitWrite + ?Sized>(&self, w: &mut W) -> std::io::Result<()> {
///         w.write_bit(self.last_block)?;
///         w.write(7, self.block_type)?;
///         w.write(24, self.block_size)
///     }
/// }
///
/// let mut data = Vec::new();
/// let mut writer = BitWriter::endian(&mut data, BigEndian);
/// writer.build(&BlockHeader { last_block: false, block_type: 4, block_size: 122 }).unwrap();
/// assert_eq!(data, b"\x04\x00\x00\x7A");
/// ```
pub trait ToBitStream {
    /// Error generated during building, such as `io::Error`
    type Error;

    /// Generate self to writer
    fn to_writer<W: BitWrite + ?Sized>(&self, w: &mut W) -> Result<(), Self::Error>
    where
        Self: Sized;
}

/// Implemented by complex types that require additional context
/// to build themselves to a writer
pub trait ToBitStreamWith<'a> {
    /// Some context to use when writing
    type Context: 'a;

    /// Error generated during building, such as `io::Error`
    type Error;

    /// Generate self to writer
    fn to_writer<W: BitWrite + ?Sized>(
        &self,
        w: &mut W,
        context: &Self::Context,
    ) -> Result<(), Self::Error>
    where
        Self: Sized;
}

/// Implemented by complex types that don't require any additional context
/// to build themselves to a writer
pub trait ToByteStream {
    /// Error generated during building, such as `io::Error`
    type Error;

    /// Generate self to writer
    fn to_writer<W: ByteWrite + ?Sized>(&self, w: &mut W) -> Result<(), Self::Error>
    where
        Self: Sized;
}

/// Implemented by complex types that require additional context
/// to build themselves to a writer
pub trait ToByteStreamWith<'a> {
    /// Some context to use when writing
    type Context: 'a;

    /// Error generated during building, such as `io::Error`
    type Error;

    /// Generate self to writer
    fn to_writer<W: ByteWrite + ?Sized>(
        &self,
        w: &mut W,
        context: &Self::Context,
    ) -> Result<(), Self::Error>
    where
        Self: Sized;
}
