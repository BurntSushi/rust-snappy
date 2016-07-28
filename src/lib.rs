/*!
Snappy compression and decompression, including support for streaming, written
in Rust.
*/

extern crate byteorder;
#[macro_use]
extern crate lazy_static;
#[cfg(test)]
extern crate quickcheck;
#[cfg(test)]
extern crate rand;
#[cfg(all(test, feature = "cpp"))]
extern crate snappy_cpp;

pub use compress::{Encoder, max_compress_len};
pub use decompress::{Decoder, decompress_len};
pub use error::{Error, IntoInnerError, Result};
pub use frame::{Reader, Writer};

/// We don't permit compressing a block bigger than what can fit in a u32.
const MAX_INPUT_SIZE: u64 = ::std::u32::MAX as u64;

/// The maximum number of bytes that we process at once. A block is the unit
/// at which we scan for candidates for compression.
const MAX_BLOCK_SIZE: usize = 1<<16;

mod compress;
mod crc32;
mod decompress;
mod error;
mod frame;
mod tag;
#[cfg(test)]
mod tests;
mod varint;
