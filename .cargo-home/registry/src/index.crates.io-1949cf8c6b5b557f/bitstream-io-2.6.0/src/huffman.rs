// Copyright 2017 Brian Langenberger
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Traits and implementations for reading or writing Huffman codes
//! from or to a stream.

#![warn(missing_docs)]

use super::BitQueue;
use super::Endianness;
#[cfg(feature = "alloc")]
use alloc::boxed::Box;
#[cfg(feature = "alloc")]
use alloc::collections::BTreeMap;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
#[cfg(feature = "alloc")]
use core::fmt;
#[cfg(feature = "alloc")]
use core::marker::PhantomData;
#[cfg(feature = "alloc")]
use core2::error::Error;

#[cfg(not(feature = "alloc"))]
use std::collections::BTreeMap;
#[cfg(not(feature = "alloc"))]
use std::error::Error;
#[cfg(not(feature = "alloc"))]
use std::fmt;
#[cfg(not(feature = "alloc"))]
use std::marker::PhantomData;

/// A compiled Huffman tree element for use with the `read_huffman` method.
/// Returned by `compile_read_tree`.
///
/// Compiled read trees are optimized for faster lookup
/// and are therefore endian-specific.
///
/// In addition, each symbol in the source tree may occur many times
/// in the compiled tree.  If symbols require a nontrivial amount of space,
/// consider using reference counting so that they may be cloned
/// more efficiently.
pub enum ReadHuffmanTree<E: Endianness, T: Clone> {
    /// The final value and new reader state
    Done(T, u8, u32, PhantomData<E>),
    /// Another byte is necessary to determine final value
    Continue(Box<[ReadHuffmanTree<E, T>]>),
    /// An invalid reader state has been used
    InvalidState,
}

/// Given a vector of symbol/code pairs, compiles a Huffman tree
/// for reading.
///
/// Code must be 0 or 1 bits and are always read from the stream
/// from least-significant in the list to most signficant
/// (which makes them easier to read for humans).
///
/// All possible codes must be assigned some symbol,
/// and it is acceptable for the same symbol to occur multiple times.
///
/// ## Examples
/// ```
/// use bitstream_io::huffman::compile_read_tree;
/// use bitstream_io::BigEndian;
/// assert!(compile_read_tree::<BigEndian,i32>(
///     vec![(1, vec![0]),
///          (2, vec![1, 0]),
///          (3, vec![1, 1])]).is_ok());
/// ```
///
/// ```
/// use std::io::{Read, Cursor};
/// use bitstream_io::{BigEndian, BitReader, HuffmanRead};
/// use bitstream_io::huffman::compile_read_tree;
/// let tree = compile_read_tree(
///     vec![('a', vec![0]),
///          ('b', vec![1, 0]),
///          ('c', vec![1, 1, 0]),
///          ('d', vec![1, 1, 1])]).unwrap();
/// let data = [0b10110111];
/// let mut cursor = Cursor::new(&data);
/// let mut reader = BitReader::endian(&mut cursor, BigEndian);
/// assert_eq!(reader.read_huffman(&tree).unwrap(), 'b');
/// assert_eq!(reader.read_huffman(&tree).unwrap(), 'c');
/// assert_eq!(reader.read_huffman(&tree).unwrap(), 'd');
/// ```
pub fn compile_read_tree<E, T>(
    values: Vec<(T, Vec<u8>)>,
) -> Result<Box<[ReadHuffmanTree<E, T>]>, HuffmanTreeError>
where
    E: Endianness,
    T: Clone,
{
    let tree = FinalHuffmanTree::new(values)?;

    let mut result = Vec::with_capacity(256);
    result.extend((0..256).map(|_| ReadHuffmanTree::InvalidState));
    let queue = BitQueue::from_value(0, 0);
    let i = queue.to_state();
    result[i] = compile_queue(queue, &tree);
    for bits in 1..8 {
        for value in 0..(1 << bits) {
            let queue = BitQueue::from_value(value, bits);
            let i = queue.to_state();
            result[i] = compile_queue(queue, &tree);
        }
    }
    assert_eq!(result.len(), 256);
    Ok(result.into_boxed_slice())
}

fn compile_queue<E, T>(
    mut queue: BitQueue<E, u8>,
    tree: &FinalHuffmanTree<T>,
) -> ReadHuffmanTree<E, T>
where
    E: Endianness,
    T: Clone,
{
    match tree {
        FinalHuffmanTree::Leaf(ref value) => {
            let len = queue.len();
            ReadHuffmanTree::Done(value.clone(), queue.value(), len, PhantomData)
        }
        FinalHuffmanTree::Tree(ref bit0, ref bit1) => {
            if queue.is_empty() {
                ReadHuffmanTree::Continue(
                    (0..256)
                        .map(|byte| compile_queue(BitQueue::from_value(byte as u8, 8), tree))
                        .collect::<Vec<ReadHuffmanTree<E, T>>>()
                        .into_boxed_slice(),
                )
            } else if queue.pop(1) == 0 {
                compile_queue(queue, bit0)
            } else {
                compile_queue(queue, bit1)
            }
        }
    }
}

// A complete Huffman tree with no empty nodes
enum FinalHuffmanTree<T: Clone> {
    Leaf(T),
    Tree(Box<FinalHuffmanTree<T>>, Box<FinalHuffmanTree<T>>),
}

impl<T: Clone> FinalHuffmanTree<T> {
    fn new(values: Vec<(T, Vec<u8>)>) -> Result<FinalHuffmanTree<T>, HuffmanTreeError> {
        let mut tree = WipHuffmanTree::new_empty();

        for (symbol, code) in values {
            tree.add(code.as_slice(), symbol)?;
        }

        tree.into_read_tree()
    }
}

// Work-in-progress trees may have empty nodes during construction
// but those are not allowed in a finalized tree.
// If the user wants some codes to be None or an error symbol of some sort,
// those will need to be specified explicitly.
enum WipHuffmanTree<T: Clone> {
    Empty,
    Leaf(T),
    Tree(Box<WipHuffmanTree<T>>, Box<WipHuffmanTree<T>>),
}

impl<T: Clone> WipHuffmanTree<T> {
    fn new_empty() -> WipHuffmanTree<T> {
        WipHuffmanTree::Empty
    }

    fn new_leaf(value: T) -> WipHuffmanTree<T> {
        WipHuffmanTree::Leaf(value)
    }

    fn new_tree() -> WipHuffmanTree<T> {
        WipHuffmanTree::Tree(Box::new(Self::new_empty()), Box::new(Self::new_empty()))
    }

    fn into_read_tree(self) -> Result<FinalHuffmanTree<T>, HuffmanTreeError> {
        match self {
            WipHuffmanTree::Empty => Err(HuffmanTreeError::MissingLeaf),
            WipHuffmanTree::Leaf(v) => Ok(FinalHuffmanTree::Leaf(v)),
            WipHuffmanTree::Tree(zero, one) => {
                let zero = zero.into_read_tree()?;
                let one = one.into_read_tree()?;
                Ok(FinalHuffmanTree::Tree(Box::new(zero), Box::new(one)))
            }
        }
    }

    fn add(&mut self, code: &[u8], symbol: T) -> Result<(), HuffmanTreeError> {
        match self {
            WipHuffmanTree::Empty => {
                if code.is_empty() {
                    *self = WipHuffmanTree::new_leaf(symbol);
                    Ok(())
                } else {
                    *self = WipHuffmanTree::new_tree();
                    self.add(code, symbol)
                }
            }
            WipHuffmanTree::Leaf(_) => Err(if code.is_empty() {
                HuffmanTreeError::DuplicateLeaf
            } else {
                HuffmanTreeError::OrphanedLeaf
            }),
            WipHuffmanTree::Tree(ref mut zero, ref mut one) => {
                if code.is_empty() {
                    Err(HuffmanTreeError::DuplicateLeaf)
                } else {
                    match code[0] {
                        0 => zero.add(&code[1..], symbol),
                        1 => one.add(&code[1..], symbol),
                        _ => Err(HuffmanTreeError::InvalidBit),
                    }
                }
            }
        }
    }
}

/// An error type during Huffman tree compilation.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum HuffmanTreeError {
    /// One of the bits in a Huffman code is not 0 or 1
    InvalidBit,
    /// A Huffman code in the specification has no defined symbol
    MissingLeaf,
    /// The same Huffman code specifies multiple symbols
    DuplicateLeaf,
    /// A Huffman code is the prefix of some longer code
    OrphanedLeaf,
}

impl fmt::Display for HuffmanTreeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            HuffmanTreeError::InvalidBit => write!(f, "invalid bit in code"),
            HuffmanTreeError::MissingLeaf => write!(f, "missing leaf node in specification"),
            HuffmanTreeError::DuplicateLeaf => write!(f, "duplicate leaf node in specification"),
            HuffmanTreeError::OrphanedLeaf => write!(f, "orphaned leaf node in specification"),
        }
    }
}

impl Error for HuffmanTreeError {}

/// Given a vector of symbol/code pairs, compiles a Huffman tree
/// for writing.
///
/// Code must be 0 or 1 bits and are always written to the stream
/// from least-significant in the list to most signficant
/// (which makes them easier to read for humans).
///
/// If the same symbol occurs multiple times, the first code is used.
/// Unlike in read trees, not all possible codes need to be
/// assigned a symbol.
///
/// ## Examples
/// ```
/// use bitstream_io::huffman::compile_write_tree;
/// use bitstream_io::BigEndian;
/// assert!(compile_write_tree::<BigEndian,i32>(
///     vec![(1, vec![0]),
///          (2, vec![1, 0]),
///          (3, vec![1, 1])]).is_ok());
/// ```
///
/// ```
/// use std::io::Write;
/// use bitstream_io::{BigEndian, BitWriter, HuffmanWrite};
/// use bitstream_io::huffman::compile_write_tree;
/// let tree = compile_write_tree(
///     vec![('a', vec![0]),
///          ('b', vec![1, 0]),
///          ('c', vec![1, 1, 0]),
///          ('d', vec![1, 1, 1])]).unwrap();
/// let mut data = Vec::new();
/// {
///     let mut writer = BitWriter::endian(&mut data, BigEndian);
///     writer.write_huffman(&tree, 'b').unwrap();
///     writer.write_huffman(&tree, 'c').unwrap();
///     writer.write_huffman(&tree, 'd').unwrap();
/// }
/// assert_eq!(data, [0b10110111]);
/// ```
pub fn compile_write_tree<E, T>(
    values: Vec<(T, Vec<u8>)>,
) -> Result<WriteHuffmanTree<E, T>, HuffmanTreeError>
where
    E: Endianness,
    T: Ord + Clone,
{
    let mut map = BTreeMap::new();

    for (symbol, code) in values {
        let mut encoded = Vec::new();
        for bits in code.chunks(32) {
            let mut acc = BitQueue::<E, u32>::new();
            for bit in bits {
                match *bit {
                    0 => acc.push(1, 0),
                    1 => acc.push(1, 1),
                    _ => return Err(HuffmanTreeError::InvalidBit),
                }
            }
            let len = acc.len();
            encoded.push((len, acc.value()))
        }
        map.entry(symbol)
            .or_insert_with(|| encoded.into_boxed_slice());
    }

    Ok(WriteHuffmanTree {
        map,
        phantom: PhantomData,
    })
}

/// A compiled Huffman tree for use with the `write_huffman` method.
/// Returned by `compiled_write_tree`.
pub struct WriteHuffmanTree<E: Endianness, T: Ord> {
    map: BTreeMap<T, Box<[(u32, u32)]>>,
    phantom: PhantomData<E>,
}

impl<E: Endianness, T: Ord + Clone> WriteHuffmanTree<E, T> {
    /// Returns true if symbol is in tree.
    #[inline]
    pub fn has_symbol(&self, symbol: &T) -> bool {
        self.map.contains_key(symbol)
    }

    /// Given symbol, returns iterator of
    /// (bits, value) pairs for writing code.
    /// Panics if symbol is not found.
    #[inline]
    pub fn get(&self, symbol: &T) -> impl Iterator<Item = &(u32, u32)> {
        self.map[symbol].iter()
    }
}
