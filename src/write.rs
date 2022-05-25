/*!
This module provides a `std::io::Write` implementation:

- `write::FrameEncoder` wraps another `std::io::Write` implemenation, and
  compresses data encoded using the Snappy frame format. Use this if you have
  uncompressed data source and wish to write it as compressed data.

It would also be possible to provide a `write::FrameDecoder`, which decompresses
data as it writes it, but it hasn't been implemented yet.
*/

use std::io::{self, Write};
use std::{cmp, fmt};

use crate::compress::Encoder;
use crate::crc32::CheckSummer;
use crate::decompress::decompress_len;
pub use crate::error::IntoInnerError;
use crate::frame::{
    compress_frame, ChunkType, CHUNK_HEADER_AND_CRC_SIZE,
    MAX_COMPRESS_BLOCK_SIZE, STREAM_BODY, STREAM_IDENTIFIER,
};
use crate::raw::Decoder;
use crate::{bytes, Error, MAX_BLOCK_SIZE};

/// A writer for decompressing a Snappy stream.
///
/// This `FrameDecoder` wraps any other reader that implements `std::io::Write`.
/// Bytes written to this writer are decompressed using the
/// [Snappy frame format](https://github.com/google/snappy/blob/master/framing_format.txt)
/// (file extension `sz`, MIME type `application/x-snappy-framed`).
///
/// Writes are buffered automatically, so there's no need to wrap the given
/// writer in a `std::io::BufWriter`.
///
/// The writer will be flushed automatically when it is dropped. If an error
/// occurs, it is ignored.
pub struct FrameDecoder<W: io::Write> {
    /// The underlying reader.
    ///
    /// An option so we can move out of it.
    w: Option<W>,
    /// A Snappy decoder that we reuse that does the actual block based
    /// decompression.
    dec: Decoder,
    /// A CRC32 checksummer that is configured to either use the portable
    /// fallback version or the SSE4.2 accelerated version when the right CPU
    /// features are available.
    checksummer: CheckSummer,
    /// The compressed bytes buffer, taken from the underlying reader.
    src: Vec<u8>,
    /// Index into src: starting point of bytes not yet decompressed.
    srcs: usize,
    /// Index into src: ending point of bytes not yet decompressed.
    srce: usize,
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

impl<W: io::Write> FrameDecoder<W> {
    /// Create a new writer for streaming Snappy decompression.
    pub fn new(wtr: W) -> FrameDecoder<W> {
        FrameDecoder {
            w: Some(wtr),
            dec: Decoder::new(),
            checksummer: CheckSummer::new(),
            src: vec![0; MAX_COMPRESS_BLOCK_SIZE],
            srcs: 0,
            srce: 0,
            dst: vec![0; MAX_BLOCK_SIZE],
            dsts: 0,
            dste: 0,
            read_stream_ident: false,
        }
    }

    /// Gets a reference to the underlying writer in this decoder.
    pub fn get_ref(&self) -> &W {
        self.w.as_ref().unwrap()
    }

    /// Gets a mutable reference to the underlying writer in this decoder.
    ///
    /// Note that mutation of the stream may result in surprising results if
    /// this decoder is continued to be used.
    pub fn get_mut(&mut self) -> &mut W {
        self.w.as_mut().unwrap()
    }

    /// Finish decoding and return the underlying writer.
    pub fn into_inner(mut self) -> io::Result<W> {
        self.flush()?;
        Ok(self.w.take().unwrap())
    }

    /// Same as [`Self::read_exact`] but also advance `srcs`.
    ///
    /// If this returns [`None`] (we don't have enough data), the pointer isn't advanced.
    fn advance_exact(&mut self, len: usize) -> Option<&[u8]> {
        if len + self.srcs > self.srce {
            return None;
        }
        let range = self.srcs..self.srcs + len;
        self.srcs += len;
        debug_assert!(self.srcs <= self.srce);
        self.src.get(range)
    }
    /// Read `len` bytes from `src` with a start offset of `start`.
    /// Returns [`None`] (which you should pass on to your caller) if
    /// we don't have enough data in `src`.
    fn read_exact(&self, start: usize, len: usize) -> Option<&[u8]> {
        if len + self.srcs + start > self.srce {
            return None;
        }
        Some(&self.src[self.srcs + start..self.srcs + start + len])
    }

    /// Tries to write data from the `src` buffer to our writer.
    ///
    /// Based of the implementation of [`crate::read::FrameDecoder`].
    fn write_from_buffer(&mut self) -> Option<io::Result<()>> {
        macro_rules! fail {
            ($err:expr) => {
                return Some(Err(io::Error::from($err)))
            };
        }
        loop {
            if self.dsts < self.dste {
                let len = self.dste - self.dsts;
                let dste = self.dsts.checked_add(len).unwrap();
                let r =
                    self.w.as_mut().unwrap().write(&self.dst[self.dsts..dste]);
                self.dsts = dste;
                return Some(r.map(|_| ()));
            }
            let first_byte = self.read_exact(0, 4)?[0];
            let ty = ChunkType::from_u8(first_byte);
            if !self.read_stream_ident {
                if ty != Ok(ChunkType::Stream) {
                    fail!(Error::StreamHeader { byte: first_byte });
                }
                self.read_stream_ident = true;
            }
            // we need &mut above, so get the reference again to please borrow checker
            let read = self.read_exact(0, 4)?;
            let len64 = bytes::read_u24_le(&read[1..]) as u64;
            if len64 + self.srcs as u64 > self.srce as u64 {
                return None;
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
                    self.advance_exact(len + 4).unwrap();
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
                    self.advance_exact(len + 4).unwrap();
                }
                Ok(ChunkType::Stream) => {
                    if len != STREAM_BODY.len() {
                        fail!(Error::UnsupportedChunkLength {
                            len: len64,
                            header: true,
                        })
                    }
                    // unwrap: we asserted above that `len` fits, and that `len>=4`.
                    let read = self.read_exact(4, len).unwrap();
                    if &read[0..len] != STREAM_BODY {
                        fail!(Error::StreamHeaderMismatch {
                            bytes: read[0..len].to_vec(),
                        });
                    }
                    self.advance_exact(4 + len).unwrap();
                }
                Ok(ChunkType::Uncompressed) => {
                    if len < 4 {
                        fail!(Error::UnsupportedChunkLength {
                            len: len as u64,
                            header: false,
                        });
                    }
                    // unwrap: we asserted above that `len` fits, and that `len>=4`.
                    let expected_sum =
                        bytes::read_u32_le(self.read_exact(4, 4).unwrap());
                    let n = len - 4;
                    if n > self.dst.len() {
                        fail!(Error::UnsupportedChunkLength {
                            len: n as u64,
                            header: false,
                        });
                    }
                    // inline self.read_exact due to needing to borrow both immutably and mutably
                    //
                    // self.read_exact(8, n)
                    if n + 8 + self.srcs > self.srce {
                        return None;
                    }
                    let read =
                        self.src.get(self.srcs + 8..self.srcs + 8 + n)?;

                    self.dst[0..n].copy_from_slice(read);
                    let got_sum =
                        self.checksummer.crc32c_masked(&self.dst[0..n]);
                    if expected_sum != got_sum {
                        fail!(Error::Checksum {
                            expected: expected_sum,
                            got: got_sum,
                        });
                    }
                    // we read 4 bytes for the chunk type + frame length,
                    // 4 bytes for the expected sum,
                    // and `n` bytes for the data.
                    self.advance_exact(8 + n).unwrap();
                    self.dsts = 0;
                    self.dste = n;
                }
                Ok(ChunkType::Compressed) => {
                    if len < 4 {
                        fail!(Error::UnsupportedChunkLength {
                            len: len as u64,
                            header: false,
                        });
                    }
                    // unwrap: we asserted above that `len` fits, and that `len>=4`.
                    let expected_sum =
                        bytes::read_u32_le(self.read_exact(4, 4).unwrap());
                    let sn = len - 4;
                    if sn > self.src.len() {
                        fail!(Error::UnsupportedChunkLength {
                            len: len64,
                            header: false,
                        });
                    }
                    // inline self.read_exact due to needing to borrow both immutably and mutably
                    //
                    // self.read_exact(8, n)
                    if sn + 8 + self.srcs > self.srce {
                        return None;
                    }
                    let read =
                        self.src.get(self.srcs + 8..self.srcs + 8 + sn)?;

                    let dn = match decompress_len(read) {
                        Err(err) => fail!(err),
                        Ok(dn) => dn,
                    };
                    if dn > self.dst.len() {
                        fail!(Error::UnsupportedChunkLength {
                            len: dn as u64,
                            header: false,
                        });
                    }
                    if let Err(err) =
                        self.dec.decompress(read, &mut self.dst[0..dn])
                    {
                        fail!(err)
                    };
                    let got_sum =
                        self.checksummer.crc32c_masked(&self.dst[0..dn]);
                    if expected_sum != got_sum {
                        fail!(Error::Checksum {
                            expected: expected_sum,
                            got: got_sum,
                        });
                    }
                    // we read 4 bytes for the chunk type + frame length,
                    // 4 bytes for the expected sum,
                    // and `sn` bytes for the data.
                    self.advance_exact(8 + sn).unwrap();
                    self.dsts = 0;
                    self.dste = dn;
                }
            }
        }
    }
}

impl<W: io::Write> io::Write for FrameDecoder<W> {
    fn write(&mut self, mut buf: &[u8]) -> io::Result<usize> {
        let initial_len = buf.len();
        loop {
            if let Some(r) = self.write_from_buffer() {
                r?;
            } else {
                // we can no longer provide more data to the implementation
                // - request more from the caller.
                if buf.is_empty() {
                    return if self.srce == self.srcs {
                        Ok(initial_len - buf.len())
                    } else {
                        Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "snappy wants more data to decompress",
                        ))
                    };
                }
                // move rest of src to start
                let len = self.srce - self.srcs;
                self.src.copy_within(self.srcs..self.srce, 0);
                self.srce = len;
                self.srcs = 0;

                // copy more from `buf`
                let len = cmp::min(self.src.len() - self.srce, buf.len());
                self.src[self.srce..self.srce + len]
                    .copy_from_slice(&buf[..len]);
                self.srce += len;
                buf = &buf[len..];
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        let r = if let Some(r) = self.write_from_buffer() {
            r.map(|_| ())
        } else if self.srce == self.srcs {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "snappy wants more data to decompress",
            ))
        };
        self.w.as_mut().unwrap().flush()?;
        r
    }
}

impl<W: io::Write> Drop for FrameDecoder<W> {
    fn drop(&mut self) {
        if self.w.is_some() {
            // Ignore errors because we can't conceivably return an error and
            // panicing in a dtor is bad juju.
            let _ = self.flush();
        }
    }
}

impl<W: fmt::Debug + io::Write> fmt::Debug for FrameDecoder<W> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("FrameDecoder")
            .field("w", self.w.as_ref().unwrap())
            .field("dec", &self.dec)
            .field("checksummer", &self.checksummer)
            .field("src", &"[...]")
            .field("src_pos", &self.srcs)
            .field("src_len", &self.srce)
            .field("dst", &"[...]")
            .field("dsts", &self.dsts)
            .field("dste", &self.dste)
            .field("read_stream_ident", &self.read_stream_ident)
            .finish()
    }
}

/// A writer for compressing a Snappy stream.
///
/// This `FrameEncoder` wraps any other writer that implements `io::Write`.
/// Bytes written to this writer are compressed using the [Snappy frame
/// format](https://github.com/google/snappy/blob/master/framing_format.txt)
/// (file extension `sz`, MIME type `application/x-snappy-framed`).
///
/// Writes are buffered automatically, so there's no need to wrap the given
/// writer in a `std::io::BufWriter`.
///
/// The writer will be flushed automatically when it is dropped. If an error
/// occurs, it is ignored.
pub struct FrameEncoder<W: io::Write> {
    /// Our main internal state, split out for borrowck reasons (happily paid).
    ///
    /// Also, it's an `Option` so we can move out of it even though
    /// `FrameEncoder` impls `Drop`.
    inner: Option<Inner<W>>,
    /// Our buffer of uncompressed bytes. This isn't part of `inner` because
    /// we may write bytes directly from the caller if the given buffer was
    /// big enough. As a result, the main `write` implementation needs to
    /// accept either the internal buffer or the caller's bytes directly. Since
    /// `write` requires a mutable borrow, we satisfy the borrow checker by
    /// separating `src` from the rest of the state.
    src: Vec<u8>,
}

struct Inner<W> {
    /// The underlying writer.
    w: W,
    /// An encoder that we reuse that does the actual block based compression.
    enc: Encoder,
    /// A CRC32 checksummer that is configured to either use the portable
    /// fallback version or the SSE4.2 accelerated version when the right CPU
    /// features are available.
    checksummer: CheckSummer,
    /// The compressed bytes buffer. Bytes are compressed from src (usually)
    /// to dst before being written to w.
    dst: Vec<u8>,
    /// When false, the stream identifier (with magic bytes) must precede the
    /// next write.
    wrote_stream_ident: bool,
    /// Space for writing the header of a chunk before writing it to the
    /// underlying writer.
    chunk_header: [u8; 8],
}

impl<W: io::Write> FrameEncoder<W> {
    /// Create a new writer for streaming Snappy compression.
    pub fn new(wtr: W) -> FrameEncoder<W> {
        FrameEncoder {
            inner: Some(Inner {
                w: wtr,
                enc: Encoder::new(),
                checksummer: CheckSummer::new(),
                dst: vec![0; MAX_COMPRESS_BLOCK_SIZE],
                wrote_stream_ident: false,
                chunk_header: [0; CHUNK_HEADER_AND_CRC_SIZE],
            }),
            src: Vec::with_capacity(MAX_BLOCK_SIZE),
        }
    }

    /// Returns the underlying stream, consuming and flushing this writer.
    ///
    /// If flushing the writer caused an error, then an `IntoInnerError` is
    /// returned, which contains both the writer and the original writer.
    pub fn into_inner(mut self) -> Result<W, IntoInnerError<FrameEncoder<W>>> {
        match self.flush() {
            Ok(()) => Ok(self.inner.take().unwrap().w),
            Err(err) => Err(IntoInnerError::new(self, err)),
        }
    }

    /// Gets a reference to the underlying writer in this encoder.
    pub fn get_ref(&self) -> &W {
        &self.inner.as_ref().unwrap().w
    }

    /// Gets a reference to the underlying writer in this encoder.
    ///
    /// Note that mutating the output/input state of the stream may corrupt
    /// this encoder, so care must be taken when using this method.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner.as_mut().unwrap().w
    }
}

impl<W: io::Write> Drop for FrameEncoder<W> {
    fn drop(&mut self) {
        if self.inner.is_some() {
            // Ignore errors because we can't conceivably return an error and
            // panicing in a dtor is bad juju.
            let _ = self.flush();
        }
    }
}

impl<W: io::Write> io::Write for FrameEncoder<W> {
    fn write(&mut self, mut buf: &[u8]) -> io::Result<usize> {
        let mut total = 0;
        // If there isn't enough room to add buf to src, then add only a piece
        // of it, flush it and mush on.
        loop {
            let free = self.src.capacity() - self.src.len();
            // n is the number of bytes extracted from buf.
            let n = if buf.len() <= free {
                break;
            } else if self.src.is_empty() {
                // If buf is bigger than our entire buffer then avoid
                // the indirection and write the buffer directly.
                self.inner.as_mut().unwrap().write(buf)?
            } else {
                self.src.extend_from_slice(&buf[0..free]);
                self.flush()?;
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
        self.inner.as_mut().unwrap().write(&self.src)?;
        self.src.truncate(0);
        Ok(())
    }
}

impl<W: io::Write> Inner<W> {
    fn write(&mut self, mut buf: &[u8]) -> io::Result<usize> {
        let mut total = 0;
        if !self.wrote_stream_ident {
            self.wrote_stream_ident = true;
            self.w.write_all(STREAM_IDENTIFIER)?;
        }
        while !buf.is_empty() {
            // Advance buf and get our block.
            let mut src = buf;
            if src.len() > MAX_BLOCK_SIZE {
                src = &src[0..MAX_BLOCK_SIZE];
            }
            buf = &buf[src.len()..];

            let frame_data = compress_frame(
                &mut self.enc,
                self.checksummer,
                src,
                &mut self.chunk_header,
                &mut self.dst,
                false,
            )?;
            self.w.write_all(&self.chunk_header)?;
            self.w.write_all(frame_data)?;
            total += src.len();
        }
        Ok(total)
    }
}

impl<W: fmt::Debug + io::Write> fmt::Debug for FrameEncoder<W> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("FrameEncoder")
            .field("inner", &self.inner)
            .field("src", &"[...]")
            .finish()
    }
}

impl<W: fmt::Debug + io::Write> fmt::Debug for Inner<W> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Inner")
            .field("w", &self.w)
            .field("enc", &self.enc)
            .field("checksummer", &self.checksummer)
            .field("dst", &"[...]")
            .field("wrote_stream_ident", &self.wrote_stream_ident)
            .field("chunk_header", &self.chunk_header)
            .finish()
    }
}
