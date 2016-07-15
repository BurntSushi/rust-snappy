#![allow(dead_code, unused_assignments, unused_variables)]

use std::error;
use std::fmt;
use std::result;

const MAX_BLOCK_SIZE: u64 = 1<<16;
const MAX_INPUT_SIZE: u64 = (1<<32) - 1;

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// This error occurs when the given input is too big to be compressed.
    InputTooBig {
        /// The size of the given input buffer.
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
            Error::InputTooBig { .. } => "input buffer too big",
            Error::BufferTooSmall { .. } => "output buffer too small",
            _ => unreachable!(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::InputTooBig { given, max } => {
                write!(f, "input buffer (size = {}) is larger than \
                           allowed (size = {})", given, max)
            }
            Error::BufferTooSmall { given, min } => {
                write!(f, "output buffer (size = {}) is smaller than \
                           required (size = {})", given, min)
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

pub fn compress(mut input: &[u8], output: &mut [u8]) -> Result<()> {
    match max_compressed_len(input.len() as u64) {
        None => {
            return Err(Error::InputTooBig {
                given: input.len() as u64,
                max: MAX_INPUT_SIZE,
            });
        }
        Some(min) if (output.len() as u64) < min => {
            return Err(Error::BufferTooSmall {
                given: output.len() as u64,
                min: min,
            });
        }
        _ => {}
    }
    write_varu64(output, input.len() as u64);
    while !input.is_empty() {
        let mut block = input;
        if block.len() as u64 > MAX_BLOCK_SIZE {
            block = &block[..MAX_BLOCK_SIZE as usize];
        }
        input = &input[block.len()..];
    }
    Ok(())
}

fn emit_literal(literal: &[u8], output: &mut [u8]) -> usize {
    let n = literal.len().checked_sub(1).unwrap();
    let mut start = 0;
    if n <= 59 {
        output[0] = ((n as u8) << 2) | (Tag::Literal as u8);
        start = 1;
    } else if n <= 256 {
        output[0] = 60 << 2;
        output[1] = n as u8;
        start = 2;
    } else if n <= 65536 {
        output[0] = 61 << 2;
        output[1] = n as u8;
        output[2] = (n >> 8) as u8;
        start = 3;
    } else {
        unreachable!();
    }
    output[start..].copy_from_slice(literal);
    start + literal.len()
}

fn emit_copy(offset: usize, mut len: usize, output: &mut [u8]) -> usize {
    let mut i = 0;
    while len >= 68 {
        output[i + 0] = (63 << 2) | (Tag::Copy2 as u8);
        output[i + 1] = offset as u8;
        output[i + 2] = (offset >> 8) as u8;
        i += 3;
        len -= 64;
    }
    if len > 64 {
        output[i + 0] = (59 << 2) | (Tag::Copy2 as u8);
        output[i + 1] = offset as u8;
        output[i + 2] = (offset >> 8) as u8;
        i += 3;
        len -= 60;
    }
    if len <= 11 && offset <= 2047 {
        output[i + 0] =
            (((offset >> 8) as u8) << 5)
            | (((len - 4) as u8) << 2)
            | (Tag::Copy1 as u8);
        output[i + 1] = offset as u8;
        i + 2
    } else {
        output[i + 0] = (((len - 1) as u8) << 2) | (Tag::Copy2 as u8);
        output[i + 1] = offset as u8;
        output[i + 2] = (offset >> 8) as u8;
        i + 3
    }
}

fn max_compressed_len(input_len: u64) -> Option<u64> {
    if input_len > MAX_INPUT_SIZE {
        return None;
    }
    let max = 32 + input_len + (input_len / 6);
    if max > MAX_INPUT_SIZE {
        None
    } else {
        Some(max)
    }
}

/// https://developers.google.com/protocol-buffers/docs/encoding#varints
fn write_varu64(data: &mut [u8], mut n: u64) {
    let mut i = 0;
    while n >= 0b1000_0000 {
        data[i] = (n as u8) | 0b1000_0000;
        n >>= 7;
        i += 1;
    }
    data[i] = n as u8;
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
    #[test]
    fn it_works() {
    }
}
