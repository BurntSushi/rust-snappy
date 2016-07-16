use std::ptr;

use byteorder::{ByteOrder, LittleEndian as LE};

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

fn decompress_block(mut input: &[u8], output: &mut [u8]) -> Result<()> {
    let (mut d, mut s, mut offset, mut len) = (0, 0, 0, 0);
    while s < input.len() {
        match Tag::from(input[s] & 0b000000_11) {
            Tag::Literal => {
                let mut lit_len = (input[s] >> 2) as usize;
                if lit_len < 60 {
                    s += 1;
                } else if lit_len == 60 {
                    s = try!(inc(s, 2, input.len()));
                    lit_len = input[s-1] as usize;
                } else if lit_len == 61 {
                    s = try!(inc(s, 3, input.len()));
                    lit_len = LE::read_u16(&input[s-2..]) as usize;
                } else if lit_len == 62 {
                    s = try!(inc(s, 4, input.len()));
                    lit_len = LE::read_uint(&input[s-3..], 3) as usize;
                } else if lit_len == 63 {
                    s = try!(inc(s, 5, input.len()));
                    lit_len = LE::read_u32(&input[s-4..]) as usize;
                } else {
                    unreachable!();
                }
                len = try!(inc(lit_len, 1, ::std::usize::MAX));
                if len > 16 || (output.len() - d) < 16 || (input.len() - s) < 16 {
                    if len > (output.len() - d) || len > (input.len() - s) {
                        return Err(Error::Corrupt);
                    }
                    output[d..d + len].copy_from_slice(&input[s..s + len]);
                } else {
                    unsafe {
                        u64_copy(input, s, output, d);
                        u64_copy(input, s + 8, output, d + 8);
                    }
                }
                d += len;
                s += len;
                continue;
            }
            Tag::Copy1 => unsafe {
                // s = try!(inc(s, 2, input.len()));
                s += 2;
                let tag = *input.get_unchecked(s - 2) as usize;
                len = 4 + ((tag >> 2) & 0b111);
                offset =
                    ((tag & 0b111_00000) << 3)
                    | (*input.get_unchecked(s - 1) as usize);
            },
            Tag::Copy2 => unsafe {
                // s = try!(inc(s, 3, input.len()));
                s += 3;
                len = 1 + (*input.get_unchecked(s - 3) >> 2) as usize;
                offset = LE::read_u16(&input[s - 2..]) as usize;
            },
            Tag::Copy3 => unsafe {
                // s = try!(inc(s, 5, input.len()));
                s += 5;
                len = 1 + (*input.get_unchecked(s - 5) >> 2) as usize;
                offset = LE::read_u32(&input[s - 4..]) as usize;
            },
        }
        if d < offset {
            return Err(Error::Corrupt);
        }
        let end = d + len;
        if d + len + 10 > output.len() {
            if d + len > output.len() {
                return Err(Error::Corrupt);
            }
            while d != end {
                output[d] = output[d - offset];
                d += 1;
            }
        } else {
            unsafe {
                let mut dst: *mut u8 = output[d..].as_mut_ptr();
                let mut src = dst.offset(-(offset as isize));
                if len <= 16 && offset >= 8 && d + len + 16 <= output.len() {
                    u64_copy_raw(src, 0, dst, 0);
                    u64_copy_raw(src, 8, dst, 8);
                } else {
                    loop {
                        let diff = (dst as isize) - (src as isize);
                        if diff >= 8 {
                            break;
                        }
                        u64_copy_raw(src, 0, dst, 0);
                        d += diff as usize;
                        dst = dst.offset(diff);
                    }
                    while d < end {
                        u64_copy_raw(src, 0, dst, 0);
                        src = src.offset(8);
                        dst = dst.offset(8);
                        d += 8;
                    }
                }
            }
            d = end;
        }
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
unsafe fn u64_copy(src: &[u8], srci: usize, dst: &mut [u8], dsti: usize) {
    u64_store_le(u64_load_le(src, srci), dst, dsti);
}

#[inline]
unsafe fn u64_copy_raw(src: *const u8, srci: usize, dst: *mut u8, dsti: usize) {
    u64_store_le_raw(u64_load_le_raw(src, srci), dst, dsti);
}

#[inline]
unsafe fn u64_load_le(buf: &[u8], i: usize) -> u64 {
    debug_assert!(i + 8 <= buf.len());
    u64_load_le_raw(buf.as_ptr(), i)
}

#[inline]
unsafe fn u64_load_le_raw(buf: *const u8, i: usize) -> u64 {
    let mut n: u64 = 0;
    ptr::copy_nonoverlapping(
        buf.offset(i as isize), &mut n as *mut u64 as *mut u8, 8);
    n.to_le()
}

#[inline]
unsafe fn u64_store_le(n: u64, buf: &mut [u8], i: usize) {
    debug_assert!(i + 8 <= buf.len());
    u64_store_le_raw(n, buf.as_mut_ptr(), i);
}

#[inline]
unsafe fn u64_store_le_raw(n: u64, buf: *mut u8, i: usize) {
    ptr::copy_nonoverlapping(
        &n.to_le() as *const u64 as *mut u8, buf.offset(i as isize), 8);
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
