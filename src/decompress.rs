use std::ptr;

use byteorder::{ByteOrder, LittleEndian as LE};

use tag::TAG_LOOKUP_TABLE;
use {
    MAX_INPUT_SIZE,
    Error, Result, Tag,
    read_varu64,
};

pub fn decompress(input: &[u8], output: &mut [u8]) -> Result<usize> {
    if input.is_empty() {
        return Ok(0);
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

const WORD_MASK: [usize; 5] = [0, 0xFF, 0xFFFF, 0xFFFFFF, 0xFFFFFFFF];

fn decompress_block(mut input: &[u8], output: &mut [u8]) -> Result<()> {
    let (mut d, mut s, mut offset, mut len) = (0, 0, 0, 0);
    while s < input.len() {
        let byte = input[s];
        s += 1;
        let entry = TAG_LOOKUP_TABLE[byte as usize] as usize;
        if byte & 0b000000_11 == 0 {
            let mut lit_len = (byte >> 2) as usize + 1;
            if lit_len <= 16 && d + 16 <= output.len() && s + 16 <= input.len() {
                unsafe {
                    ptr::copy_nonoverlapping(
                        input.as_ptr().offset(s as isize),
                        output.as_mut_ptr().offset(d as isize),
                        16);
                }
                d += lit_len;
                s += lit_len;
                continue;
            }
            if lit_len >= 61 {
                if (s as u64) + 4 > input.len() as u64 {
                    return Err(Error::Corrupt);
                }
                let big_len = LE::read_u32(&input[s..]) as usize;
                s += lit_len - 60;
                lit_len = (big_len & WORD_MASK[lit_len - 60]) + 1;
            }
            if d + lit_len > output.len() || s + lit_len > input.len() {
                return Err(Error::Corrupt);
            }
            output[d..d + lit_len].copy_from_slice(&input[s..s + lit_len]);
            s += lit_len;
            d += lit_len;
            continue;
        }
        let extra = entry >> 11;
        let trailer =
            if s + 4 <= input.len() {
                LE::read_u32(&input[s..]) as usize & WORD_MASK[extra]
            } else if extra == 1 {
                input[s] as usize
            } else if extra == 2 {
                LE::read_u16(&input[s..]) as usize
            } else {
                LE::read_u32(&input[s..]) as usize
            };

        len = entry & 0xFF;
        s += extra;
        offset = (entry & 0x700) + trailer;

        if d <= offset.wrapping_sub(1) {
            return Err(Error::Corrupt);
        }
        let end = d + len;
        if len <= 16 && offset >= 8 && d + 16 <= output.len() {
            unsafe {
                let mut dst = output.as_mut_ptr().offset(d as isize);
                let mut src = dst.offset(-(offset as isize));
                ptr::copy_nonoverlapping(src, dst, 8);
                ptr::copy_nonoverlapping(src.offset(8), dst.offset(8), 8);
            }
        } else {
            if end + 10 <= output.len() {
                unsafe {
                    let mut dst = output.as_mut_ptr().offset(d as isize);
                    let mut src = dst.offset(-(offset as isize));
                    loop {
                        let diff = (dst as isize) - (src as isize);
                        if diff >= 8 {
                            break;
                        }
                        ptr::copy(src, dst, 8);
                        d += diff as usize;
                        dst = dst.offset(diff);
                    }
                    while d < end {
                        ptr::copy_nonoverlapping(src, dst, 8);
                        src = src.offset(8);
                        dst = dst.offset(8);
                        d += 8;
                    }
                }
            } else {
                if end > output.len() {
                    return Err(Error::Corrupt);
                }
                while d != end {
                    output[d] = output[d - offset];
                    d += 1;
                }
            }
        }
        d = end;
    }
    if d != output.len() {
        return Err(Error::Corrupt);
    }
    Ok(())
}

pub fn decompress_len(input: &[u8]) -> Result<usize> {
    if input.is_empty() {
        return Ok(0);
    }
    Ok(try!(Header::read(input)).decompress_len)
}

#[inline]
fn inc(n: usize, by: usize, limit: usize) -> Result<usize> {
    let sum = (n as u64) + (by as u64);
    if sum > limit as u64 {
        Err(Error::Corrupt)
    } else {
        Ok(sum as usize)
    }
}

struct Header {
    /// The length of the header in bytes (i.e., the varint).
    len: usize,
    /// The length of the original decompressed input in bytes.
    decompress_len: usize,
}

impl Header {
    fn read(input: &[u8]) -> Result<Header> {
        let (decompress_len, header_len) = read_varu64(input);
        if decompress_len == 0 || header_len == 0 {
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
