/*!
This library provides zero-overhead bindings to Google's Snappy C++ library.

These bindings should only be used in testing and benchmarks.
*/

extern crate libc;

use libc::{c_int, size_t};

/// Compress the bytes in `src` into `dst`. `dst` must be big enough to
/// hold the maximum compressed size of the bytes in `src`.
///
/// If there was a problem compressing `src`, an error is returned.
pub fn compress(src: &[u8], dst: &mut [u8]) -> Result<usize, String> {
    unsafe {
        let mut dst_len = snappy_max_compressed_length(src.len());
        if dst.len() < dst_len {
            return Err(format!(
                "destination buffer too small ({} < {})",
                dst.len(), dst_len));
        }
        snappy_compress(
            src.as_ptr(),
            src.len(),
            dst.as_mut_ptr(),
            &mut dst_len);
        Ok(dst_len)
    }
}

/// Decompress the bytes in `src` into `dst`. `dst` must be big enough to
/// hold the the uncompressed size of the bytes in `src`.
///
/// If there was a problem decompressing `src`, an error is returned.
pub fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, String> {
    unsafe {
        let mut dst_len = 0;
        snappy_uncompressed_length(
            src.as_ptr(), src.len() as size_t, &mut dst_len);
        if dst.len() < dst_len {
            return Err(format!(
                "destination buffer too small ({} < {})", dst.len(), dst_len));
        }
        let r = snappy_uncompress(
            src.as_ptr(),
            src.len(),
            dst.as_mut_ptr(),
            &mut dst_len);
        if r == 0 {
            Ok(dst_len)
        } else {
            Err("snappy: invalid input".to_owned())
        }
    }
}

extern {
    fn snappy_compress(
        input: *const u8,
        input_len: size_t,
        compressed: *mut u8,
        compressed_len: *mut size_t,
    ) -> c_int;

    fn snappy_uncompress(
        compressed: *const u8,
        compressed_len: size_t,
        uncompressed: *mut u8,
        uncompressed_len: *mut size_t,
    ) -> c_int;

    fn snappy_max_compressed_length(input_len: size_t) -> size_t;

    fn snappy_uncompressed_length(
        compressed: *const u8,
        compressed_len: size_t,
        result: *mut size_t,
    ) -> c_int;
}
