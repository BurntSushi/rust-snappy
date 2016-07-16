use libc::{c_int, size_t};

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

pub fn compress(src: &[u8], dst: &mut [u8]) -> Result<usize, ()> {
    unsafe {
        let mut dst_len = snappy_max_compressed_length(src.len());
        if dst.len() < dst_len {
            panic!("destination buffer too small ({} < {})",
                   dst.len(), dst_len);
        }
        snappy_compress(
            src.as_ptr(),
            src.len(),
            dst.as_mut_ptr(),
            &mut dst_len);
        Ok(dst_len)
    }
}

pub fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, ()> {
    unsafe {
        let mut dst_len = 0;
        snappy_uncompressed_length(
            src.as_ptr(), src.len() as size_t, &mut dst_len);
        if dst.len() < dst_len {
            panic!("destination buffer too small ({} < {})",
                   dst.len(), dst_len);
        }
        let r = snappy_uncompress(
            src.as_ptr(),
            src.len(),
            dst.as_mut_ptr(),
            &mut dst_len);
        if r == 0 {
            Ok(dst_len)
        } else {
            panic!("snappy: invalid input")
        }
    }
}
