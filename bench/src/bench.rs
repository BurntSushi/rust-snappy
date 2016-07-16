#![feature(test)]

#[macro_use]
extern crate lazy_static;
extern crate libc;
extern crate snap;
extern crate test;

mod cpp;

macro_rules! compress {
    ($name:ident, $filename:expr) => {
        compress!($name, $filename, 0);
    };
    ($name:ident, $filename:expr, $size:expr) => {
        #[bench]
        fn $name(b: &mut test::Bencher) {
            lazy_static! {
                static ref SRC: Vec<u8> = {
                    let src = include_bytes!(concat!("../../data/", $filename));
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

compress!(zflat00_html, "html");
compress!(zflat01_urls, "urls.10K");
compress!(zflat02_jpg, "fireworks.jpeg");
compress!(zflat03_jpg_200, "fireworks.jpeg", 200);
compress!(zflat04_pdf, "paper-100k.pdf");
compress!(zflat05_html4, "html_x_4");
compress!(zflat06_txt1, "alice29.txt");
compress!(zflat07_txt2, "asyoulik.txt");
compress!(zflat08_txt3, "lcet10.txt");
compress!(zflat09_txt4, "plrabn12.txt");
compress!(zflat10_pb, "geo.protodata");
compress!(zflat11_gaviota, "kppkn.gtb");

macro_rules! decompress {
    ($dec:expr, $name:ident, $filename:expr) => {
        decompress!($dec, $name, $filename, 0);
    };
    ($dec:expr, $name:ident, $filename:expr, $size:expr) => {
        #[bench]
        fn $name(b: &mut test::Bencher) {
            lazy_static! {
                static ref SRC: Vec<u8> = {
                    let src = include_bytes!(concat!("../../data/", $filename));
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
                $dec(COMPRESSED.as_slice(), &mut dst).unwrap()
            });
        }
    };
}

decompress!(snap::decompress, rust_uflat00_html, "html");
decompress!(snap::decompress, rust_uflat01_urls, "urls.10K");
decompress!(snap::decompress, rust_uflat02_jpg, "fireworks.jpeg");
decompress!(snap::decompress, rust_uflat03_jpg_200, "fireworks.jpeg", 200);
decompress!(snap::decompress, rust_uflat04_pdf, "paper-100k.pdf");
decompress!(snap::decompress, rust_uflat05_html4, "html_x_4");
decompress!(snap::decompress, rust_uflat06_txt1, "alice29.txt");
decompress!(snap::decompress, rust_uflat07_txt2, "asyoulik.txt");
decompress!(snap::decompress, rust_uflat08_txt3, "lcet10.txt");
decompress!(snap::decompress, rust_uflat09_txt4, "plrabn12.txt");
decompress!(snap::decompress, rust_uflat10_pb, "geo.protodata");
decompress!(snap::decompress, rust_uflat11_gaviota, "kppkn.gtb");

decompress!(cpp::decompress, cpp_uflat00_html, "html");
decompress!(cpp::decompress, cpp_uflat01_urls, "urls.10K");
decompress!(cpp::decompress, cpp_uflat02_jpg, "fireworks.jpeg");
decompress!(cpp::decompress, cpp_uflat03_jpg_200, "fireworks.jpeg", 200);
decompress!(cpp::decompress, cpp_uflat04_pdf, "paper-100k.pdf");
decompress!(cpp::decompress, cpp_uflat05_html4, "html_x_4");
decompress!(cpp::decompress, cpp_uflat06_txt1, "alice29.txt");
decompress!(cpp::decompress, cpp_uflat07_txt2, "asyoulik.txt");
decompress!(cpp::decompress, cpp_uflat08_txt3, "lcet10.txt");
decompress!(cpp::decompress, cpp_uflat09_txt4, "plrabn12.txt");
decompress!(cpp::decompress, cpp_uflat10_pb, "geo.protodata");
decompress!(cpp::decompress, cpp_uflat11_gaviota, "kppkn.gtb");
