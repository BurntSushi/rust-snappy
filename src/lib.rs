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
use std::io;
use std::result;

pub use compress::{compress, max_compressed_len};
pub use decompress::{Decoder, decompress_len};

const MAX_INPUT_SIZE: u64 = ::std::u32::MAX as u64;

mod compress;
mod decompress;
mod tag;
#[cfg(test)]
mod tests;

/// A convenient type alias for `Result<T, snap::Error>`.
pub type Result<T> = result::Result<T, Error>;

/// Error describes all the possible errors that may occur during Snappy
/// compression or decompression.
///
/// Note that while any of the errors defined may occur during decompression,
/// only the `TooBig` and `BufferTooSmall` errors may occur during compression.
#[derive(Debug)]
pub enum Error {
    /// This error occurs if an I/O error happens during compression or
    /// decompression. Note that this error only occurs via the `Read` and
    /// `Write` interfaces. It does not occur when using the raw buffer based
    /// encoder and decoder.
    Io(io::Error),
    /// This error occurs when the given input is too big. This can happen
    /// during compression or decompression.
    TooBig {
        /// The size of the given input.
        given: u64,
        /// The maximum allowed size of an input buffer.
        max: u64,
    },
    /// This error occurs when the given buffer is too small to contain the
    /// maximum possible compressed bytes or the total number of decompressed
    /// bytes.
    BufferTooSmall {
        /// The size of the given output buffer.
        given: u64,
        /// The minimum size of the output buffer.
        min: u64,
    },
    /// This error occurs when trying to decompress a zero length buffer.
    Empty,
    /// This error occurs when an invalid header is found during decompression.
    Header,
    /// This error occurs when there is a mismatch between the number of
    /// decompressed bytes reported in the header and the number of
    /// actual decompressed bytes. In this error case, the number of actual
    /// decompressed bytes is always less than the number reported in the
    /// header.
    HeaderMismatch {
        /// The total number of decompressed bytes expected (i.e., the header
        /// value).
        expected_len: u64,
        /// The total number of actual decompressed bytes.
        got_len: u64,
    },
    /// This error occurs during decompression when there was a problem
    /// reading a literal.
    Literal {
        /// The expected length of the literal.
        len: u64,
        /// The number of remaining bytes in the compressed bytes.
        src_len: u64,
        /// The number of remaining slots in the decompression buffer.
        dst_len: u64,
    },
    /// This error occurs during decompression when there was a problem
    /// reading a copy.
    CopyRead {
        /// The expected length of the copy (as encoded in the compressed
        /// bytes).
        len: u64,
        /// The number of remaining bytes in the compressed bytes.
        src_len: u64,
    },
    /// This error occurs during decompression when there was a problem
    /// writing a copy to the decompression buffer.
    CopyWrite {
        /// The length of the copy (i.e., the total number of bytes to be
        /// produced by this copy in the decompression buffer).
        len: u64,
        /// The number of remaining bytes in the decompression buffer.
        dst_len: u64,
    },
    /// This error occurs during decompression when an invalid copy offset
    /// is found. An offset is invalid if it is zero or if it is out of bounds.
    Offset {
        /// The offset that was read.
        offset: u64,
        /// The current position in the decompression buffer. If the offset is
        /// non-zero, then the offset must be greater than this position.
        dst_pos: u64,
    },
}

impl Eq for Error {}

/// This implementation of `PartialEq` returns `false` when comparing two
/// errors whose underlying type is `std::io::Error`.
impl PartialEq for Error {
    fn eq(&self, other: &Error) -> bool {
        use self::Error::*;
        match (self, other) {
            (&Io(_), &Io(_)) => false,
            (&TooBig { given: given1, max: max1 },
             &TooBig { given: given2, max: max2 }) => {
                (given1, max1) == (given2, max2)
            }
            (&BufferTooSmall { given: given1, min: min1 },
             &BufferTooSmall { given: given2, min: min2 }) => {
                (given1, min1) == (given2, min2)
            }
            (&Empty, &Empty) => true,
            (&Header, &Header) => true,
            (&HeaderMismatch { expected_len: elen1, got_len: glen1 },
             &HeaderMismatch { expected_len: elen2, got_len: glen2 }) => {
                (elen1, glen1) == (elen2, glen2)
            }
            (&Literal { len: len1, src_len: src_len1, dst_len: dst_len1 },
             &Literal { len: len2, src_len: src_len2, dst_len: dst_len2 }) => {
                (len1, src_len1, dst_len1) == (len2, src_len2, dst_len2)
            }
            (&CopyRead { len: len1, src_len: src_len1 },
             &CopyRead { len: len2, src_len: src_len2 }) => {
                (len1, src_len1) == (len2, src_len2)
            }
            (&CopyWrite { len: len1, dst_len: dst_len1 },
             &CopyWrite { len: len2, dst_len: dst_len2 }) => {
                (len1, dst_len1) == (len2, dst_len2)
            }
            (&Offset { offset: offset1, dst_pos: dst_pos1 },
             &Offset { offset: offset2, dst_pos: dst_pos2 }) => {
                (offset1, dst_pos1) == (offset2, dst_pos2)
            }
            _ => false,
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Io(ref err) => err.description(),
            Error::TooBig { .. } => "snappy: input buffer too big",
            Error::BufferTooSmall { .. } => "snappy: output buffer too small",
            Error::Empty => "snappy: corrupt input (empty)",
            Error::Header => "snappy: corrupt input (invalid header)",
            Error::HeaderMismatch { .. } => "snappy: corrupt input \
                                             (header mismatch)",
            Error::Literal { .. } => "snappy: corrupt input (bad literal)",
            Error::CopyRead { .. } => "snappy: corrupt input (bad copy read)",
            Error::CopyWrite { .. } => "snappy: corrupt input \
                                        (bad copy write)",
            Error::Offset { .. } => "snappy: corrupt input (bad offset)",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::Io(ref err) => Some(err),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Io(ref err) => err.fmt(f),
            Error::TooBig { given, max } => {
                write!(f, "snappy: input buffer (size = {}) is larger than \
                           allowed (size = {})", given, max)
            }
            Error::BufferTooSmall { given, min } => {
                write!(f, "snappy: output buffer (size = {}) is smaller than \
                           required (size = {})", given, min)
            }
            Error::Empty => {
                write!(f, "snappy: corrupt input (empty)")
            }
            Error::Header => {
                write!(f, "snappy: corrupt input (invalid header)")
            }
            Error::HeaderMismatch { expected_len, got_len } => {
                write!(f, "snappy: corrupt input (header mismatch; expected \
                           {} decompressed bytes but got {})",
                           expected_len, got_len)
            }
            Error::Literal { len, src_len, dst_len } => {
                write!(f, "snappy: corrupt input (expected literal read of \
                           length {}; remaining src: {}; remaining dst: {})",
                       len, src_len, dst_len)
            }
            Error::CopyRead { len, src_len } => {
                write!(f, "snappy: corrupt input (expected copy read of \
                           length {}; remaining src: {})", len, src_len)
            }
            Error::CopyWrite { len, dst_len } => {
                write!(f, "snappy: corrupt input (expected copy write of \
                           length {}; remaining dst: {})", len, dst_len)
            }
            Error::Offset { offset, dst_pos } => {
                write!(f, "snappy: corrupt input (expected valid offset but \
                           got offset {}; dst position: {})", offset, dst_pos)
            }
        }
    }
}

enum Tag {
    Literal = 0b00,
    Copy1 = 0b01,
    Copy2 = 0b10,
    Copy4 = 0b11,
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
