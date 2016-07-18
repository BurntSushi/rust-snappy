use quickcheck::{QuickCheck, StdGen};
use snappy_cpp as cpp;

use {compress, decompress, decompress_len, max_compressed_len};

macro_rules! roundtrip {
    ($name:ident, $data:expr) => {
        #[test]
        fn $name() {
            let data = &$data[..];
            assert_eq!(data, &*roundtrip(data));
        }
    }
}

macro_rules! cmpcpp {
    ($name:ident, $data:expr) => {
        #[test]
        fn $name() {
            let data = &$data[..];
            let rust = press(data);
            let cpp = press_cpp(data);
            if rust == cpp {
                return;
            }
            panic!("compression results are not equal!
original (len == {:?})
----------------------
{:?}

rust (len == {:?})
------------------
{:?}

cpp (len == {:?})
-----------------
{:?}
", data.len(), data, rust.len(), rust, cpp.len(), cpp);
        }
    }
}

#[test]
fn qc_roundtrip() {
    fn p(bytes: Vec<u8>) -> bool {
        roundtrip(&bytes) == bytes
    }
    QuickCheck::new()
        .gen(StdGen::new(::rand::thread_rng(), 10_000))
        .tests(10_000)
        .quickcheck(p as fn(_) -> _);
}

#[test]
fn qc_cpp_compress() {
    fn p(bytes: Vec<u8>) -> bool {
        press(&bytes) == press_cpp(&bytes)
    }
    QuickCheck::new()
        .gen(StdGen::new(::rand::thread_rng(), 10_000))
        .tests(10_000)
        .quickcheck(p as fn(_) -> _);
}

roundtrip!(roundtrip_empty, []);

roundtrip!(roundtrip_one_zero, [0]);

roundtrip!(roundtrip_random1, [
    0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 1, 1,
    0, 0, 1, 2, 0, 0, 2, 1, 0, 0, 2, 2, 0, 0, 0, 6, 0, 0, 3, 1, 0, 0, 0, 7, 0,
    0, 1, 3, 0, 0, 0, 8, 0, 0, 2, 3, 0, 0, 0, 9, 0, 0, 1, 4, 0, 0, 1, 0, 0, 3,
    0, 0, 1, 0, 1, 0, 0, 0, 10, 0, 0, 0, 0, 2, 4, 0, 0, 2, 0, 0, 3, 0, 1, 0, 0,
    1, 5, 0, 0, 6, 0, 0, 0, 0, 11, 0, 0, 1, 6, 0, 0, 1, 7, 0, 0, 0, 12, 0, 0,
    3, 2, 0, 0, 0, 13, 0, 0, 2, 5, 0, 0, 0, 3, 3, 0, 0, 0, 1, 8, 0, 0, 1, 0,
    1, 0, 0, 0, 4, 1, 0, 0, 0, 0, 14, 0, 0, 0, 1, 9, 0, 0, 0, 1, 10, 0, 0, 0,
    0, 1, 11, 0, 0, 0, 1, 0, 2, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 5, 1, 0, 0, 0, 1,
    2, 1, 0, 0, 0, 0, 0, 2, 6, 0, 0, 0, 0, 0, 1, 12, 0, 0, 0, 0, 0, 3, 4, 0, 0,
    0, 0, 0, 7, 0, 0, 0, 0, 0, 1, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
]);

roundtrip!(roundtrip_random2, [
    10, 2, 14, 13, 0, 8, 2, 10, 2, 14, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
]);

roundtrip!(roundtrip_random3, [
    0, 0, 0, 4, 1, 4, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
]);

cmpcpp!(cmpcpp_empty, []);

cmpcpp!(cmpcpp_one_zero, [0]);

cmpcpp!(cmpcpp_random1, [
    0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 1, 1,
    0, 0, 1, 2, 0, 0, 2, 1, 0, 0, 2, 2, 0, 0, 0, 6, 0, 0, 3, 1, 0, 0, 0, 7, 0,
    0, 1, 3, 0, 0, 0, 8, 0, 0, 2, 3, 0, 0, 0, 9, 0, 0, 1, 4, 0, 0, 1, 0, 0, 3,
    0, 0, 1, 0, 1, 0, 0, 0, 10, 0, 0, 0, 0, 2, 4, 0, 0, 2, 0, 0, 3, 0, 1, 0, 0,
    1, 5, 0, 0, 6, 0, 0, 0, 0, 11, 0, 0, 1, 6, 0, 0, 1, 7, 0, 0, 0, 12, 0, 0,
    3, 2, 0, 0, 0, 13, 0, 0, 2, 5, 0, 0, 0, 3, 3, 0, 0, 0, 1, 8, 0, 0, 1, 0,
    1, 0, 0, 0, 4, 1, 0, 0, 0, 0, 14, 0, 0, 0, 1, 9, 0, 0, 0, 1, 10, 0, 0, 0,
    0, 1, 11, 0, 0, 0, 1, 0, 2, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 5, 1, 0, 0, 0, 1,
    2, 1, 0, 0, 0, 0, 0, 2, 6, 0, 0, 0, 0, 0, 1, 12, 0, 0, 0, 0, 0, 3, 4, 0, 0,
    0, 0, 0, 7, 0, 0, 0, 0, 0, 1, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
]);

cmpcpp!(cmpcpp_random2, [
    10, 2, 14, 13, 0, 8, 2, 10, 2, 14, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
]);

cmpcpp!(cmpcpp_random3, [
    0, 0, 0, 4, 1, 4, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
]);

fn roundtrip(bytes: &[u8]) -> Vec<u8> {
    depress(&press(bytes))
}

fn press(bytes: &[u8]) -> Vec<u8> {
    let mut buf = vec![0; max_compressed_len(bytes.len())];
    let n = compress(bytes, &mut buf).unwrap();
    buf.truncate(n);
    buf
}

fn depress(bytes: &[u8]) -> Vec<u8> {
    let mut buf = vec![0; decompress_len(bytes).unwrap()];
    let m = decompress(bytes, &mut buf).unwrap();
    buf
}

fn press_cpp(bytes: &[u8]) -> Vec<u8> {
    let mut buf = vec![0; max_compressed_len(bytes.len())];
    let n = cpp::compress(bytes, &mut buf).unwrap();
    buf.truncate(n);
    buf
}

fn depress_cpp(bytes: &[u8]) -> Vec<u8> {
    let mut buf = vec![0; decompress_len(bytes).unwrap()];
    let m = cpp::decompress(bytes, &mut buf).unwrap();
    buf
}
