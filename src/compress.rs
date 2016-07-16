use byteorder::{ByteOrder, LittleEndian as LE};

use {
    MAX_BLOCK_SIZE, MAX_INPUT_SIZE,
    Error, Result, Tag,
    write_varu64,
};

const MAX_TABLE_SIZE: usize = 1<<14;
const INPUT_MARGIN: usize = 16 - 1;
const MIN_NON_LITERAL_BLOCK_SIZE: usize = 1 + 1 + INPUT_MARGIN;
const TABLE_MASK: usize = MAX_TABLE_SIZE - 1;

pub fn compress(mut input: &[u8], output: &mut [u8]) -> Result<usize> {
    if input.is_empty() {
        return Ok(0);
    }
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
    let mut opos = write_varu64(output, input.len() as u64);
    while !input.is_empty() {
        let mut block = input;
        if block.len() > MAX_BLOCK_SIZE {
            block = &block[..MAX_BLOCK_SIZE as usize];
        }
        input = &input[block.len()..];
        if block.len() < MIN_NON_LITERAL_BLOCK_SIZE {
            opos += emit_literal(block, &mut output[opos..]);
        } else {
            opos += compress_block(block, &mut output[opos..]);
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

fn compress_block(input: &[u8], output: &mut [u8]) -> usize {
    let mut opos = 0;
    let mut table: [u16; MAX_TABLE_SIZE] = [0; MAX_TABLE_SIZE];
    let mut shift = 32 - 8;
    let mut table_size = 256;
    while table_size < MAX_TABLE_SIZE && table_size < input.len() {
        shift -= 1;
        table_size *= 2;
    }
    let hash = |x: usize| { (x * 0x1E35A7BD) >> shift };
    let s_limit = input.len() - INPUT_MARGIN;
    let mut next_emit = 0;
    let mut s = 1;
    let mut next_hash = hash(LE::read_u32(&input[s..]) as usize);
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
                    opos += emit_literal(
                        &input[next_emit..], &mut output[opos..]);
                }
                return opos;
            }
            candidate = table[next_hash & TABLE_MASK] as usize;
            table[next_hash & TABLE_MASK] = s as u16;
            next_hash = hash(LE::read_u32(&input[next_s..]) as usize);
            if LE::read_u32(&input[s..]) == LE::read_u32(&input[candidate..]) {
                break;
            }
        }
        opos += emit_literal(&input[next_emit..s], &mut output[opos..]);
        loop {
            let base = s;
            s += 4;
            let mut i = candidate + 4;
            while s < input.len() && input[i] == input[s] {
                i += 1;
                s += 1;
            }
            opos += emit_copy(base - candidate, s - base, &mut output[opos..]);
            next_emit = s;
            if s >= s_limit {
                if next_emit < input.len() {
                    opos += emit_literal(
                        &input[next_emit..], &mut output[opos..]);
                }
                return opos;
            }
            let x = LE::read_u64(&input[s-1..]);
            let prev_hash = hash(x as u32 as usize);
            table[prev_hash & TABLE_MASK] = (s - 1) as u16;
            let cur_hash = hash((x >> 8) as u32 as usize);
            candidate = table[cur_hash & TABLE_MASK] as usize;
            table[cur_hash & TABLE_MASK] = s as u16;
            if (x >> 8) as u32 != LE::read_u32(&input[candidate..]) {
                next_hash = hash((x >> 16) as u32 as usize);
                s += 1;
                break;
            }
        }
    }
}

fn emit_literal(literal: &[u8], output: &mut [u8]) -> usize {
    let n = literal.len().checked_sub(1).unwrap();
    let mut start = 0;
    if n <= 59 {
        output[0] = ((n as u8) << 2) | (Tag::Literal as u8);
        start = 1;
    } else if n < 256 {
        output[0] = (60 << 2) | (Tag::Literal as u8);
        output[1] = n as u8;
        start = 2;
    } else if n < 65536 {
        output[0] = (61 << 2) | (Tag::Literal as u8);
        output[1] = n as u8;
        output[2] = (n >> 8) as u8;
        start = 3;
    } else {
        unreachable!();
    }
    output[start..start + literal.len()].copy_from_slice(literal);
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
