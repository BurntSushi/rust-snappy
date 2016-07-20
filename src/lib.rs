#![allow(dead_code, unused_assignments, unused_mut, unused_variables)]

extern crate byteorder;
#[cfg(test)]
extern crate quickcheck;
#[cfg(test)]
extern crate rand;
#[cfg(all(test, feature = "cpp"))]
extern crate snappy_cpp;

use std::error;
use std::fmt;
use std::result;

pub use compress::{compress, max_compressed_len};
pub use decompress::{Decoder, decompress_len};

const MAX_INPUT_SIZE: u64 = ::std::u32::MAX as u64;

mod compress;
mod decompress;
mod tag;
#[cfg(test)]
mod tests;

pub type Result<T> = result::Result<T, Error>;

#[derive(Clone, Debug, Eq, PartialEq)]
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
    let mut shift: u32 = 0;
    for (i, &b) in data.iter().enumerate() {
        if b < 0b1000_0000 {
            return match (b as u64).checked_shl(shift) {
                None => (0, 0),
                Some(b) => (n | b, i + 1),
            };
        }
        match ((b as u64) & 0b0111_1111).checked_shl(shift) {
            None => return (0, 0),
            Some(b) => n |= b,
        }
        shift += 7;
    }
    (0, 0)
}
