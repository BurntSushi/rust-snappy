use compress::max_compress_len;
use crc32::crc32c;
use MAX_BLOCK_SIZE;

lazy_static! {
    pub static ref MAX_COMPRESS_BLOCK_SIZE: usize =
        max_compress_len(MAX_BLOCK_SIZE);
}

// The special magic string that starts any stream.
//
// This may appear more than once in a stream in order to support easy
// concatenation of files compressed in the Snappy frame format.
pub const STREAM_IDENTIFIER: &'static [u8] = b"\xFF\x06\x00\x00sNaPpY";

// The body of the special stream identifier.
pub const STREAM_BODY: &'static [u8] = b"sNaPpY";

// An enumeration describing each of the 4 main chunk types.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ChunkType {
    Stream = 0xFF,
    Compressed = 0x00,
    Uncompressed = 0x01,
    Padding = 0xFE,
}

impl ChunkType {
    /// Converts a byte to one of the four defined chunk types represented by
    /// a single byte. If the chunk type is reserved, then it is returned as
    /// an Err.
    pub fn from_u8(b: u8) -> Result<ChunkType, u8> {
        match b {
            0xFF => Ok(ChunkType::Stream),
            0x00 => Ok(ChunkType::Compressed),
            0x01 => Ok(ChunkType::Uncompressed),
            0xFE => Ok(ChunkType::Padding),
            b => Err(b),
        }
    }
}

pub fn crc32c_masked(buf: &[u8]) -> u32 {
    // TODO(burntsushi): SSE 4.2 has a CRC32 instruction that is probably
    // faster. Oh how I long for you, SIMD. See src/crc32.rs for a lamentation.
    let sum = crc32c(buf);
    (sum.wrapping_shr(15) | sum.wrapping_shl(17)).wrapping_add(0xA282EAD8)
}
