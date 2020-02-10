// Sadly, all of the crc crates in Rust appear to be insufficient for high
// performance algorithms. In fact, the implementation here is insufficient!
// The best implementation uses the CRC32 instruction from SSE 4.2, but using
// specialty instructions on stable Rust is an absolute nightmare at the
// moment. The only way I can think to do it is to write some Assembly in a
// separate source file and run an Assembler from build.rs. But we'd need to be
// careful to only do this on platforms that support the CRC32 instruction,
// which means we'd need to query CPUID. There are a couple Rust crates for
// that, but of course, they only work on unstable Rust so we'd need to hand
// roll that too.
//
// As a stopgap, we implement the fastest (slicing-by-16) algorithm described
// here: http://create.stephan-brumme.com/crc32/
// (Actually, slicing-by-16 doesn't seem to be measurably faster than
// slicing-by-8 on my i7-6900K, so use slicing-by-8.)
//
// For Snappy, we only care about CRC32C (32 bit, Castagnoli).
//
// ---AG

// We have a bunch of different implementations of crc32 below that seem
// somewhat useful to leave around for easy benchmarking.
#![allow(dead_code)]

use crate::bytes;
use crate::crc32_table::{CASTAGNOLI_POLY, TABLE, TABLE16};

/// Returns the CRC32 checksum of `buf` using the Castagnoli polynomial.
pub fn crc32c(buf: &[u8]) -> u32 {
    // I can't measure any difference between slice8 and slice16.
    crc32c_slice8(buf)
}

/// Returns the CRC32 checksum of `buf` using the Castagnoli polynomial.
fn crc32c_slice16(mut buf: &[u8]) -> u32 {
    let mut crc: u32 = !0;
    while buf.len() >= 16 {
        crc ^= bytes::read_u32_le(buf);
        crc = TABLE16[0][buf[15] as usize]
            ^ TABLE16[1][buf[14] as usize]
            ^ TABLE16[2][buf[13] as usize]
            ^ TABLE16[3][buf[12] as usize]
            ^ TABLE16[4][buf[11] as usize]
            ^ TABLE16[5][buf[10] as usize]
            ^ TABLE16[6][buf[9] as usize]
            ^ TABLE16[7][buf[8] as usize]
            ^ TABLE16[8][buf[7] as usize]
            ^ TABLE16[9][buf[6] as usize]
            ^ TABLE16[10][buf[5] as usize]
            ^ TABLE16[11][buf[4] as usize]
            ^ TABLE16[12][(crc >> 24) as u8 as usize]
            ^ TABLE16[13][(crc >> 16) as u8 as usize]
            ^ TABLE16[14][(crc >> 8) as u8 as usize]
            ^ TABLE16[15][(crc) as u8 as usize];
        buf = &buf[16..];
    }
    for &b in buf {
        crc = TABLE[((crc as u8) ^ b) as usize] ^ (crc >> 8);
    }
    !crc
}

/// Returns the CRC32 checksum of `buf` using the Castagnoli polynomial.
fn crc32c_slice8(mut buf: &[u8]) -> u32 {
    let mut crc: u32 = !0;
    while buf.len() >= 8 {
        crc ^= bytes::read_u32_le(buf);
        crc = TABLE16[0][buf[7] as usize]
            ^ TABLE16[1][buf[6] as usize]
            ^ TABLE16[2][buf[5] as usize]
            ^ TABLE16[3][buf[4] as usize]
            ^ TABLE16[4][(crc >> 24) as u8 as usize]
            ^ TABLE16[5][(crc >> 16) as u8 as usize]
            ^ TABLE16[6][(crc >> 8) as u8 as usize]
            ^ TABLE16[7][(crc) as u8 as usize];
        buf = &buf[8..];
    }
    for &b in buf {
        crc = TABLE[((crc as u8) ^ b) as usize] ^ (crc >> 8);
    }
    !crc
}

/// Returns the CRC32 checksum of `buf` using the Castagnoli polynomial.
fn crc32c_slice4(mut buf: &[u8]) -> u32 {
    let mut crc: u32 = !0;
    while buf.len() >= 4 {
        crc ^= bytes::read_u32_le(buf);
        crc = TABLE16[0][(crc >> 24) as u8 as usize]
            ^ TABLE16[1][(crc >> 16) as u8 as usize]
            ^ TABLE16[2][(crc >> 8) as u8 as usize]
            ^ TABLE16[3][(crc) as u8 as usize];
        buf = &buf[4..];
    }
    for &b in buf {
        crc = TABLE[((crc as u8) ^ b) as usize] ^ (crc >> 8);
    }
    !crc
}

/// Returns the CRC32 checksum of `buf` using the Castagnoli polynomial.
fn crc32c_multiple(buf: &[u8]) -> u32 {
    let mut crc: u32 = !0;
    for &b in buf {
        crc = TABLE[((crc as u8) ^ b) as usize] ^ (crc >> 8);
    }
    !crc
}

/// Returns the CRC32 checksum of `buf` using the Castagnoli polynomial.
fn crc32c_bitwise(buf: &[u8]) -> u32 {
    let mut crc: u32 = !0;
    for &b in buf {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ ((crc & 1) * CASTAGNOLI_POLY);
        }
    }
    !crc
}
