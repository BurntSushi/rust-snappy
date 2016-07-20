use std::ptr;

use byteorder::{ByteOrder, LittleEndian as LE};

use tag;
use {
    MAX_INPUT_SIZE,
    Error, Result,
    read_varu64,
};

const TAG_LOOKUP_TABLE: TagLookupTable = TagLookupTable(tag::TAG_LOOKUP_TABLE);
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
            return Err(Error::Corrupt);
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
        return Err(Error::Corrupt);
    }
    Ok(())
}

#[inline(always)]
fn read_literal(
    src: &[u8],
    mut s: usize,
    dst: &mut [u8],
    mut d: usize,
    mut len: usize,
) -> Result<(usize, usize)> {
    if len <= 16 && d + 16 <= dst.len() && s + 16 <= src.len() {
        unsafe {
            ptr::copy_nonoverlapping(
                src.as_ptr().offset(s as isize),
                dst.as_mut_ptr().offset(d as isize),
                16);
        }
        d += len;
        s += len;
        return Ok((s, d));
    }
    if len >= 61 {
        if (s as u64) + 4 > src.len() as u64 {
            return Err(Error::Corrupt);
        }
        let old_len = len;
        len = (LE::read_u32(&src[s..]) as usize & WORD_MASK[len - 60]) + 1;
        s += old_len - 60;
    }
    if d + len > dst.len() || s + len > src.len() {
        return Err(Error::Corrupt);
    }
    dst[d..d + len].copy_from_slice(&src[s..s + len]);
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
        return Err(Error::Corrupt);
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
                return Err(Error::Corrupt);
            }
            while d != end {
                dst[d] = dst[d - offset];
                d += 1;
            }
        }
    }
    Ok((s, end))
}

#[derive(Debug)]
struct Header {
    /// The length of the header in bytes (i.e., the varint).
    len: usize,
    /// The length of the original decompressed input in bytes.
    decompress_len: usize,
}

impl Header {
    #[inline(always)]
    fn read(input: &[u8]) -> Result<Header> {
        let (decompress_len, header_len) = read_varu64(input);
        if header_len == 0 {
            return Err(Error::Corrupt);
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

struct TagLookupTable([u16; 256]);

impl TagLookupTable {
    #[inline(always)]
    fn entry(&self, byte: u8) -> TagEntry {
        TagEntry(self.0[byte as usize] as usize)
    }
}

struct TagEntry(usize);

impl TagEntry {
    fn num_tag_bytes(&self) -> usize {
        self.0 >> 11
    }

    fn len(&self) -> usize {
        self.0 & 0xFF
    }

    fn offset(&self, src: &[u8], s: usize) -> Result<usize> {
        let num_tag_bytes = self.num_tag_bytes();
        let trailer =
            if s + 4 <= src.len() {
                unsafe {
                    let p = src.as_ptr().offset(s as isize);
                    loadu32_le(p) as usize & WORD_MASK[num_tag_bytes]
                }
            } else if num_tag_bytes == 1 {
                if s >= src.len() {
                    return Err(Error::Corrupt);
                }
                src[s] as usize
            } else if num_tag_bytes == 2 {
                if s + 1 >= src.len() {
                    return Err(Error::Corrupt);
                }
                LE::read_u16(&src[s..]) as usize
            } else {
                return Err(Error::Corrupt);
            };
        Ok((self.0 & 0b0000_0111_0000_0000) | trailer)
    }
}

unsafe fn loadu32(data: *const u8) -> u32 {
    let mut n: u32 = 0;
    ptr::copy_nonoverlapping(
        data,
        &mut n as *mut u32 as *mut u8,
        4);
    n
}

unsafe fn loadu32_le(data: *const u8) -> u32 {
    loadu32(data).to_le()
}
