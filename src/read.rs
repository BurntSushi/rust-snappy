/*!
This module provides two `std::io::Read` implementations:

- `read::FrameDecoder` wraps another `std::io::Read` implemenation, and
  decompresses data encoded using the Snappy frame format. Use this
  if you have a compressed data source and wish to read it as uncompressed data.
- `read::FrameEncoder` wraps another `std::io::Read` implemenation, and
  compresses data encoded using the Snappy frame format. Use this if you have
  uncompressed data source and wish to read it as compressed data.

Typically, `read::FrameDecoder` is the version that you'll want.
*/

use byteorder::{ReadBytesExt, ByteOrder, LittleEndian as LE};
use std::cmp;
use std::io::{self, Read};

use decompress::{Decoder, decompress_len};
use error::Error;
use frame::{ChunkType, crc32c_masked, MAX_COMPRESS_BLOCK_SIZE,
            STREAM_BODY};
use MAX_BLOCK_SIZE;

/// A reader for decompressing a Snappy stream.
///
/// This `FrameDecoder` wraps any other reader that implements `io::Read`. Bytes
/// read from this reader are decompressed using the
/// [Snappy frame format](https://github.com/google/snappy/blob/master/framing_format.txt)
/// (file extension `sz`, MIME type `application/x-snappy-framed`).
///
/// This reader can potentially make many small reads from the underlying
/// stream depending on its format, therefore, passing in a buffered reader
/// may be beneficial.
pub struct FrameDecoder<R: Read> {
    /// The underlying reader.
    r: R,
    /// A Snappy decoder that we reuse that does the actual block based
    /// decompression.
    dec: Decoder,
    /// The compressed bytes buffer, taken from the underlying reader.
    src: Vec<u8>,
    /// The decompressed bytes buffer. Bytes are decompressed from src to dst
    /// before being passed back to the caller.
    dst: Vec<u8>,
    /// Index into dst: starting point of bytes not yet given back to caller.
    dsts: usize,
    /// Index into dst: ending point of bytes not yet given back to caller.
    dste: usize,
    /// Whether we've read the special stream header or not.
    read_stream_ident: bool,
}

impl<R: Read> FrameDecoder<R> {
    /// Create a new reader for streaming Snappy decompression.
    pub fn new(rdr: R) -> FrameDecoder<R> {
        FrameDecoder {
            r: rdr,
            dec: Decoder::new(),
            src: vec![0; *MAX_COMPRESS_BLOCK_SIZE],
            dst: vec![0; MAX_BLOCK_SIZE],
            dsts: 0,
            dste: 0,
            read_stream_ident: false,
        }
    }

    /// Gets a reference to the underlying reader in this decoder.
    pub fn get_ref(&self) -> &R {
        &self.r
    }
}

impl<R: Read> Read for FrameDecoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        macro_rules! fail {
            ($err:expr) => {
                return Err(io::Error::from($err));
            }
        }
        loop {
            if self.dsts < self.dste {
                let len = cmp::min(self.dste - self.dsts, buf.len());
                let dste = self.dsts.checked_add(len).unwrap();
                buf[0..len].copy_from_slice(&self.dst[self.dsts..dste]);
                self.dsts = dste;
                return Ok(len);
            }
            if !try!(read_exact_eof(&mut self.r, &mut self.src[0..4])) {
                return Ok(0);
            }
            let ty = ChunkType::from_u8(self.src[0]);
            if !self.read_stream_ident {
                if ty != Ok(ChunkType::Stream) {
                    fail!(Error::StreamHeader { byte: self.src[0] });
                }
                self.read_stream_ident = true;
            }
            let len64 = LE::read_uint(&self.src[1..4], 3);
            if len64 > self.src.len() as u64 {
                fail!(Error::UnsupportedChunkLength {
                    len: len64,
                    header: false,
                });
            }
            let len = len64 as usize;
            match ty {
                Err(b) if 0x02 <= b && b <= 0x7F => {
                    // Spec says that chunk types 0x02-0x7F are reserved and
                    // conformant decoders must return an error.
                    fail!(Error::UnsupportedChunkType { byte: b });
                }
                Err(b) if 0x80 <= b && b <= 0xFD => {
                    // Spec says that chunk types 0x80-0xFD are reserved but
                    // skippable.
                    try!(self.r.read_exact(&mut self.src[0..len]));
                }
                Err(b) => {
                    // Can never happen. 0x02-0x7F and 0x80-0xFD are handled
                    // above in the error case. That leaves 0x00, 0x01, 0xFE
                    // and 0xFF, each of which correspond to one of the four
                    // defined chunk types.
                    unreachable!("BUG: unhandled chunk type: {}", b);
                }
                Ok(ChunkType::Padding) => {
                    // Just read and move on.
                    try!(self.r.read_exact(&mut self.src[0..len]));
                }
                Ok(ChunkType::Stream) => {
                    if len != STREAM_BODY.len() {
                        fail!(Error::UnsupportedChunkLength {
                            len: len64,
                            header: true,
                        })
                    }
                    try!(self.r.read_exact(&mut self.src[0..len]));
                    if &self.src[0..len] != STREAM_BODY {
                        fail!(Error::StreamHeaderMismatch {
                            bytes: self.src[0..len].to_vec(),
                        });
                    }
                }
                Ok(ChunkType::Uncompressed) => {
                    let expected_sum = try!(self.r.read_u32::<LE>());
                    let n = len - 4;
                    if n > self.dst.len() {
                        fail!(Error::UnsupportedChunkLength {
                            len: n as u64,
                            header: false,
                        });
                    }
                    try!(self.r.read_exact(&mut self.dst[0..n]));
                    let got_sum = crc32c_masked(&self.dst[0..n]);
                    if expected_sum != got_sum {
                        fail!(Error::Checksum {
                            expected: expected_sum,
                            got: got_sum,
                        });
                    }
                    self.dsts = 0;
                    self.dste = n;
                }
                Ok(ChunkType::Compressed) => {
                    let expected_sum = try!(self.r.read_u32::<LE>());
                    let sn = len - 4;
                    if sn > self.src.len() {
                        fail!(Error::UnsupportedChunkLength {
                            len: len64,
                            header: false,
                        });
                    }
                    try!(self.r.read_exact(&mut self.src[0..sn]));
                    let dn = try!(decompress_len(&self.src));
                    if dn > self.dst.len() {
                        fail!(Error::UnsupportedChunkLength {
                            len: dn as u64,
                            header: false,
                        });
                    }
                    try!(self.dec.decompress(
                        &self.src[0..sn], &mut self.dst[0..dn]));
                    let got_sum = crc32c_masked(&self.dst[0..dn]);
                    if expected_sum != got_sum {
                        fail!(Error::Checksum {
                            expected: expected_sum,
                            got: got_sum,
                        });
                    }
                    self.dsts = 0;
                    self.dste = dn;
                }
            }
        }
    }
}

// read_exact_eof is like Read::read_exact, except it converts an UnexpectedEof
// error to a bool of false.
//
// If no error occurred, then this returns true.
fn read_exact_eof<R: Read>(rdr: &mut R, buf: &mut [u8]) -> io::Result<bool> {
    use std::io::ErrorKind::UnexpectedEof;
    match rdr.read_exact(buf) {
        Ok(()) => Ok(true),
        Err(ref err) if err.kind() == UnexpectedEof => Ok(false),
        Err(err) => Err(err),
    }
}
