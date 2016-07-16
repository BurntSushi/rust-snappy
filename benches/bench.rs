#![feature(test)]

#[macro_use]
extern crate lazy_static;
extern crate snap;
extern crate test;

macro_rules! compress {
    ($name:ident, $filename:expr) => {
        compress!($name, $filename, 0);
    };
    ($name:ident, $filename:expr, $size:expr) => {
        #[bench]
        fn $name(b: &mut test::Bencher) {
            lazy_static! {
                static ref SRC: Vec<u8> = {
                    let src = include_bytes!(concat!("../data/", $filename));
                    let mut src = &src[..];
                    if $size > 0 {
                        src = &src[0..$size];
                    }
                    src.to_owned()
                };
            };
            let mut dst = vec![0; snap::max_compressed_len(SRC.len())];
            b.bytes = SRC.len() as u64;
            b.iter(|| {
                snap::compress(SRC.as_slice(), &mut dst).unwrap()
            });
        }
    };
}

compress!(zflat00, "html");
compress!(zflat01, "urls.10K");
compress!(zflat02, "fireworks.jpeg");
compress!(zflat03, "fireworks.jpeg", 200);
compress!(zflat04, "paper-100k.pdf");
compress!(zflat05, "html_x_4");
compress!(zflat06, "alice29.txt");
compress!(zflat07, "asyoulik.txt");
compress!(zflat08, "lcet10.txt");
compress!(zflat09, "plrabn12.txt");
compress!(zflat10, "geo.protodata");
compress!(zflat11, "kppkn.gtb");

macro_rules! decompress {
    ($name:ident, $filename:expr) => {
        decompress!($name, $filename, 0);
    };
    ($name:ident, $filename:expr, $size:expr) => {
        #[bench]
        fn $name(b: &mut test::Bencher) {
            lazy_static! {
                static ref SRC: Vec<u8> = {
                    let src = include_bytes!(concat!("../data/", $filename));
                    let mut src = &src[..];
                    if $size > 0 {
                        src = &src[0..$size];
                    }
                    src.to_owned()
                };
                static ref COMPRESSED: Vec<u8> = {
                    let len = snap::max_compressed_len(SRC.len());
                    let mut compressed = vec![0; len];
                    let n = snap::compress(
                        SRC.as_slice(), &mut compressed).unwrap();
                    compressed.truncate(n);
                    compressed
                };
            };

            let mut dst = vec![0; SRC.len()];
            b.bytes = SRC.len() as u64;
            b.iter(|| {
                snap::decompress(COMPRESSED.as_slice(), &mut dst).unwrap()
            });
        }
    };
}

decompress!(uflat00, "html");
decompress!(uflat01, "urls.10K");
decompress!(uflat02, "fireworks.jpeg");
decompress!(uflat03, "fireworks.jpeg", 200);
decompress!(uflat04, "paper-100k.pdf");
decompress!(uflat05, "html_x_4");
decompress!(uflat06, "alice29.txt");
decompress!(uflat07, "asyoulik.txt");
decompress!(uflat08, "lcet10.txt");
decompress!(uflat09, "plrabn12.txt");
decompress!(uflat10, "geo.protodata");
decompress!(uflat11, "kppkn.gtb");
