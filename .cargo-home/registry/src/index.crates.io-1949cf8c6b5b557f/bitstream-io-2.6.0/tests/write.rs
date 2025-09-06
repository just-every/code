// Copyright 2017 Brian Langenberger
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate bitstream_io;

#[test]
fn test_write_queue_be() {
    use bitstream_io::{BitQueue, Numeric, BE};
    let mut q: BitQueue<BE, u8> = BitQueue::new();
    let mut v = BitQueue::<BE, u8>::from_value(0b10u8, 2);
    q.push(2, v.pop(2).to_u8());
    let mut v = BitQueue::<BE, u8>::from_value(0b110u8, 3);
    q.push(3, v.pop(3).to_u8());
    let mut v = BitQueue::<BE, u8>::from_value(0b001_11u8, 5);
    q.push(3, v.pop(3).to_u8());
    assert_eq!(q.len(), 8);
    assert_eq!(q.pop(8), 0b10_110_001);
    q.push(2, v.pop(2).to_u8());
    let mut v = BitQueue::<BE, u8>::from_value(0b101u8, 3);
    q.push(3, v.pop(3).to_u8());
    let mut v = BitQueue::<BE, u32>::from_value(0b101_00111011_11000001, 19);
    q.push(3, v.pop(3).to_u8());
    assert_eq!(q.len(), 8);
    assert_eq!(q.pop(8), 0b11_101_101);
    q.push(8, v.pop(8).to_u8());
    assert_eq!(q.len(), 8);
    assert_eq!(q.pop(8), 0b00111011);
    q.push(8, v.pop(8).to_u8());
    assert_eq!(q.len(), 8);
    assert_eq!(q.pop(8), 0b11000001);
    assert!(v.is_empty());
    assert!(q.is_empty());
}

#[test]
fn test_write_queue_edge_be() {
    use bitstream_io::{BitQueue, BE};

    let mut q: BitQueue<BE, u8> = BitQueue::from_value(0, 0);
    q.push(8, 0b11111111);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<BE, u8> = BitQueue::from_value(0b1, 1);
    q.push(7, 0b1111111);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<BE, u8> = BitQueue::from_value(0b11, 2);
    q.push(6, 0b111111);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<BE, u8> = BitQueue::from_value(0b111, 3);
    q.push(5, 0b11111);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<BE, u8> = BitQueue::from_value(0b1111, 4);
    q.push(4, 0b1111);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<BE, u8> = BitQueue::from_value(0b11111, 5);
    q.push(3, 0b111);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<BE, u8> = BitQueue::from_value(0b111111, 6);
    q.push(2, 0b11);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<BE, u8> = BitQueue::from_value(0b1111111, 7);
    q.push(1, 0b1);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<BE, u8> = BitQueue::from_value(0b11111111, 8);
    q.push(0, 0);
    assert_eq!(q.pop(8), 0b11111111);
}

#[test]
fn test_write_queue_le() {
    use bitstream_io::{BitQueue, Numeric, LE};
    let mut q: BitQueue<LE, u8> = BitQueue::new();
    let mut v = BitQueue::<LE, u8>::from_value(0b01u8, 2);
    q.push(2, v.pop(2).to_u8());
    let mut v = BitQueue::<LE, u8>::from_value(0b100u8, 3);
    q.push(3, v.pop(3).to_u8());
    let mut v = BitQueue::<LE, u8>::from_value(0b01_101u8, 5);
    q.push(3, v.pop(3).to_u8());
    assert_eq!(q.len(), 8);
    assert_eq!(q.pop(8), 0b101_100_01);
    q.push(2, v.pop(2).to_u8());
    let mut v = BitQueue::<LE, u8>::from_value(0b011u8, 3);
    q.push(3, v.pop(3).to_u8());
    let mut v = BitQueue::<LE, u32>::from_value(0b11000001_00111011_111, 19);
    q.push(3, v.pop(3).to_u8());
    assert_eq!(q.len(), 8);
    assert_eq!(q.pop(8), 0b111_011_01);
    q.push(8, v.pop(8).to_u8());
    assert_eq!(q.len(), 8);
    assert_eq!(q.pop(8), 0b00111011);
    q.push(8, v.pop(8).to_u8());
    assert_eq!(q.len(), 8);
    assert_eq!(q.pop(8), 0b11000001);
    assert!(v.is_empty());
    assert!(q.is_empty());
}

#[test]
fn test_write_queue_edge_le() {
    use bitstream_io::{BitQueue, LE};

    let mut q: BitQueue<LE, u8> = BitQueue::from_value(0, 0);
    q.push(8, 0b11111111);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<LE, u8> = BitQueue::from_value(0b1, 1);
    q.push(7, 0b1111111);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<LE, u8> = BitQueue::from_value(0b11, 2);
    q.push(6, 0b111111);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<LE, u8> = BitQueue::from_value(0b111, 3);
    q.push(5, 0b11111);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<LE, u8> = BitQueue::from_value(0b1111, 4);
    q.push(4, 0b1111);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<LE, u8> = BitQueue::from_value(0b11111, 5);
    q.push(3, 0b111);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<LE, u8> = BitQueue::from_value(0b111111, 6);
    q.push(2, 0b11);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<LE, u8> = BitQueue::from_value(0b1111111, 7);
    q.push(1, 0b1);
    assert_eq!(q.pop(8), 0b11111111);

    let mut q: BitQueue<LE, u8> = BitQueue::from_value(0b11111111, 8);
    q.push(0, 0);
    assert_eq!(q.pop(8), 0b11111111);
}

#[test]
fn test_writer_be() {
    use bitstream_io::{BigEndian, BitWrite, BitWriter};

    let final_data: [u8; 4] = [0xB1, 0xED, 0x3B, 0xC1];

    /*writing individual bits*/
    let mut w = BitWriter::endian(Vec::with_capacity(2), BigEndian);
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data[0..2]);

    /*writing unsigned values*/
    let mut w = BitWriter::endian(Vec::with_capacity(4), BigEndian);
    assert!(w.byte_aligned());
    w.write(2, 2u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(3, 6u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(5, 7u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(3, 5u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(19, 0x53BC1u32).unwrap();
    assert!(w.byte_aligned());
    assert_eq!(w.into_writer().as_slice(), &final_data);

    let mut w = BitWriter::endian(Vec::with_capacity(4), BigEndian);
    assert!(w.byte_aligned());
    w.write_out::<2, u32>(2).unwrap();
    assert!(!w.byte_aligned());
    w.write_out::<3, u32>(6).unwrap();
    assert!(!w.byte_aligned());
    w.write_out::<5, u32>(7).unwrap();
    assert!(!w.byte_aligned());
    w.write_out::<3, u32>(5).unwrap();
    assert!(!w.byte_aligned());
    w.write_out::<19, u32>(0x53BC1).unwrap();
    assert!(w.byte_aligned());
    assert_eq!(w.into_writer().as_slice(), &final_data);

    /*writing signed values*/
    let mut w = BitWriter::endian(Vec::with_capacity(4), BigEndian);
    w.write_signed(2, -2).unwrap();
    w.write_signed(3, -2).unwrap();
    w.write_signed(5, 7).unwrap();
    w.write_signed(3, -3).unwrap();
    w.write_signed(19, -181311).unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data);

    let mut w = BitWriter::endian(Vec::with_capacity(4), BigEndian);
    w.write_signed_out::<2, i32>(-2).unwrap();
    w.write_signed_out::<3, i32>(-2).unwrap();
    w.write_signed_out::<5, i32>(7).unwrap();
    w.write_signed_out::<3, i32>(-3).unwrap();
    w.write_signed_out::<19, i32>(-181311).unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data);

    /*writing unary 0 values*/
    let mut w = BitWriter::endian(Vec::with_capacity(4), BigEndian);
    w.write_unary0(1).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(4).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(1).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(3).unwrap();
    w.write_unary0(4).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write(1, 1u32).unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data);

    /*writing unary 1 values*/
    let mut w = BitWriter::endian(Vec::with_capacity(4), BigEndian);
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(3).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(2).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(5).unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data);

    /*byte aligning*/
    let aligned_data = [0xA0, 0xE0, 0x3B, 0xC0];
    let mut w = BitWriter::endian(Vec::with_capacity(4), BigEndian);
    w.write(3, 5u32).unwrap();
    w.byte_align().unwrap();
    w.write(3, 7u32).unwrap();
    w.byte_align().unwrap();
    w.byte_align().unwrap();
    w.write(8, 59u32).unwrap();
    w.byte_align().unwrap();
    w.write(4, 12u32).unwrap();
    w.byte_align().unwrap();
    assert_eq!(w.into_writer().as_slice(), &aligned_data);

    /*writing bytes, aligned*/
    let final_data = [0xB1, 0xED];
    let mut w = BitWriter::endian(Vec::with_capacity(2), BigEndian);
    w.write_bytes(b"\xB1\xED").unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data);

    /*writing bytes, un-aligned*/
    let final_data = [0xBB, 0x1E, 0xD0];
    let mut w = BitWriter::endian(Vec::with_capacity(3), BigEndian);
    w.write(4, 11u32).unwrap();
    w.write_bytes(b"\xB1\xED").unwrap();
    w.byte_align().unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data);
}

#[test]
fn test_writer_edge_cases_be() {
    use bitstream_io::{BigEndian, BitWrite, BitWriter};

    // 0 bit writes
    let mut w = BitWriter::endian(Vec::new(), BigEndian);
    w.write(0, 0u8).unwrap();
    w.write(0, 0u16).unwrap();
    w.write(0, 0u32).unwrap();
    w.write(0, 0u64).unwrap();
    assert!(w.into_writer().is_empty());

    let mut w = BitWriter::endian(Vec::new(), BigEndian);
    assert!(w.write_signed(0, 0i8).is_err());
    assert!(w.write_signed(0, 0i16).is_err());
    assert!(w.write_signed(0, 0i32).is_err());
    assert!(w.write_signed(0, 0i64).is_err());
    assert!(w.into_writer().is_empty());

    let final_data: Vec<u8> = vec![
        0, 0, 0, 0, 255, 255, 255, 255, 128, 0, 0, 0, 127, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0,
        255, 255, 255, 255, 255, 255, 255, 255, 128, 0, 0, 0, 0, 0, 0, 0, 127, 255, 255, 255, 255,
        255, 255, 255,
    ];

    /*unsigned 32 and 64-bit values*/
    let mut w = BitWriter::endian(Vec::with_capacity(48), BigEndian);
    w.write(32, 0u32).unwrap();
    w.write(32, 4294967295u32).unwrap();
    w.write(32, 2147483648u32).unwrap();
    w.write(32, 2147483647u32).unwrap();
    w.write(64, 0u64).unwrap();
    w.write(64, 0xFFFFFFFFFFFFFFFFu64).unwrap();
    w.write(64, 9223372036854775808u64).unwrap();
    w.write(64, 9223372036854775807u64).unwrap();
    assert_eq!(w.into_writer(), final_data);

    let mut w = BitWriter::endian(Vec::with_capacity(48), BigEndian);
    w.write_out::<32, u32>(0).unwrap();
    w.write_out::<32, u32>(4294967295).unwrap();
    w.write_out::<32, u32>(2147483648).unwrap();
    w.write_out::<32, u32>(2147483647).unwrap();
    w.write_out::<64, u64>(0).unwrap();
    w.write_out::<64, u64>(0xFFFFFFFFFFFFFFFF).unwrap();
    w.write_out::<64, u64>(9223372036854775808).unwrap();
    w.write_out::<64, u64>(9223372036854775807).unwrap();
    assert_eq!(w.into_writer(), final_data);

    /*signed 32 and 64-bit values*/
    let mut w = BitWriter::endian(Vec::with_capacity(48), BigEndian);
    w.write(32, 0i32).unwrap();
    w.write(32, -1i32).unwrap();
    w.write(32, -2147483648i32).unwrap();
    w.write(32, 2147483647i32).unwrap();
    w.write(64, 0i64).unwrap();
    w.write(64, -1i64).unwrap();
    w.write(64, -9223372036854775808i64).unwrap();
    w.write(64, 9223372036854775807i64).unwrap();
    assert_eq!(w.into_writer(), final_data);

    let mut w = BitWriter::endian(Vec::with_capacity(48), BigEndian);
    w.write_out::<32, i32>(0).unwrap();
    w.write_out::<32, i32>(-1).unwrap();
    w.write_out::<32, i32>(-2147483648).unwrap();
    w.write_out::<32, i32>(2147483647).unwrap();
    w.write_out::<64, i64>(0).unwrap();
    w.write_out::<64, i64>(-1).unwrap();
    w.write_out::<64, i64>(-9223372036854775808).unwrap();
    w.write_out::<64, i64>(9223372036854775807).unwrap();
    assert_eq!(w.into_writer(), final_data);

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, BigEndian)
            .write_signed(8, std::i8::MAX)
            .unwrap();
    }
    assert_eq!(bytes, std::i8::MAX.to_be_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, BigEndian)
            .write_signed(8, std::i8::MIN)
            .unwrap();
    }
    assert_eq!(bytes, std::i8::MIN.to_be_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, BigEndian)
            .write_signed(16, std::i16::MAX)
            .unwrap();
    }
    assert_eq!(bytes, std::i16::MAX.to_be_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, BigEndian)
            .write_signed(16, std::i16::MIN)
            .unwrap();
    }
    assert_eq!(bytes, std::i16::MIN.to_be_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, BigEndian)
            .write_signed(32, std::i32::MAX)
            .unwrap();
    }
    assert_eq!(bytes, std::i32::MAX.to_be_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, BigEndian)
            .write_signed(32, std::i32::MIN)
            .unwrap();
    }
    assert_eq!(bytes, std::i32::MIN.to_be_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, BigEndian)
            .write_signed(64, std::i64::MAX)
            .unwrap();
    }
    assert_eq!(bytes, std::i64::MAX.to_be_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, BigEndian)
            .write_signed(64, std::i64::MIN)
            .unwrap();
    }
    assert_eq!(bytes, std::i64::MIN.to_be_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, BigEndian)
            .write_signed(128, std::i128::MAX)
            .unwrap();
    }
    assert_eq!(bytes, std::i128::MAX.to_be_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, BigEndian)
            .write_signed(128, std::i128::MIN)
            .unwrap();
    }
    assert_eq!(bytes, std::i128::MIN.to_be_bytes());
}

#[test]
fn test_writer_huffman_be() {
    use bitstream_io::huffman::compile_write_tree;
    use bitstream_io::{BigEndian, BitWrite, BitWriter, HuffmanWrite};

    let final_data: [u8; 4] = [0xB1, 0xED, 0x3B, 0xC1];
    let tree = compile_write_tree(vec![
        (0, vec![1, 1]),
        (1, vec![1, 0]),
        (2, vec![0, 1]),
        (3, vec![0, 0, 1]),
        (4, vec![0, 0, 0]),
    ])
    .unwrap();
    let mut w = BitWriter::endian(Vec::with_capacity(4), BigEndian);
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 4).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 4).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.byte_align().unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data);
}

#[test]
fn test_writer_le() {
    use bitstream_io::{BitWrite, BitWriter, LittleEndian};

    let final_data: [u8; 4] = [0xB1, 0xED, 0x3B, 0xC1];

    /*writing individual bits*/
    let mut w = BitWriter::endian(Vec::with_capacity(2), LittleEndian);
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data[0..2]);

    /*writing unsigned values*/
    let mut w = BitWriter::endian(Vec::with_capacity(4), LittleEndian);
    assert!(w.byte_aligned());
    w.write(2, 1u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(3, 4u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(5, 13u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(3, 3u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(19, 0x609DFu32).unwrap();
    assert!(w.byte_aligned());
    assert_eq!(w.into_writer().as_slice(), &final_data);

    let mut w = BitWriter::endian(Vec::with_capacity(4), LittleEndian);
    assert!(w.byte_aligned());
    w.write_out::<2, u32>(1).unwrap();
    assert!(!w.byte_aligned());
    w.write_out::<3, u32>(4).unwrap();
    assert!(!w.byte_aligned());
    w.write_out::<5, u32>(13).unwrap();
    assert!(!w.byte_aligned());
    w.write_out::<3, u32>(3).unwrap();
    assert!(!w.byte_aligned());
    w.write_out::<19, u32>(0x609DF).unwrap();
    assert!(w.byte_aligned());
    assert_eq!(w.into_writer().as_slice(), &final_data);

    /*writing signed values*/
    let mut w = BitWriter::endian(Vec::with_capacity(4), LittleEndian);
    w.write_signed(2, 1).unwrap();
    w.write_signed(3, -4).unwrap();
    w.write_signed(5, 13).unwrap();
    w.write_signed(3, 3).unwrap();
    w.write_signed(19, -128545).unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data);

    let mut w = BitWriter::endian(Vec::with_capacity(4), LittleEndian);
    w.write_signed_out::<2, i32>(1).unwrap();
    w.write_signed_out::<3, i32>(-4).unwrap();
    w.write_signed_out::<5, i32>(13).unwrap();
    w.write_signed_out::<3, i32>(3).unwrap();
    w.write_signed_out::<19, i32>(-128545).unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data);

    /*writing unary 0 values*/
    let mut w = BitWriter::endian(Vec::with_capacity(4), LittleEndian);
    w.write_unary0(1).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(5).unwrap();
    w.write_unary0(3).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(1).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write(2, 3u32).unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data);

    /*writing unary 1 values*/
    let mut w = BitWriter::endian(Vec::with_capacity(4), LittleEndian);
    w.write_unary1(0).unwrap();
    w.write_unary1(3).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(2).unwrap();
    w.write_unary1(5).unwrap();
    w.write_unary1(0).unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data);

    /*byte aligning*/
    let aligned_data = [0x05, 0x07, 0x3B, 0x0C];
    let mut w = BitWriter::endian(Vec::with_capacity(4), LittleEndian);
    w.write(3, 5u32).unwrap();
    w.byte_align().unwrap();
    w.write(3, 7u32).unwrap();
    w.byte_align().unwrap();
    w.byte_align().unwrap();
    w.write(8, 59u32).unwrap();
    w.byte_align().unwrap();
    w.write(4, 12u32).unwrap();
    w.byte_align().unwrap();
    assert_eq!(w.into_writer().as_slice(), &aligned_data);

    /*writing bytes, aligned*/
    let final_data = [0xB1, 0xED];
    let mut w = BitWriter::endian(Vec::with_capacity(2), LittleEndian);
    w.write_bytes(b"\xB1\xED").unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data);

    /*writing bytes, un-aligned*/
    let final_data = [0x1B, 0xDB, 0x0E];
    let mut w = BitWriter::endian(Vec::with_capacity(3), LittleEndian);
    w.write(4, 11u32).unwrap();
    w.write_bytes(b"\xB1\xED").unwrap();
    w.byte_align().unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data);
}

#[test]
fn test_writer_edge_cases_le() {
    use bitstream_io::{BitWrite, BitWriter, LittleEndian};

    // 0 bit writes
    let mut w = BitWriter::endian(Vec::new(), LittleEndian);
    w.write(0, 0u8).unwrap();
    w.write(0, 0u16).unwrap();
    w.write(0, 0u32).unwrap();
    w.write(0, 0u64).unwrap();
    assert!(w.into_writer().is_empty());

    let mut w = BitWriter::endian(Vec::new(), LittleEndian);
    assert!(w.write_signed(0, 0i8).is_err());
    assert!(w.write_signed(0, 0i16).is_err());
    assert!(w.write_signed(0, 0i32).is_err());
    assert!(w.write_signed(0, 0i64).is_err());
    assert!(w.into_writer().is_empty());

    let final_data: Vec<u8> = vec![
        0, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 128, 255, 255, 255, 127, 0, 0, 0, 0, 0, 0, 0, 0,
        255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 128, 255, 255, 255, 255, 255,
        255, 255, 127,
    ];

    /*unsigned 32 and 64-bit values*/
    let mut w = BitWriter::endian(Vec::with_capacity(48), LittleEndian);
    w.write(32, 0u32).unwrap();
    w.write(32, 4294967295u32).unwrap();
    w.write(32, 2147483648u32).unwrap();
    w.write(32, 2147483647u32).unwrap();
    w.write(64, 0u64).unwrap();
    w.write(64, 0xFFFFFFFFFFFFFFFFu64).unwrap();
    w.write(64, 9223372036854775808u64).unwrap();
    w.write(64, 9223372036854775807u64).unwrap();
    assert_eq!(w.into_writer(), final_data);

    let mut w = BitWriter::endian(Vec::with_capacity(48), LittleEndian);
    w.write_out::<32, u32>(0).unwrap();
    w.write_out::<32, u32>(4294967295).unwrap();
    w.write_out::<32, u32>(2147483648).unwrap();
    w.write_out::<32, u32>(2147483647).unwrap();
    w.write_out::<64, u64>(0).unwrap();
    w.write_out::<64, u64>(0xFFFFFFFFFFFFFFFF).unwrap();
    w.write_out::<64, u64>(9223372036854775808).unwrap();
    w.write_out::<64, u64>(9223372036854775807).unwrap();
    assert_eq!(w.into_writer(), final_data);

    /*signed 32 and 64-bit values*/
    let mut w = BitWriter::endian(Vec::with_capacity(48), LittleEndian);
    w.write(32, 0i32).unwrap();
    w.write(32, -1i32).unwrap();
    w.write(32, -2147483648i32).unwrap();
    w.write(32, 2147483647i32).unwrap();
    w.write(64, 0i64).unwrap();
    w.write(64, -1i64).unwrap();
    w.write(64, -9223372036854775808i64).unwrap();
    w.write(64, 9223372036854775807i64).unwrap();
    assert_eq!(w.into_writer(), final_data);

    let mut w = BitWriter::endian(Vec::with_capacity(48), LittleEndian);
    w.write_out::<32, i32>(0).unwrap();
    w.write_out::<32, i32>(-1).unwrap();
    w.write_out::<32, i32>(-2147483648).unwrap();
    w.write_out::<32, i32>(2147483647).unwrap();
    w.write_out::<64, i64>(0).unwrap();
    w.write_out::<64, i64>(-1).unwrap();
    w.write_out::<64, i64>(-9223372036854775808).unwrap();
    w.write_out::<64, i64>(9223372036854775807).unwrap();
    assert_eq!(w.into_writer(), final_data);

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, LittleEndian)
            .write_signed(8, std::i8::MAX)
            .unwrap();
    }
    assert_eq!(bytes, std::i8::MAX.to_le_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, LittleEndian)
            .write_signed(8, std::i8::MIN)
            .unwrap();
    }
    assert_eq!(bytes, std::i8::MIN.to_le_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, LittleEndian)
            .write_signed(16, std::i16::MAX)
            .unwrap();
    }
    assert_eq!(bytes, std::i16::MAX.to_le_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, LittleEndian)
            .write_signed(16, std::i16::MIN)
            .unwrap();
    }
    assert_eq!(bytes, std::i16::MIN.to_le_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, LittleEndian)
            .write_signed(32, std::i32::MAX)
            .unwrap();
    }
    assert_eq!(bytes, std::i32::MAX.to_le_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, LittleEndian)
            .write_signed(32, std::i32::MIN)
            .unwrap();
    }
    assert_eq!(bytes, std::i32::MIN.to_le_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, LittleEndian)
            .write_signed(64, std::i64::MAX)
            .unwrap();
    }
    assert_eq!(bytes, std::i64::MAX.to_le_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, LittleEndian)
            .write_signed(64, std::i64::MIN)
            .unwrap();
    }
    assert_eq!(bytes, std::i64::MIN.to_le_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, LittleEndian)
            .write_signed(128, std::i128::MAX)
            .unwrap();
    }
    assert_eq!(bytes, std::i128::MAX.to_le_bytes());

    let mut bytes = Vec::new();
    {
        BitWriter::endian(&mut bytes, LittleEndian)
            .write_signed(128, std::i128::MIN)
            .unwrap();
    }
    assert_eq!(bytes, std::i128::MIN.to_le_bytes());
}

#[test]
fn test_writer_huffman_le() {
    use bitstream_io::huffman::compile_write_tree;
    use bitstream_io::{BitWrite, BitWriter, HuffmanWrite, LittleEndian};

    let final_data: [u8; 4] = [0xB1, 0xED, 0x3B, 0xC1];
    let tree = compile_write_tree(vec![
        (0, vec![1, 1]),
        (1, vec![1, 0]),
        (2, vec![0, 1]),
        (3, vec![0, 0, 1]),
        (4, vec![0, 0, 0]),
    ])
    .unwrap();
    let mut w = BitWriter::endian(Vec::with_capacity(4), LittleEndian);
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 3).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 4).unwrap();
    w.write_huffman(&tree, 3).unwrap();
    w.write(1, 1).unwrap();
    assert_eq!(w.into_writer().as_slice(), &final_data);
}

struct LimitedWriter {
    can_write: usize,
}

impl LimitedWriter {
    fn new(max_bytes: usize) -> LimitedWriter {
        LimitedWriter {
            can_write: max_bytes,
        }
    }
}

impl std::io::Write for LimitedWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize, std::io::Error> {
        use std::cmp::min;

        let to_write = min(buf.len(), self.can_write);
        self.can_write -= to_write;
        Ok(to_write)
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        Ok(())
    }
}

#[test]
fn test_writer_io_errors_be() {
    use bitstream_io::{BigEndian, BitWrite, BitWriter};
    use std::io::ErrorKind;

    /*individual bits*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), BigEndian);
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(false).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(false).is_ok());
    assert!(w.write_bit(false).is_ok());
    assert!(w.write_bit(false).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(false).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(false).is_ok());
    assert_eq!(w.write_bit(true).unwrap_err().kind(), ErrorKind::WriteZero);

    /*unsigned values*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), BigEndian);
    assert!(w.write(2, 2u32).is_ok());
    assert!(w.write(3, 6u32).is_ok());
    assert!(w.write(5, 7u32).is_ok());
    assert!(w.write(3, 5u32).is_ok());
    assert_eq!(
        w.write(19, 0x53BC1u32).unwrap_err().kind(),
        ErrorKind::WriteZero
    );

    /*signed values*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), BigEndian);
    assert!(w.write_signed(2, -2).is_ok());
    assert!(w.write_signed(3, -2).is_ok());
    assert!(w.write_signed(5, 7).is_ok());
    assert!(w.write_signed(3, -3).is_ok());
    assert_eq!(
        w.write_signed(19, -181311).unwrap_err().kind(),
        ErrorKind::WriteZero
    );

    /*unary 0 values*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), BigEndian);
    assert!(w.write_unary0(1).is_ok());
    assert!(w.write_unary0(2).is_ok());
    assert!(w.write_unary0(0).is_ok());
    assert!(w.write_unary0(0).is_ok());
    assert!(w.write_unary0(4).is_ok());
    assert!(w.write_unary0(2).is_ok());
    assert_eq!(w.write_unary0(1).unwrap_err().kind(), ErrorKind::WriteZero);

    /*unary 1 values*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), BigEndian);
    assert!(w.write_unary1(0).is_ok());
    assert!(w.write_unary1(1).is_ok());
    assert!(w.write_unary1(0).is_ok());
    assert!(w.write_unary1(3).is_ok());
    assert!(w.write_unary1(0).is_ok());
    assert!(w.write_unary1(0).is_ok());
    assert!(w.write_unary1(0).is_ok());
    assert!(w.write_unary1(1).is_ok());
    assert!(w.write_unary1(0).is_ok());
    assert_eq!(w.write_unary1(1).unwrap_err().kind(), ErrorKind::WriteZero);

    /*byte aligning*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), BigEndian);
    assert!(w.write::<u16>(9, 0b111111111).is_ok());
    assert_eq!(w.byte_align().unwrap_err().kind(), ErrorKind::WriteZero);

    /*aligned bytes*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), BigEndian);
    assert_eq!(
        w.write_bytes(b"\xB1\xED").unwrap_err().kind(),
        ErrorKind::WriteZero
    );

    /*un-aligned bytes*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), BigEndian);
    assert!(w.write(4, 11).is_ok());
    assert_eq!(
        w.write_bytes(b"\xB1\xED").unwrap_err().kind(),
        ErrorKind::WriteZero
    );
}

#[test]
fn test_writer_io_errors_le() {
    use bitstream_io::{BitWrite, BitWriter, LittleEndian};
    use std::io::ErrorKind;

    /*individual bits*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), LittleEndian);
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(false).is_ok());
    assert!(w.write_bit(false).is_ok());
    assert!(w.write_bit(false).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(false).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(false).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(false).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert!(w.write_bit(true).is_ok());
    assert_eq!(w.write_bit(true).unwrap_err().kind(), ErrorKind::WriteZero);

    /*unsigned values*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), LittleEndian);
    assert!(w.write(2, 1u32).is_ok());
    assert!(w.write(3, 4u32).is_ok());
    assert!(w.write(5, 13u32).is_ok());
    assert!(w.write(3, 3u32).is_ok());
    assert_eq!(
        w.write(19, 0x609DFu32).unwrap_err().kind(),
        ErrorKind::WriteZero
    );

    /*signed values*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), LittleEndian);
    assert!(w.write_signed(2, 1).is_ok());
    assert!(w.write_signed(3, -4).is_ok());
    assert!(w.write_signed(5, 13).is_ok());
    assert!(w.write_signed(3, 3).is_ok());
    assert_eq!(
        w.write_signed(19, -128545).unwrap_err().kind(),
        ErrorKind::WriteZero
    );

    /*unary 0 values*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), LittleEndian);
    assert!(w.write_unary0(1).is_ok());
    assert!(w.write_unary0(0).is_ok());
    assert!(w.write_unary0(0).is_ok());
    assert!(w.write_unary0(2).is_ok());
    assert!(w.write_unary0(2).is_ok());
    assert!(w.write_unary0(2).is_ok());
    assert_eq!(w.write_unary0(5).unwrap_err().kind(), ErrorKind::WriteZero);

    /*unary 1 values*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), LittleEndian);
    assert!(w.write_unary1(0).is_ok());
    assert!(w.write_unary1(3).is_ok());
    assert!(w.write_unary1(0).is_ok());
    assert!(w.write_unary1(1).is_ok());
    assert!(w.write_unary1(0).is_ok());
    assert!(w.write_unary1(1).is_ok());
    assert!(w.write_unary1(0).is_ok());
    assert!(w.write_unary1(1).is_ok());
    assert!(w.write_unary1(0).is_ok());
    assert_eq!(w.write_unary1(1).unwrap_err().kind(), ErrorKind::WriteZero);

    /*byte aligning*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), LittleEndian);
    assert!(w.write::<u16>(9, 0b111111111).is_ok());
    assert_eq!(w.byte_align().unwrap_err().kind(), ErrorKind::WriteZero);

    /*aligned bytes*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), LittleEndian);
    assert_eq!(
        w.write_bytes(b"\xB1\xED").unwrap_err().kind(),
        ErrorKind::WriteZero
    );

    /*un-aligned bytes*/
    let mut w = BitWriter::endian(LimitedWriter::new(1), LittleEndian);
    assert!(w.write(4, 11).is_ok());
    assert_eq!(
        w.write_bytes(b"\xB1\xED").unwrap_err().kind(),
        ErrorKind::WriteZero
    );
}

#[test]
fn test_writer_bits_errors() {
    use bitstream_io::{BigEndian, BitWrite, BitWriter, LittleEndian};
    use std::io::{sink, ErrorKind};

    let mut w = BitWriter::endian(sink(), BigEndian);
    assert_eq!(w.write(9, 0u8).unwrap_err().kind(), ErrorKind::InvalidInput);
    assert_eq!(
        w.write(17, 0u16).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        w.write(33, 0u32).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        w.write(65, 0u64).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );

    assert_eq!(
        w.write(1, 0b10).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        w.write(2, 0b100).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        w.write(3, 0b1000).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );

    for bits in 1..8 {
        let val = 1u8 << bits;
        assert_eq!(
            w.write(bits, val).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }
    for bits in 8..16 {
        let val = 1u16 << bits;
        assert_eq!(
            w.write(bits, val).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }
    for bits in 16..32 {
        let val = 1u32 << bits;
        assert_eq!(
            w.write(bits, val).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }
    for bits in 32..64 {
        let val = 1u64 << bits;
        assert_eq!(
            w.write(bits, val).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }

    assert_eq!(
        w.write_signed(9, 0i8).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        w.write_signed(17, 0i16).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        w.write_signed(33, 0i32).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        w.write_signed(65, 0i64).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );

    let mut w = BitWriter::endian(sink(), LittleEndian);
    assert_eq!(w.write(9, 0u8).unwrap_err().kind(), ErrorKind::InvalidInput);
    assert_eq!(
        w.write(17, 0u16).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        w.write(33, 0u32).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        w.write(65, 0u64).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );

    assert_eq!(
        w.write(1, 0b10).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        w.write(2, 0b100).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        w.write(3, 0b1000).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );

    for bits in 1..8 {
        let val = 1u8 << bits;
        assert_eq!(
            w.write(bits, val).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }
    for bits in 8..16 {
        let val = 1u16 << bits;
        assert_eq!(
            w.write(bits, val).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }
    for bits in 16..32 {
        let val = 1u32 << bits;
        assert_eq!(
            w.write(bits, val).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }
    for bits in 32..64 {
        let val = 1u64 << bits;
        assert_eq!(
            w.write(bits, val).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }

    assert_eq!(
        w.write_signed(9, 0i8).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        w.write_signed(17, 0i16).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        w.write_signed(33, 0i32).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        w.write_signed(65, 0i64).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
}

#[test]
fn test_counter_be() {
    use bitstream_io::{BigEndian, BitCounter, BitWrite};

    /*writing individual bits*/
    let mut w: BitCounter<u32, BigEndian> = BitCounter::new();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    assert_eq!(w.written(), 16);

    /*writing unsigned values*/
    let mut w: BitCounter<u32, BigEndian> = BitCounter::new();
    assert!(w.byte_aligned());
    w.write(2, 2u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(3, 6u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(5, 7u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(3, 5u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(19, 0x53BC1u32).unwrap();
    assert!(w.byte_aligned());
    assert_eq!(w.written(), 32);

    /*writing signed values*/
    let mut w: BitCounter<u32, BigEndian> = BitCounter::new();
    w.write_signed(2, -2).unwrap();
    w.write_signed(3, -2).unwrap();
    w.write_signed(5, 7).unwrap();
    w.write_signed(3, -3).unwrap();
    w.write_signed(19, -181311).unwrap();
    assert_eq!(w.written(), 32);

    /*writing unary 0 values*/
    let mut w: BitCounter<u32, BigEndian> = BitCounter::new();
    w.write_unary0(1).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(4).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(1).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(3).unwrap();
    w.write_unary0(4).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write(1, 1u32).unwrap();
    assert_eq!(w.written(), 32);

    /*writing unary 1 values*/
    let mut w: BitCounter<u32, BigEndian> = BitCounter::new();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(3).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(2).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(5).unwrap();
    assert_eq!(w.written(), 32);

    /*byte aligning*/
    let mut w: BitCounter<u32, BigEndian> = BitCounter::new();
    w.write(3, 5u32).unwrap();
    w.byte_align().unwrap();
    w.write(3, 7u32).unwrap();
    w.byte_align().unwrap();
    w.byte_align().unwrap();
    w.write(8, 59u32).unwrap();
    w.byte_align().unwrap();
    w.write(4, 12u32).unwrap();
    w.byte_align().unwrap();
    assert_eq!(w.written(), 32);

    /*writing bytes, aligned*/
    let mut w: BitCounter<u32, BigEndian> = BitCounter::new();
    w.write_bytes(b"\xB1\xED").unwrap();
    assert_eq!(w.written(), 16);

    /*writing bytes, un-aligned*/
    let mut w: BitCounter<u32, BigEndian> = BitCounter::new();
    w.write(4, 11u32).unwrap();
    w.write_bytes(b"\xB1\xED").unwrap();
    w.byte_align().unwrap();
    assert_eq!(w.written(), 24);
}

#[test]
fn test_counter_huffman_be() {
    use bitstream_io::huffman::compile_write_tree;
    use bitstream_io::{BigEndian, BitCounter, BitWrite, HuffmanWrite};

    let tree = compile_write_tree(vec![
        (0, vec![1, 1]),
        (1, vec![1, 0]),
        (2, vec![0, 1]),
        (3, vec![0, 0, 1]),
        (4, vec![0, 0, 0]),
    ])
    .unwrap();
    let mut w: BitCounter<u32, BigEndian> = BitCounter::new();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 4).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 4).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.byte_align().unwrap();
    assert_eq!(w.written(), 32);
}

#[test]
fn test_counter_le() {
    use bitstream_io::{BitCounter, BitWrite, LittleEndian};

    /*writing individual bits*/
    let mut w: BitCounter<u32, LittleEndian> = BitCounter::new();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    assert_eq!(w.written(), 16);

    /*writing unsigned values*/
    let mut w: BitCounter<u32, LittleEndian> = BitCounter::new();
    assert!(w.byte_aligned());
    w.write(2, 1u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(3, 4u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(5, 13u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(3, 3u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(19, 0x609DFu32).unwrap();
    assert!(w.byte_aligned());
    assert_eq!(w.written(), 32);

    /*writing signed values*/
    let mut w: BitCounter<u32, LittleEndian> = BitCounter::new();
    w.write_signed(2, 1).unwrap();
    w.write_signed(3, -4).unwrap();
    w.write_signed(5, 13).unwrap();
    w.write_signed(3, 3).unwrap();
    w.write_signed(19, -128545).unwrap();
    assert_eq!(w.written(), 32);

    /*writing unary 0 values*/
    let mut w: BitCounter<u32, LittleEndian> = BitCounter::new();
    w.write_unary0(1).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(5).unwrap();
    w.write_unary0(3).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(1).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write(2, 3u32).unwrap();
    assert_eq!(w.written(), 32);

    /*writing unary 1 values*/
    let mut w: BitCounter<u32, LittleEndian> = BitCounter::new();
    w.write_unary1(0).unwrap();
    w.write_unary1(3).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(2).unwrap();
    w.write_unary1(5).unwrap();
    w.write_unary1(0).unwrap();
    assert_eq!(w.written(), 32);

    /*byte aligning*/
    let mut w: BitCounter<u32, LittleEndian> = BitCounter::new();
    w.write(3, 5u32).unwrap();
    w.byte_align().unwrap();
    w.write(3, 7u32).unwrap();
    w.byte_align().unwrap();
    w.byte_align().unwrap();
    w.write(8, 59u32).unwrap();
    w.byte_align().unwrap();
    w.write(4, 12u32).unwrap();
    w.byte_align().unwrap();
    assert_eq!(w.written(), 32);

    /*writing bytes, aligned*/
    let mut w: BitCounter<u32, LittleEndian> = BitCounter::new();
    w.write_bytes(b"\xB1\xED").unwrap();
    assert_eq!(w.written(), 16);

    /*writing bytes, un-aligned*/
    let mut w: BitCounter<u32, LittleEndian> = BitCounter::new();
    w.write(4, 11u32).unwrap();
    w.write_bytes(b"\xB1\xED").unwrap();
    w.byte_align().unwrap();
    assert_eq!(w.written(), 24);
}

#[test]
fn test_counter_huffman_le() {
    use bitstream_io::huffman::compile_write_tree;
    use bitstream_io::{BitCounter, BitWrite, HuffmanWrite, LittleEndian};

    let tree = compile_write_tree(vec![
        (0, vec![1, 1]),
        (1, vec![1, 0]),
        (2, vec![0, 1]),
        (3, vec![0, 0, 1]),
        (4, vec![0, 0, 0]),
    ])
    .unwrap();
    let mut w: BitCounter<u32, LittleEndian> = BitCounter::new();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 3).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 4).unwrap();
    w.write_huffman(&tree, 3).unwrap();
    w.write(1, 1).unwrap();
    assert_eq!(w.written(), 32);
}

#[test]
fn test_recorder_be() {
    use bitstream_io::{BigEndian, BitRecorder, BitWrite, BitWriter};

    let final_data: [u8; 4] = [0xB1, 0xED, 0x3B, 0xC1];

    /*writing individual bits*/
    let mut w: BitRecorder<u32, BigEndian> = BitRecorder::new();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    assert_eq!(w.written(), 16);
    let mut w2 = BitWriter::endian(Vec::with_capacity(2), BigEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data[0..2]);

    /*writing unsigned values*/
    let mut w: BitRecorder<u32, BigEndian> = BitRecorder::new();
    assert!(w.byte_aligned());
    w.write(2, 2u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(3, 6u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(5, 7u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(3, 5u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(19, 0x53BC1u32).unwrap();
    assert!(w.byte_aligned());
    assert_eq!(w.written(), 32);
    let mut w2 = BitWriter::endian(Vec::with_capacity(4), BigEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data);

    /*writing signed values*/
    let mut w: BitRecorder<u32, BigEndian> = BitRecorder::new();
    w.write_signed(2, -2).unwrap();
    w.write_signed(3, -2).unwrap();
    w.write_signed(5, 7).unwrap();
    w.write_signed(3, -3).unwrap();
    w.write_signed(19, -181311).unwrap();
    assert_eq!(w.written(), 32);
    let mut w2 = BitWriter::endian(Vec::with_capacity(4), BigEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data);

    /*writing unary 0 values*/
    let mut w: BitRecorder<u32, BigEndian> = BitRecorder::new();
    w.write_unary0(1).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(4).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(1).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(3).unwrap();
    w.write_unary0(4).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write(1, 1u32).unwrap();
    assert_eq!(w.written(), 32);
    let mut w2 = BitWriter::endian(Vec::with_capacity(4), BigEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data);

    /*writing unary 1 values*/
    let mut w: BitRecorder<u32, BigEndian> = BitRecorder::new();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(3).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(2).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(5).unwrap();
    assert_eq!(w.written(), 32);
    let mut w2 = BitWriter::endian(Vec::with_capacity(4), BigEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data);

    /*byte aligning*/
    let aligned_data = [0xA0, 0xE0, 0x3B, 0xC0];
    let mut w: BitRecorder<u32, BigEndian> = BitRecorder::new();
    w.write(3, 5u32).unwrap();
    w.byte_align().unwrap();
    w.write(3, 7u32).unwrap();
    w.byte_align().unwrap();
    w.byte_align().unwrap();
    w.write(8, 59u32).unwrap();
    w.byte_align().unwrap();
    w.write(4, 12u32).unwrap();
    w.byte_align().unwrap();
    assert_eq!(w.written(), 32);
    let mut w2 = BitWriter::endian(Vec::with_capacity(4), BigEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &aligned_data);

    /*writing bytes, aligned*/
    let final_data = [0xB1, 0xED];
    let mut w: BitRecorder<u32, BigEndian> = BitRecorder::new();
    w.write_bytes(b"\xB1\xED").unwrap();
    assert_eq!(w.written(), 16);
    let mut w2 = BitWriter::endian(Vec::with_capacity(2), BigEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data);

    /*writing bytes, un-aligned*/
    let mut w: BitRecorder<u32, BigEndian> = BitRecorder::new();
    let final_data = [0xBB, 0x1E, 0xD0];
    w.write(4, 11u32).unwrap();
    w.write_bytes(b"\xB1\xED").unwrap();
    w.byte_align().unwrap();
    let mut w2 = BitWriter::endian(Vec::with_capacity(3), BigEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data);
}

#[test]
fn test_recorder_huffman_be() {
    use bitstream_io::huffman::compile_write_tree;
    use bitstream_io::{BigEndian, BitRecorder, BitWrite, BitWriter, HuffmanWrite};

    let final_data: [u8; 4] = [0xB1, 0xED, 0x3B, 0xC1];
    let tree = compile_write_tree(vec![
        (0, vec![1, 1]),
        (1, vec![1, 0]),
        (2, vec![0, 1]),
        (3, vec![0, 0, 1]),
        (4, vec![0, 0, 0]),
    ])
    .unwrap();
    let mut w: BitRecorder<u32, BigEndian> = BitRecorder::new();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 4).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 4).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.byte_align().unwrap();
    let mut w2 = BitWriter::endian(Vec::with_capacity(4), BigEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data);
}

#[test]
fn test_recorder_le() {
    use bitstream_io::{BitRecorder, BitWrite, BitWriter, LittleEndian};

    let final_data: [u8; 4] = [0xB1, 0xED, 0x3B, 0xC1];

    /*writing individual bits*/
    let mut w: BitRecorder<u32, LittleEndian> = BitRecorder::new();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(false).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    w.write_bit(true).unwrap();
    assert_eq!(w.written(), 16);
    let mut w2 = BitWriter::endian(Vec::with_capacity(2), LittleEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data[0..2]);

    /*writing unsigned values*/
    let mut w: BitRecorder<u32, LittleEndian> = BitRecorder::new();
    assert!(w.byte_aligned());
    w.write(2, 1u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(3, 4u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(5, 13u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(3, 3u32).unwrap();
    assert!(!w.byte_aligned());
    w.write(19, 0x609DFu32).unwrap();
    assert!(w.byte_aligned());
    assert_eq!(w.written(), 32);
    let mut w2 = BitWriter::endian(Vec::with_capacity(4), LittleEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data);

    /*writing signed values*/
    let mut w: BitRecorder<u32, LittleEndian> = BitRecorder::new();
    w.write_signed(2, 1).unwrap();
    w.write_signed(3, -4).unwrap();
    w.write_signed(5, 13).unwrap();
    w.write_signed(3, 3).unwrap();
    w.write_signed(19, -128545).unwrap();
    assert_eq!(w.written(), 32);
    let mut w2 = BitWriter::endian(Vec::with_capacity(4), LittleEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data);

    /*writing unary 0 values*/
    let mut w: BitRecorder<u32, LittleEndian> = BitRecorder::new();
    w.write_unary0(1).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(2).unwrap();
    w.write_unary0(5).unwrap();
    w.write_unary0(3).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(1).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write_unary0(0).unwrap();
    w.write(2, 3u32).unwrap();
    assert_eq!(w.written(), 32);
    let mut w2 = BitWriter::endian(Vec::with_capacity(4), LittleEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data);

    /*writing unary 1 values*/
    let mut w: BitRecorder<u32, LittleEndian> = BitRecorder::new();
    w.write_unary1(0).unwrap();
    w.write_unary1(3).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(1).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(0).unwrap();
    w.write_unary1(2).unwrap();
    w.write_unary1(5).unwrap();
    w.write_unary1(0).unwrap();
    assert_eq!(w.written(), 32);
    let mut w2 = BitWriter::endian(Vec::with_capacity(4), LittleEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data);

    /*byte aligning*/
    let aligned_data = [0x05, 0x07, 0x3B, 0x0C];
    let mut w: BitRecorder<u32, LittleEndian> = BitRecorder::new();
    w.write(3, 5u32).unwrap();
    w.byte_align().unwrap();
    w.write(3, 7u32).unwrap();
    w.byte_align().unwrap();
    w.byte_align().unwrap();
    w.write(8, 59u32).unwrap();
    w.byte_align().unwrap();
    w.write(4, 12u32).unwrap();
    w.byte_align().unwrap();
    assert_eq!(w.written(), 32);
    let mut w2 = BitWriter::endian(Vec::with_capacity(4), LittleEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &aligned_data);

    /*writing bytes, aligned*/
    let final_data = [0xB1, 0xED];
    let mut w: BitRecorder<u32, LittleEndian> = BitRecorder::new();
    w.write_bytes(b"\xB1\xED").unwrap();
    assert_eq!(w.written(), 16);
    let mut w2 = BitWriter::endian(Vec::with_capacity(2), LittleEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data);

    /*writing bytes, un-aligned*/
    let final_data = [0x1B, 0xDB, 0x0E];
    let mut w: BitRecorder<u32, LittleEndian> = BitRecorder::new();
    w.write(4, 11u32).unwrap();
    w.write_bytes(b"\xB1\xED").unwrap();
    w.byte_align().unwrap();
    assert_eq!(w.written(), 24);
    let mut w2 = BitWriter::endian(Vec::with_capacity(3), LittleEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data);
}

#[test]
fn test_recorder_huffman_le() {
    use bitstream_io::huffman::compile_write_tree;
    use bitstream_io::{BitRecorder, BitWrite, BitWriter, HuffmanWrite, LittleEndian};

    let final_data: [u8; 4] = [0xB1, 0xED, 0x3B, 0xC1];
    let tree = compile_write_tree(vec![
        (0, vec![1, 1]),
        (1, vec![1, 0]),
        (2, vec![0, 1]),
        (3, vec![0, 0, 1]),
        (4, vec![0, 0, 0]),
    ])
    .unwrap();
    let mut w: BitRecorder<u32, LittleEndian> = BitRecorder::new();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 3).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 0).unwrap();
    w.write_huffman(&tree, 1).unwrap();
    w.write_huffman(&tree, 2).unwrap();
    w.write_huffman(&tree, 4).unwrap();
    w.write_huffman(&tree, 3).unwrap();
    w.write(1, 1).unwrap();
    let mut w2 = BitWriter::endian(Vec::with_capacity(4), LittleEndian);
    w.playback(&mut w2).unwrap();
    assert_eq!(w2.into_writer().as_slice(), &final_data);
}
