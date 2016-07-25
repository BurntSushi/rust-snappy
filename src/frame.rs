#![allow(missing_docs)]
#![allow(dead_code, unused_variables)]

use std::io::{self, Write};

use byteorder::{ByteOrder, LittleEndian as LE};

use {
    MAX_BLOCK_SIZE,
    Encoder,
    max_compressed_len,
};

lazy_static! {
    static ref MAX_COMPRESSED_BLOCK_SIZE: usize =
        max_compressed_len(MAX_BLOCK_SIZE);
}

const STREAM_IDENTIFIER: &'static [u8] = b"\xFF\x06\x00\x00sNaPpY";

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum ChunkType {
    Stream = 0xFF,
    Compressed = 0x00,
    Uncompressed = 0x01,
    Padding = 0xFE,
}

pub struct Writer<W: Write> {
    inner: Inner<W>,
    src: Vec<u8>,
}

struct Inner<W> {
    w: W,
    enc: Encoder,
    dst: Vec<u8>,
    wrote_stream_ident: bool,
    chunk_header: [u8; 8],
}

impl<W: Write> Writer<W> {
    pub fn new(w: W) -> Writer<W> {
        Writer {
            inner: Inner {
                w: w,
                enc: Encoder::new(),
                dst: vec![0; *MAX_COMPRESSED_BLOCK_SIZE],
                wrote_stream_ident: false,
                chunk_header: [0; 8],
            },
            src: Vec::with_capacity(MAX_BLOCK_SIZE),
        }
    }

    fn reset(&mut self, w: W) {
        self.inner.w = w;
        self.src.truncate(0);
        self.inner.wrote_stream_ident = false;
    }
}

impl<W: Write> Drop for Writer<W> {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

impl<W: Write> Write for Writer<W> {
    fn write(&mut self, mut buf: &[u8]) -> io::Result<usize> {
        let mut total = 0;
        // If there isn't enough room to add buf to src, then add only a piece
        // of it, flush it and mush on.
        loop {
            let free = self.src.capacity() - self.src.len();
            // n is the number of bytes extracted from buf.
            let n =
                if buf.len() <= free {
                    break;
                } else if self.src.is_empty() {
                    // If buf is bigger than our entire buffer then avoid
                    // the indirection and write the buffer directly.
                    try!(self.inner.write(buf))
                } else {
                    self.src.extend_from_slice(&buf[0..free]);
                    try!(self.flush());
                    free
                };
            buf = &buf[n..];
            total += n;
        }
        // We're only here if buf.len() will fit within the available space of
        // self.src.
        debug_assert!(buf.len() <= (self.src.capacity() - self.src.len()));
        self.src.extend_from_slice(buf);
        total += buf.len();
        // We should never expand or contract self.src.
        debug_assert!(self.src.capacity() == MAX_BLOCK_SIZE);
        Ok(total)
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.src.is_empty() {
            return Ok(());
        }
        try!(self.inner.write(&self.src));
        self.src.truncate(0);
        Ok(())
    }
}

impl<W: Write> Inner<W> {
    fn write(&mut self, mut buf: &[u8]) -> io::Result<usize> {
        let mut total = 0;
        if !self.wrote_stream_ident {
            self.wrote_stream_ident = true;
            try!(self.w.write_all(STREAM_IDENTIFIER));
            total += STREAM_IDENTIFIER.len();
        }
        while !buf.is_empty() {
            // Advance buf and get our block.
            let mut src = buf;
            if src.len() > MAX_BLOCK_SIZE {
                src = &src[0..MAX_BLOCK_SIZE];
            }
            buf = &buf[src.len()..];
            let checksum = crc32c(src);

            // Compress the buffer. If compression sucked, throw it out and
            // write uncompressed bytes instead. Since our buffer is at most
            // MAX_BLOCK_SIZE and our dst buffer has size
            // max_compressed_len(MAX_BLOCK_SIZE), we have enough space.
            let compress_len = match self.enc.compress(src, &mut self.dst) {
                Ok(n) => n,
                Err(err) => {
                    return Err(io::Error::new(io::ErrorKind::Other, err));
                }
            };
            let (chunk_type, chunk_len) =
                if compress_len >= src.len() - (src.len() / 8) {
                    (ChunkType::Uncompressed, 4 + src.len())
                } else {
                    (ChunkType::Compressed, 4 + compress_len)
                };

            self.chunk_header[0] = chunk_type as u8;
            LE::write_uint(&mut self.chunk_header[1..], chunk_len as u64, 3);
            // panic!("{} {} {}", src.len(), src[0], checksum);
            LE::write_u32(&mut self.chunk_header[4..], checksum);
            try!(self.w.write_all(&self.chunk_header));
            if chunk_type == ChunkType::Compressed {
                try!(self.w.write_all(&self.dst[0..compress_len]))
            } else {
                try!(self.w.write_all(src))
            };
            total += src.len();
        }
        Ok(total)
    }
}

fn crc32c(buf: &[u8]) -> u32 {
    // TODO(burntsushi): SSE 4.2 has a CRC32 instruction that might be faster.
    // Oh how I long for you, SIMD.
    let sum = ::crc::crc32::checksum_castagnoli(buf);
    (sum.wrapping_shr(15) | sum.wrapping_shl(17)).wrapping_add(0xA282EAD8)
}
