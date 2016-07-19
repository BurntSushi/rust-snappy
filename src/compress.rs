use std::ptr;

use byteorder::{ByteOrder, LittleEndian as LE};

use {
    MAX_INPUT_SIZE,
    Error, Result, Tag,
    write_varu64,
};

const MAX_BLOCK_SIZE: usize = 1<<16;
const MAX_TABLE_SIZE: usize = 1<<14;
const SMALL_TABLE_SIZE: usize = 1<<10;
const INPUT_MARGIN: usize = 16 - 1;
const MIN_NON_LITERAL_BLOCK_SIZE: usize = 1 + 1 + INPUT_MARGIN;

pub fn compress(mut input: &[u8], output: &mut [u8]) -> Result<usize> {
    // println!("\n[compress] len: {:?}", input.len());
    match max_compressed_len(input.len()) {
        0 => {
            return Err(Error::TooBig {
                given: input.len() as u64,
                max: MAX_INPUT_SIZE,
            });
        }
        min if output.len() < min => {
            return Err(Error::BufferTooSmall {
                given: output.len() as u64,
                min: min as u64,
            });
        }
        _ => {}
    }
    if input.is_empty() {
        output[0] = 0;
        return Ok(1);
    }
    let mut small_table = [0; SMALL_TABLE_SIZE];
    let mut big_table = vec![];
    let mut opos = write_varu64(output, input.len() as u64);
    while !input.is_empty() {
        let mut block = input;
        if block.len() > MAX_BLOCK_SIZE {
            block = &block[..MAX_BLOCK_SIZE as usize];
        }
        input = &input[block.len()..];
        if block.len() < MIN_NON_LITERAL_BLOCK_SIZE {
            opos = emit_literal(
                block, 0, block.len(), output, opos, false);
        } else {
            opos += compress_block(
                block, &mut output[opos..], &mut small_table, &mut big_table);
        }
    }
    Ok(opos)
}

pub fn max_compressed_len(input_len: usize) -> usize {
    let input_len = input_len as u64;
    if input_len > MAX_INPUT_SIZE {
        return 0;
    }
    let max = 32 + input_len + (input_len / 6);
    if max > MAX_INPUT_SIZE {
        0
    } else {
        max as usize
    }
}

fn compress_block(
    input: &[u8],
    output: &mut [u8],
    small_table: &mut [u16],
    big_table: &mut Vec<u16>,
) -> usize {
    let mut opos = 0;
    let mut shift: u32 = 32 - 8;
    let mut table_size = 256;
    while table_size < MAX_TABLE_SIZE && table_size < input.len() {
        shift -= 1;
        table_size *= 2;
    }
    let mut table: &mut [u16] =
        if table_size <= SMALL_TABLE_SIZE {
            &mut small_table[0..table_size]
        } else {
            if big_table.is_empty() {
                big_table.resize(MAX_TABLE_SIZE, 0);
            }
            &mut big_table[0..table_size]
        };
    for x in &mut *table {
        *x = 0;
    }
    // println!("shift: {:?}, table_size: {:?}", shift, table_size);
    let hash = |x: u32| { (x.wrapping_mul(0x1E35A7BD) >> shift) as usize };
    let s_limit = input.len() - INPUT_MARGIN;
    let mut next_emit = 0;
    let mut s = 1;
    let mut next_hash = hash(LE::read_u32(&input[s..]));
    loop {
        let mut skip = 32;
        let mut next_s = s;
        let mut candidate = 0;
        loop {
            s = next_s;
            let bytes_between_hash_lookups = skip >> 5;
            next_s = s + bytes_between_hash_lookups;
            skip += bytes_between_hash_lookups;
            if next_s > s_limit {
                if next_emit < input.len() {
                    opos = emit_literal(
                        input, next_emit, input.len(), output, opos, false);
                }
                return opos;
            }
            unsafe {
                candidate = *table.get_unchecked(next_hash) as usize;
                *table.get_unchecked_mut(next_hash) = s as u16;
                next_hash = hash(loadu32_le(input.as_ptr().offset(next_s as isize)));
                let x = loadu32(input.as_ptr().offset(s as isize));
                let y = loadu32(input.as_ptr().offset(candidate as isize));
                if x == y {
                    break;
                }
            }
        }
        debug_assert!(next_emit + 16 <= input.len());
        opos = emit_literal(input, next_emit, s, output, opos, true);
        loop {
            let base = s;
            let matched = 4 + extend_match(input, s + 4, candidate + 4);
            s += matched;
            opos = emit_copy(base - candidate, matched, output, opos);
            next_emit = s;
            if s >= s_limit {
                if next_emit < input.len() {
                    opos = emit_literal(
                        input, next_emit, input.len(), output, opos, false);
                }
                return opos;
            }
            unsafe {
                let x = loadu64_le(input.as_ptr().offset((s - 1) as isize));
                let prev_hash = hash(x as u32);
                *table.get_unchecked_mut(prev_hash) = (s - 1) as u16;
                let cur_hash = hash((x >> 8) as u32);
                candidate = *table.get_unchecked(cur_hash) as usize;
                *table.get_unchecked_mut(cur_hash) = s as u16;

                let y = loadu32_le(input.as_ptr().offset(candidate as isize));
                if (x >> 8) as u32 != y {
                    next_hash = hash((x >> 16) as u32);
                    s += 1;
                    break;
                }
            }
        }
    }
}

#[inline(always)]
fn emit_literal(
    input: &[u8],
    lit_start: usize,
    lit_end: usize,
    output: &mut [u8],
    mut oi: usize,
    allow_fast: bool,
) -> usize {
    // println!("[emit_literal] start: {:?}, end: {:?}, oi: {:?}",
             // lit_start, lit_end, oi);
    let len = lit_end - lit_start;
    let n = len.checked_sub(1).unwrap();
    unsafe {
        if n <= 59 {
            *output.get_unchecked_mut(oi + 0) = ((n as u8) << 2) | (Tag::Literal as u8);
            oi += 1;
            if allow_fast && len <= 16 {
                ptr::copy_nonoverlapping(
                    input.as_ptr().offset(lit_start as isize),
                    output.as_mut_ptr().offset(oi as isize),
                    16);
                return oi + len;
            }
        } else if n < 256 {
            *output.get_unchecked_mut(oi + 0) = (60 << 2) | (Tag::Literal as u8);
            *output.get_unchecked_mut(oi + 1) = n as u8;
            oi += 2;
        } else {
            *output.get_unchecked_mut(oi + 0) = (61 << 2) | (Tag::Literal as u8);
            *output.get_unchecked_mut(oi + 1) = n as u8;
            *output.get_unchecked_mut(oi + 2) = (n >> 8) as u8;
            oi += 3;
        }
        ptr::copy_nonoverlapping(
            input.as_ptr().offset(lit_start as isize),
            output.as_mut_ptr().offset(oi as isize),
            len);
    }
    oi += len;
    oi
}

#[inline(always)]
fn emit_copy(
    offset: usize,
    mut len: usize,
    output: &mut [u8],
    mut i: usize,
) -> usize {
    // println!("[emit_copy] offset: {:?}, len: {:?}", offset, len);
    while len >= 68 {
        i = emit_copy2(offset, 64, output, i);
        len -= 64;
    }
    if len > 64 {
        i = emit_copy2(offset, 60, output, i);
        len -= 60;
    }
    if len <= 11 && offset <= 2047 {
        unsafe {
            *output.get_unchecked_mut(i + 0) =
                (((offset >> 8) as u8) << 5)
                | (((len - 4) as u8) << 2)
                | (Tag::Copy1 as u8);
            *output.get_unchecked_mut(i + 1) = offset as u8;
        }
        i + 2
    } else {
        emit_copy2(offset, len, output, i)
    }
}

#[inline(always)]
fn emit_copy2(
    offset: usize,
    len: usize,
    output: &mut [u8],
    i: usize,
) -> usize {
    unsafe {
        *output.get_unchecked_mut(i + 0) =
            (((len - 1) as u8) << 2) | (Tag::Copy2 as u8);
        *output.get_unchecked_mut(i + 1) = offset as u8;
        *output.get_unchecked_mut(i + 2) = (offset >> 8) as u8;
    }
    i + 3
}

#[inline(always)]
fn extend_match(input: &[u8], mut i: usize, mut j: usize) -> usize {
    let mut matched = 0;
    while i + 8 <= input.len() {
        let x = unsafe { loadu64(input.as_ptr().offset(i as isize)) };
        let y = unsafe { loadu64(input.as_ptr().offset(j as isize)) };
        if x == y {
            i += 8;
            j += 8;
            matched += 8;
        } else {
            let mut z = x.to_le() ^ y.to_le();
            matched += z.trailing_zeros() as usize / 8;
            return matched;
        }
    }
    while i < input.len() && input[i] == input[j] {
        i += 1;
        j += 1;
        matched += 1;
    }
    matched
}

unsafe fn loadu128(data: *const u8) -> (u64, u64) {
    let mut x: (u64, u64) = (0, 0);
    ptr::copy_nonoverlapping(
        data,
        &mut x as *mut (u64, u64) as *mut u8,
        16);
    x
}

unsafe fn loadu64(data: *const u8) -> u64 {
    let mut n: u64 = 0;
    ptr::copy_nonoverlapping(
        data,
        &mut n as *mut u64 as *mut u8,
        8);
    n
}

unsafe fn loadu64_le(data: *const u8) -> u64 {
    loadu64(data).to_le()
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
