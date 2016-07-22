use std::ptr;

use byteorder::{ByteOrder, LittleEndian as LE};

use tag;
use {
    MAX_INPUT_SIZE,
    Error, Result,
    read_varu64,
};

const TAG_LOOKUP_TABLE: TagLookupTable = TagLookupTable(tag::TAG_LOOKUP_TABLE);

/// WORD_MASK is a map from the size of an integer in bytes to its
/// corresponding on a 32 bit integer. This is used when we need to read an
/// integer and we know there are at least 4 bytes to read from a buffer. In
/// this case, we can read a 32 bit little endian integer and mask out only the
/// bits we need. This in particular saves a branch.
const WORD_MASK: [usize; 5] = [0, 0xFF, 0xFFFF, 0xFFFFFF, 0xFFFFFFFF];

/// Returns the decompressed size (in bytes) of the compressed bytes given.
///
/// `input` must be a sequence of bytes returned by a conforming Snappy
/// compressor.
///
/// # Errors
///
/// This function returns an error in the following circumstances:
///
/// * An invalid Snappy header was seen.
/// * The total space required for decompression exceeds `2^32 - 1`.
#[inline(always)]
pub fn decompress_len(input: &[u8]) -> Result<usize> {
    if input.is_empty() {
        return Ok(0);
    }
    Ok(try!(Header::read(input)).decompress_len)
}

/// Decoder is a raw low level decoder for decompressing bytes.
///
/// This decoder does not use the Snappy frame format and simply decompresses
/// the given bytes as if it were returned from `Encoder`.
///
/// Unless you explicitly need the low-level control, you should use
/// `Reader` instead, which decompresses the Snappy frame format.
#[derive(Clone, Debug, Default)]
pub struct Decoder {
    total_in: u64,
    total_out: u64,
}

impl Decoder {
    /// Return a new decoder that can be used for decompressing bytes.
    #[inline(always)]
    pub fn new() -> Decoder {
        Decoder { total_in: 0, total_out: 0 }
    }

    /// Decompresses all bytes in `input` into `output`.
    ///
    /// `input` must be a sequence of bytes returned by a conforming Snappy
    /// compressor.
    ///
    /// The size of `output` must be large enough to hold all decompressed
    /// bytes from the `input`. The size required can be queried with the
    /// `decompress_len` function.
    ///
    /// On success, this returns the number of bytes written to `output`.
    ///
    /// # Errors
    ///
    /// This method returns an error in the following circumstances:
    ///
    /// * Invalid compressed Snappy data was seen.
    /// * The total space required for decompression exceeds `2^32 - 1`.
    /// * `output` has length less than `decompress_len(input)`.
    pub fn decompress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<usize> {
        if input.is_empty() {
            return Err(Error::Empty);
        }
        let hdr = try!(Header::read(input));
        if hdr.decompress_len > output.len() {
            return Err(Error::BufferTooSmall {
                given: output.len() as u64,
                min: hdr.decompress_len as u64,
            });
        }
        let output = &mut output[..hdr.decompress_len];
        try!(decompress_block(&input[hdr.len..], output));
        Ok(output.len())
    }

    /// Decompresses all bytes in `input` into a freshly allocated `Vec`.
    ///
    /// This is just like the `decompress` method, except it allocates a `Vec`
    /// with the right size for you. (This is intended to be a convenience
    /// method.)
    ///
    /// This method returns an error under the same circumstances that
    /// `decompress` does.
    pub fn decompress_vec(&mut self, input: &[u8]) -> Result<Vec<u8>> {
        let mut buf = vec![0; try!(decompress_len(input))];
        try!(self.decompress(input, &mut buf));
        Ok(buf)
    }
}

fn decompress_block(src: &[u8], dst: &mut [u8]) -> Result<()> {
    let (mut d, mut s, mut offset, mut len) = (0, 0, 0, 0);
    while s < src.len() {
        let byte = src[s];
        s += 1;
        let entry = TAG_LOOKUP_TABLE.entry(byte);
        let (_s, _d) =
            if byte & 0b000000_11 == 0 {
                let mut len = (byte >> 2) as usize + 1;
                try!(read_literal(src, s, dst, d, len))
            } else {
                try!(read_copy(src, s, dst, d, entry))
            };
        s = _s;
        d = _d;
    }
    if d != dst.len() {
        return Err(Error::HeaderMismatch {
            expected_len: dst.len() as u64,
            got_len: d as u64,
        });
    }
    Ok(())
}

/// Decompresses a literal from `src` starting at `s` to `dst` starting at `d`
/// and returns the updated values of `s` and `d`.
///
/// `len` is the length of the literal if it's <=60. Otherwise, it's the length
/// tag, indicating the number of bytes needed to read a little endian
/// integer at `src[s..]`. i.e., `61 => 1 byte`, `62 => 2 bytes`,
/// `63 => 3 bytes` and `64 => 4 bytes`.
///
/// `len` must be <=64.
#[inline(always)]
fn read_literal(
    src: &[u8],
    mut s: usize,
    dst: &mut [u8],
    mut d: usize,
    mut len: usize,
) -> Result<(usize, usize)> {
    debug_assert!(len <= 64);
    // As an optimization for the common case, if the literal length is <=16
    // and we have enough room in both `src` and `dst`, copy the literal using
    // unaligned loads and stores.
    //
    // We pick 16 bytes with the hope that it optimizes down to a 128 bit
    // load/store.
    if len <= 16 && s + 16 <= src.len() && d + 16 <= dst.len() {
        unsafe {
            // SAFETY: We know both src and dst have at least 16 bytes of
            // wiggle room after s/d, even if `len` is <16, so the copy is
            // safe.
            let srcp = src.as_ptr().offset(s as isize);
            let dstp = dst.as_mut_ptr().offset(d as isize);
            ptr::copy_nonoverlapping(srcp, dstp, 16);
        }
        d += len;
        s += len;
        return Ok((s, d));
    }
    // When the length is bigger than 60, it indicates that we need to read
    // an additional 1-4 bytes to get the real length of the literal.
    // println!("len: {:?}", len);
    if len >= 61 {
        // If there aren't at least 4 bytes left to read then we know this is
        // corrupt because the literal must have length >=61.
        // println!("s: {:?}, src.len: {:?}", s, src.len());
        if s as u64 + 4 > src.len() as u64 {
            return Err(Error::Literal {
                len: 4,
                src_len: (src.len() - s) as u64,
                dst_len: (dst.len() - d) as u64,
            });
        }
        // Since we know there are 4 bytes left to read, read a 32 bit LE
        // integer and mask away the bits we don't need.
        let byte_count = len - 60;
        let old_len = len;
        len = (LE::read_u32(&src[s..]) as usize & WORD_MASK[byte_count]) + 1;
        s += old_len - 60;
    }
    // If there's not enough buffer left to load or store this literal, then
    // the input is corrupt.
    if s + len > src.len() || d + len > dst.len() {
        return Err(Error::Literal {
            len: len as u64,
            src_len: (src.len() - s) as u64,
            dst_len: (dst.len() - d) as u64,
        });
    }
    unsafe {
        // SAFETY: We've already checked the bounds, so we know this copy is
        // correct.
        let srcp = src.as_ptr().offset(s as isize);
        let dstp = dst.as_mut_ptr().offset(d as isize);
        ptr::copy_nonoverlapping(srcp, dstp, len);
    }
    s += len;
    d += len;
    Ok((s, d))
}

#[inline(always)]
fn read_copy(
    src: &[u8],
    mut s: usize,
    dst: &mut [u8],
    mut d: usize,
    entry: TagEntry,
) -> Result<(usize, usize)> {
    let offset = try!(entry.offset(src, s));
    let len = entry.len();
    s += entry.num_tag_bytes();

    if d <= offset.wrapping_sub(1) {
        return Err(Error::Offset {
            offset: offset as u64,
            dst_pos: d as u64,
        });
    }
    let end = d + len;
    if len <= 16 && offset >= 8 && d + 16 <= dst.len() {
        unsafe {
            let mut dstp = dst.as_mut_ptr().offset(d as isize);
            let mut srcp = dstp.offset(-(offset as isize));
            ptr::copy_nonoverlapping(srcp, dstp, 8);
            ptr::copy_nonoverlapping(srcp.offset(8), dstp.offset(8), 8);
        }
    } else {
        if end + 10 <= dst.len() {
            unsafe {
                let mut dstp = dst.as_mut_ptr().offset(d as isize);
                let mut srcp = dstp.offset(-(offset as isize));
                loop {
                    let diff = (dstp as isize) - (srcp as isize);
                    if diff >= 8 {
                        break;
                    }
                    ptr::copy(srcp, dstp, 8);
                    d += diff as usize;
                    dstp = dstp.offset(diff);
                }
                while d < end {
                    ptr::copy_nonoverlapping(srcp, dstp, 8);
                    srcp = srcp.offset(8);
                    dstp = dstp.offset(8);
                    d += 8;
                }
            }
        } else {
            if end > dst.len() {
                return Err(Error::CopyWrite {
                    len: len as u64,
                    dst_len: (dst.len() - d) as u64,
                });
            }
            while d != end {
                dst[d] = dst[d - offset];
                d += 1;
            }
        }
    }
    Ok((s, end))
}

/// Header represents the single varint that starts every Snappy compressed
/// block.
#[derive(Debug)]
struct Header {
    /// The length of the header in bytes (i.e., the varint).
    len: usize,
    /// The length of the original decompressed input in bytes.
    decompress_len: usize,
}

impl Header {
    /// Reads the varint header from the given input.
    ///
    /// If there was a problem reading the header then an error is returned.
    /// If a header is returned then it is guaranteed to be valid.
    #[inline(always)]
    fn read(input: &[u8]) -> Result<Header> {
        let (decompress_len, header_len) = read_varu64(input);
        if header_len == 0 {
            return Err(Error::Header);
        }
        if decompress_len > MAX_INPUT_SIZE {
            return Err(Error::TooBig {
                given: decompress_len as u64,
                max: MAX_INPUT_SIZE,
            });
        }
        Ok(Header { len: header_len, decompress_len: decompress_len as usize })
    }
}

/// A lookup table for quickly computing the various attributes derived from
/// a tag byte. The attributes are most useful for the three "copy" tags
/// and include the length of the copy, part of the offset (for copy 1-byte
/// only) and the total number of bytes proceding the tag byte that encode
/// the other part of the offset (1 for copy 1, 2 for copy 2 and 4 for copy 4).
///
/// More specifically, the keys of the table are u8s and the values are u16s.
/// The bits of the values are laid out as follows:
///
/// xxaa abbb xxcc cccc
///
/// Where `a` is the number of bytes, `b` are the three bits of the offset
/// for copy 1 (the other 8 bits are in the byte proceding the tag byte; for
/// copy 2 and copy 4, `b = 0`), and `c` is the length of the copy (max of 64).
///
/// We could pack this in fewer bits, but the position of the three `b` bits
/// lines up with the most significant three bits in the total offset for copy
/// 1, which avoids an extra shift instruction.
///
/// In sum, this table is useful because it reduces branches and various
/// arithmetic operations.
struct TagLookupTable([u16; 256]);

impl TagLookupTable {
    /// Look up the tag entry given the tag `byte`.
    #[inline(always)]
    fn entry(&self, byte: u8) -> TagEntry {
        TagEntry(self.0[byte as usize] as usize)
    }
}

/// Represents a single entry in the tag lookup table.
///
/// See the documentation in TagLookupTable for the bit layout.
///
/// The type is a `usize` for convenience.
struct TagEntry(usize);

impl TagEntry {
    /// Return the total number of bytes proceding this tag byte required to
    /// encode the offset.
    fn num_tag_bytes(&self) -> usize {
        self.0 >> 11
    }

    /// Return the total copy length, capped at 64.
    fn len(&self) -> usize {
        self.0 & 0xFF
    }

    /// Return the copy offset corresponding to this copy operation. `s` should
    /// point to the position just after the tag byte that this entry was read
    /// from.
    ///
    /// This requires reading from the compressed input since the offset is
    /// encoded in bytes proceding the tag byte.
    fn offset(&self, src: &[u8], s: usize) -> Result<usize> {
        let num_tag_bytes = self.num_tag_bytes();
        let trailer =
            // It is critical for this case to come first, since it is the
            // fast path. We really hope that this case gets branch
            // predicted.
            if s + 4 <= src.len() {
                unsafe {
                    // SAFETY: The conditional above guarantees that
                    // src[s..s+4] is valid to read from.
                    let p = src.as_ptr().offset(s as isize);
                    // We use WORD_MASK here to mask out the bits we don't
                    // need. While we're guaranteed to read 4 valid bytes,
                    // not all of those bytes are necessarily part of the
                    // offset. This is the key optimization: we don't need to
                    // branch on num_tag_bytes.
                    loadu32_le(p) as usize & WORD_MASK[num_tag_bytes]
                }
            } else if num_tag_bytes == 1 {
                if s >= src.len() {
                    return Err(Error::CopyRead {
                        len: 1,
                        src_len: (src.len() - s) as u64,
                    });
                }
                src[s] as usize
            } else if num_tag_bytes == 2 {
                if s + 1 >= src.len() {
                    return Err(Error::CopyRead {
                        len: 2,
                        src_len: (src.len() - s) as u64,
                    });
                }
                LE::read_u16(&src[s..]) as usize
            } else {
                return Err(Error::CopyRead {
                    len: num_tag_bytes as u64,
                    src_len: (src.len() - s) as u64,
                });
            };
        Ok((self.0 & 0b0000_0111_0000_0000) | trailer)
    }
}

#[inline(always)]
unsafe fn loadu32_le(data: *const u8) -> u32 {
    let mut n: u32 = 0;
    ptr::copy_nonoverlapping(
        data,
        &mut n as *mut u32 as *mut u8,
        4);
    n.to_le()
}
