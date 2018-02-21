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

use compress::Encoder;
use decompress::{Decoder, decompress_len};
use error::Error;
use frame::{CHUNK_HEADER_AND_CRC_SIZE, ChunkType, compress_frame, crc32c_masked,
            MAX_COMPRESS_BLOCK_SIZE, STREAM_BODY, STREAM_IDENTIFIER};
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

/// The maximum block that `FrameEncoder` can output in a single read operation.
lazy_static! {
    static ref MAX_READ_FRAME_ENCODER_BLOCK_SIZE: usize = (
        STREAM_IDENTIFIER.len() + CHUNK_HEADER_AND_CRC_SIZE
            + *MAX_COMPRESS_BLOCK_SIZE
    );
}

/// A reader for compressing data using snappy as it is read. Usually you'll
/// want `snap::read::FrameDecoder` (for decompressing while reading) or
/// `snap::write::FrameEncoder` (for compressing while writing) instead.
pub struct FrameEncoder<R: Read> {
    /// Internally, we split `FrameEncoder` in two to keep the borrow checker
    /// happy. The `inner` member contains everything that `read_frame` needs
    /// to fetch a frame's worth of data and compress it.
    inner: Inner<R>,

    /// Data that we've encoded and are ready to return to our caller.
    dst: Vec<u8>,

    /// Starting point of bytes in `dst` not yet given back to the caller.
    dsts: usize,

    /// Ending point of bytes in `dst` that we want to give to our caller.
    dste: usize,
}

struct Inner<R: Read> {
    /// The underlying data source.
    r: R,

    /// An encoder that we reuse that does the actual block based compression.
    enc: Encoder,

    /// Data taken from the underlying `r`, and not yet compressed.
    src: Vec<u8>,

    /// Have we written the standard snappy header to `dst` yet?
    wrote_stream_ident: bool,
}

impl<R: Read> FrameEncoder<R> {
    /// Create a new reader for streaming Snappy compression.
    pub fn new(rdr: R) -> FrameEncoder<R> {
        FrameEncoder {
            inner: Inner {
                r: rdr,
                enc: Encoder::new(),
                src: Vec::with_capacity(MAX_BLOCK_SIZE),
                wrote_stream_ident: false,
            },
            dst: vec![0; *MAX_READ_FRAME_ENCODER_BLOCK_SIZE],
            dsts: 0,
            dste: 0,
        }
    }

    /// Gets a reference to the underlying reader in this decoder.
    pub fn get_ref(&self) -> &R {
        &self.inner.r
    }

    /// Read previously compressed data from `self.dst`, returning the number of
    /// bytes read. If `self.dst` is empty, returns 0.
    fn read_from_dst(&mut self, buf: &mut [u8]) -> usize {
        let available_bytes = self.dste - self.dsts;
        let count = cmp::min(available_bytes, buf.len());
        buf[..count].copy_from_slice(&self.dst[self.dsts..self.dsts+count]);
        self.dsts += count;
        count
    }
}

impl<R: Read> Read for FrameEncoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Try reading previously compressed bytes from our `dst` buffer, if
        // any.
        let count = self.read_from_dst(buf);

        if count > 0 {
            // We had some bytes in our `dst` buffer that we used.
            Ok(count)
        } else if buf.len() >= *MAX_READ_FRAME_ENCODER_BLOCK_SIZE {
            // Our output `buf` is big enough that we can directly write into
            // it, so bypass `dst` entirely.
            self.inner.read_frame(buf)
        } else {
            // We need to refill `self.dst`, and then return some bytes from
            // that.
            let count = try!(self.inner.read_frame(&mut self.dst));
            self.dsts = 0;
            self.dste = count;
            Ok(self.read_from_dst(buf))
        }
    }
}

impl<R: Read> Inner<R> {
    /// Read from `self.r`, and create a new frame, writing it to `dst`, which
    /// must be at least `*MAX_READ_FRAME_ENCODER_BLOCK_SIZE` bytes in size.
    fn read_frame(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        debug_assert!(dst.len() >= *MAX_READ_FRAME_ENCODER_BLOCK_SIZE);

        // Try to read a max-sized block to compress from the underlying stream.
        // This is surprisingly complicated in Rust, requiring us to pass a
        // zero-length mutable buffer and get it filled, and requiring the
        // use of the `by_ref().take(...).read_to_end(...)` idiom to read
        // up to the specified number of bytes from a file.
        self.src.truncate(0);
        try!(self.r.by_ref().take(MAX_BLOCK_SIZE as u64)
            .read_to_end(&mut self.src));
        debug_assert!(self.src.len() <= MAX_BLOCK_SIZE);
        if self.src.len() == 0 {
            return Ok(0);
        }

        // If we haven't yet written the stream header to `dst`, write it.
        let mut dst_write_start = 0;
        if !self.wrote_stream_ident {
            dst[0..STREAM_IDENTIFIER.len()]
                .copy_from_slice(STREAM_IDENTIFIER);
            dst_write_start += STREAM_IDENTIFIER.len();
            self.wrote_stream_ident = true;
        }

        // Reserve space for our chunk header. We need to use `split_at_mut` so
        // that we can get two mutable slices pointing at non-overlapping parts
        // of `dst`.
        let (chunk_header, remaining_dst) = dst[dst_write_start..]
            .split_at_mut(CHUNK_HEADER_AND_CRC_SIZE);
        dst_write_start += CHUNK_HEADER_AND_CRC_SIZE;

        // Compress our frame if possible, telling `compress_frame` to always
        // put the output in `dst`.
        let frame_data = try!(compress_frame(
            &mut self.enc,
            &self.src,
            chunk_header,
            remaining_dst,
            true,
        ));
        Ok(dst_write_start + frame_data.len())
    }
}
