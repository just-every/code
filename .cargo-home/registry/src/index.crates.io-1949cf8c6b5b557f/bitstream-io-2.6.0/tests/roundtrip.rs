// Copyright 2017 Brian Langenberger
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate bitstream_io;
use bitstream_io::{BigEndian, BitRead, BitReader, BitWrite, BitWriter, LittleEndian};
use std::io::Cursor;

macro_rules! define_roundtrip {
    ($func_name:ident, $endianness:ident) => {
        #[test]
        fn $func_name() {
            /*unsigned values*/
            for bits in 1..17 {
                let max = 1 << bits;
                let mut output: Vec<u8> = Vec::with_capacity(max);
                {
                    let mut writer = BitWriter::endian(&mut output, $endianness);
                    for value in 0..max {
                        writer.write(bits, value as u32).unwrap();
                    }
                    writer.byte_align().unwrap();
                }
                {
                    let mut c = Cursor::new(&output);
                    let mut reader = BitReader::endian(&mut c, $endianness);
                    for value in 0..max {
                        assert_eq!(reader.read::<u32>(bits).unwrap(), value as u32);
                    }
                }
            }

            /*signed values*/
            for bits in 2..17 {
                let min = -1i32 << (bits - 1);
                let max = 1i32 << (bits - 1);
                let mut output: Vec<u8> = Vec::with_capacity(max as usize);
                {
                    let mut writer = BitWriter::endian(&mut output, $endianness);
                    for value in min..max {
                        writer.write_signed(bits, value as i32).unwrap();
                    }
                    writer.byte_align().unwrap();
                }
                {
                    let mut c = Cursor::new(&output);
                    let mut reader = BitReader::endian(&mut c, $endianness);
                    for value in min..max {
                        assert_eq!(reader.read_signed::<i32>(bits).unwrap(), value as i32);
                    }
                }
            }
        }
    };
}

define_roundtrip!(test_roundtrip_be, BigEndian);
define_roundtrip!(test_roundtrip_le, LittleEndian);

macro_rules! define_unary_roundtrip {
    ($func_name:ident, $endianness:ident) => {
        #[test]
        fn $func_name() {
            let mut output: Vec<u8> = Vec::new();
            {
                let mut writer = BitWriter::endian(&mut output, $endianness);
                for value in 0..1024 {
                    writer.write_unary0(value).unwrap();
                }
                writer.byte_align().unwrap();
            }
            {
                let mut c = Cursor::new(&output);
                let mut reader = BitReader::endian(&mut c, $endianness);
                for value in 0..1024 {
                    assert_eq!(reader.read_unary0().unwrap(), value);
                }
            }

            let mut output: Vec<u8> = Vec::new();
            {
                let mut writer = BitWriter::endian(&mut output, $endianness);
                for value in 0..1024 {
                    writer.write_unary1(value).unwrap();
                }
                writer.byte_align().unwrap();
            }
            {
                let mut c = Cursor::new(&output);
                let mut reader = BitReader::endian(&mut c, $endianness);
                for value in 0..1024 {
                    assert_eq!(reader.read_unary1().unwrap(), value);
                }
            }
        }
    };
}

define_unary_roundtrip!(test_unary_roundtrip_be, BigEndian);
define_unary_roundtrip!(test_unary_roundtrip_le, LittleEndian);

macro_rules! define_float_roundtrip {
    ($func_name:ident, $endianness:ident, $t:ty) => {
        #[test]
        fn $func_name() {
            let mut output: Vec<u8> = Vec::new();
            {
                let mut writer = BitWriter::endian(&mut output, $endianness);
                // these values should all be exact in floating-point
                for value in 0..1024 {
                    writer.write_from(value as $t).unwrap();
                }
                writer.byte_align().unwrap();
            }
            {
                let mut c = Cursor::new(&output);
                let mut reader = BitReader::endian(&mut c, $endianness);
                for value in 0..1024 {
                    assert_eq!(reader.read_to::<$t>().unwrap(), value as $t);
                }
            }
        }
    };
}

define_float_roundtrip!(test_f32_roundtrip_be, BigEndian, f32);
define_float_roundtrip!(test_f64_roundtrip_be, BigEndian, f64);
define_float_roundtrip!(test_f32_roundtrip_le, LittleEndian, f32);
define_float_roundtrip!(test_f64_roundtrip_le, LittleEndian, f64);
