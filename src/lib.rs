#![allow(dead_code, unused_assignments, unused_mut, unused_variables)]

extern crate byteorder;
#[cfg(test)]
extern crate quickcheck;
#[cfg(test)]
extern crate rand;

use std::error;
use std::fmt;
use std::result;

pub use compress::{compress, max_compressed_len};
pub use decompress::{decompress, decompress_len};

const MAX_BLOCK_SIZE: usize = 1<<16;
const MAX_INPUT_SIZE: u64 = ::std::u32::MAX as u64;

mod compress;
mod decompress;
mod tag;

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// This error occurs when the given input is too big.
    TooBig {
        /// The size of the given input.
        given: u64,
        /// The maximum allowed size of an input buffer.
        max: u64,
    },
    /// This error occurs during compression when the given buffer is too
    /// small to contain the maximum possible compressed bytes.
    BufferTooSmall {
        /// The size of the given output buffer.
        given: u64,
        /// The minimum size of the output buffer.
        min: u64,
    },
    /// This error occurs during decompression when invalid input is found.
    Corrupt,
    /// Hints that destructuring should not be exhaustive.
    ///
    /// This enum may grow additional variants, so this makes sure clients
    /// don't count on exhaustive matching. (Otherwise, adding a new variant
    /// could break existing code.)
    #[doc(hidden)]
    __Nonexhaustive,
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::TooBig { .. } => "snappy: input buffer too big",
            Error::BufferTooSmall { .. } => "snappy: output buffer too small",
            Error::Corrupt => "snappy: corrupt input",
            _ => unreachable!(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::TooBig { given, max } => {
                write!(f, "snappy: input buffer (size = {}) is larger than \
                           allowed (size = {})", given, max)
            }
            Error::BufferTooSmall { given, min } => {
                write!(f, "snappy: output buffer (size = {}) is smaller than \
                           required (size = {})", given, min)
            }
            Error::Corrupt => {
                write!(f, "snappy: corrupt input")
            }
            _ => unreachable!(),
        }
    }
}

enum Tag {
    Literal = 0b00,
    Copy1 = 0b01,
    Copy2 = 0b10,
    Copy3 = 0b11,
}

impl From<u8> for Tag {
    fn from(b: u8) -> Tag {
        match b {
            0b00 => Tag::Literal,
            0b01 => Tag::Copy1,
            0b10 => Tag::Copy2,
            0b11 => Tag::Copy3,
            _ => panic!("invalid tag byte: {:x}", b),
        }
    }
}

/// https://developers.google.com/protocol-buffers/docs/encoding#varints
fn write_varu64(data: &mut [u8], mut n: u64) -> usize {
    let mut i = 0;
    while n >= 0b1000_0000 {
        data[i] = (n as u8) | 0b1000_0000;
        n >>= 7;
        i += 1;
    }
    data[i] = n as u8;
    i + 1
}

/// https://developers.google.com/protocol-buffers/docs/encoding#varints
fn read_varu64(data: &[u8]) -> (u64, usize) {
    let mut n: u64 = 0;
    let mut shift: u64 = 0;
    for (i, &b) in data.iter().enumerate() {
        if b < 0b1000_0000 {
            return (n | ((b as u64) << shift), i + 1);
        }
        n |= ((b as u64) & 0b0111_1111) << shift;
        shift += 7;
    }
    (0, 0)
}

#[cfg(test)]
mod tests {
    use quickcheck::{QuickCheck, StdGen};

    use super::{compress, decompress, decompress_len, max_compressed_len};

    fn roundtrip(bytes: &[u8]) -> Vec<u8> {
        depress(&press(bytes))
    }

    fn press(bytes: &[u8]) -> Vec<u8> {
        let mut buf = vec![0; max_compressed_len(bytes.len())];
        let n = compress(bytes, &mut buf).unwrap();
        buf.truncate(n);
        buf
    }

    fn depress(bytes: &[u8]) -> Vec<u8> {
        let mut buf = vec![0; decompress_len(bytes).unwrap()];
        let m = decompress(bytes, &mut buf).unwrap();
        buf
    }

    #[test]
    fn qc_roundtrip() {
        fn p(bytes: Vec<u8>) -> bool {
            roundtrip(&bytes) == bytes
        }
        QuickCheck::new()
            .gen(StdGen::new(::rand::thread_rng(), 10_000))
            .tests(10_000)
            .quickcheck(p as fn(_) -> _);
    }

    #[test]
    fn roundtrip1() {
        let data = &[0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 1, 1, 0, 0, 1, 2, 0, 0, 2, 1, 0, 0, 2, 2, 0, 0, 0, 6, 0, 0, 3, 1, 0, 0, 0, 7, 0, 0, 1, 3, 0, 0, 0, 8, 0, 0, 2, 3, 0, 0, 0, 9, 0, 0, 1, 4, 0, 0, 1, 0, 0, 3, 0, 0, 1, 0, 1, 0, 0, 0, 10, 0, 0, 0, 0, 2, 4, 0, 0, 2, 0, 0, 3, 0, 1, 0, 0, 1, 5, 0, 0, 6, 0, 0, 0, 0, 11, 0, 0, 1, 6, 0, 0, 1, 7, 0, 0, 0, 12, 0, 0, 3, 2, 0, 0, 0, 13, 0, 0, 2, 5, 0, 0, 0, 3, 3, 0, 0, 0, 1, 8, 0, 0, 1, 0, 1, 0, 0, 0, 4, 1, 0, 0, 0, 0, 14, 0, 0, 0, 1, 9, 0, 0, 0, 1, 10, 0, 0, 0, 0, 1, 11, 0, 0, 0, 1, 0, 2, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 5, 1, 0, 0, 0, 1, 2, 1, 0, 0, 0, 0, 0, 2, 6, 0, 0, 0, 0, 0, 1, 12, 0, 0, 0, 0, 0, 3, 4, 0, 0, 0, 0, 0, 7, 0, 0, 0, 0, 0, 1, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0][..];

        assert_eq!(data, &*roundtrip(data));
    }
}
