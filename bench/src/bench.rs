use std::time::Duration;

use criterion::{
    criterion_group, criterion_main, Bencher, Benchmark, Criterion, Throughput,
};

const CORPUS_HTML: &'static [u8] = include_bytes!("../../data/html");
const CORPUS_URLS_10K: &'static [u8] = include_bytes!("../../data/urls.10K");
const CORPUS_FIREWORKS: &'static [u8] =
    include_bytes!("../../data/fireworks.jpeg");
const CORPUS_PAPER_100K: &'static [u8] =
    include_bytes!("../../data/paper-100k.pdf");
const CORPUS_HTML_X_4: &'static [u8] = include_bytes!("../../data/html_x_4");
const CORPUS_ALICE29: &'static [u8] = include_bytes!("../../data/alice29.txt");
const CORPUS_ASYOULIK: &'static [u8] =
    include_bytes!("../../data/asyoulik.txt");
const CORPUS_LCET10: &'static [u8] = include_bytes!("../../data/lcet10.txt");
const CORPUS_PLRABN12: &'static [u8] =
    include_bytes!("../../data/plrabn12.txt");
const CORPUS_GEOPROTO: &'static [u8] =
    include_bytes!("../../data/geo.protodata");
const CORPUS_KPPKN: &'static [u8] = include_bytes!("../../data/kppkn.gtb");

macro_rules! compress {
    ($c:expr, $comp:expr, $group:expr, $name:expr, $corpus:expr) => {
        compress!($c, $comp, $group, $name, $corpus, 0);
    };
    ($c:expr, $comp:expr, $group:expr, $name:expr, $corpus:expr, $size:expr) => {
        let mut corpus = $corpus;
        if $size > 0 {
            corpus = &corpus[..$size];
        }
        let mut dst = vec![0; snap::raw::max_compress_len(corpus.len())];
        define($c, $group, &format!("compress/{}", $name), corpus, move |b| {
            b.iter(|| {
                $comp(corpus, &mut dst).unwrap();
            });
        });
    };
}

macro_rules! decompress {
    ($c:expr, $decomp:expr, $group:expr, $name:expr, $corpus:expr) => {
        decompress!($c, $decomp, $group, $name, $corpus, 0);
    };
    ($c:expr, $decomp:expr, $group:expr, $name:expr, $corpus:expr, $size:expr) => {
        let mut corpus = $corpus;
        if $size > 0 {
            corpus = &corpus[..$size];
        }
        let compressed =
            snap::raw::Encoder::new().compress_vec(corpus).unwrap();
        let mut dst = vec![0; corpus.len()];
        define(
            $c,
            $group,
            &format!("decompress/{}", $name),
            corpus,
            move |b| {
                b.iter(|| {
                    $decomp(&compressed, &mut dst).unwrap();
                });
            },
        );
    };
}

fn all(c: &mut Criterion) {
    rust(c);
    #[cfg(feature = "cpp")]
    cpp(c);
}

fn rust(c: &mut Criterion) {
    fn compress(input: &[u8], output: &mut [u8]) -> snap::Result<usize> {
        snap::raw::Encoder::new().compress(input, output)
    }

    fn decompress(input: &[u8], output: &mut [u8]) -> snap::Result<usize> {
        snap::raw::Decoder::new().decompress(input, output)
    }

    compress!(c, compress, "snap", "zflat00_html", CORPUS_HTML);
    compress!(c, compress, "snap", "zflat01_urls", CORPUS_URLS_10K);
    compress!(c, compress, "snap", "zflat02_jpg", CORPUS_FIREWORKS);
    compress!(c, compress, "snap", "zflat03_jpg_200", CORPUS_FIREWORKS, 200);
    compress!(c, compress, "snap", "zflat04_pdf", CORPUS_PAPER_100K);
    compress!(c, compress, "snap", "zflat05_html4", CORPUS_HTML_X_4);
    compress!(c, compress, "snap", "zflat06_txt1", CORPUS_ALICE29);
    compress!(c, compress, "snap", "zflat07_txt2", CORPUS_ASYOULIK);
    compress!(c, compress, "snap", "zflat08_txt3", CORPUS_LCET10);
    compress!(c, compress, "snap", "zflat09_txt4", CORPUS_PLRABN12);
    compress!(c, compress, "snap", "zflat10_pb", CORPUS_GEOPROTO);
    compress!(c, compress, "snap", "zflat11_gaviota", CORPUS_KPPKN);

    decompress!(c, decompress, "snap", "uflat00_html", CORPUS_HTML);
    decompress!(c, decompress, "snap", "uflat01_urls", CORPUS_URLS_10K);
    decompress!(c, decompress, "snap", "uflat02_jpg", CORPUS_FIREWORKS);
    decompress!(
        c,
        decompress,
        "snap",
        "uflat03_jpg_200",
        CORPUS_FIREWORKS,
        200
    );
    decompress!(c, decompress, "snap", "uflat04_pdf", CORPUS_PAPER_100K);
    decompress!(c, decompress, "snap", "uflat05_html4", CORPUS_HTML_X_4);
    decompress!(c, decompress, "snap", "uflat06_txt1", CORPUS_ALICE29);
    decompress!(c, decompress, "snap", "uflat07_txt2", CORPUS_ASYOULIK);
    decompress!(c, decompress, "snap", "uflat08_txt3", CORPUS_LCET10);
    decompress!(c, decompress, "snap", "uflat09_txt4", CORPUS_PLRABN12);
    decompress!(c, decompress, "snap", "uflat10_pb", CORPUS_GEOPROTO);
    decompress!(c, decompress, "snap", "uflat11_gaviota", CORPUS_KPPKN);
}

#[cfg(feature = "cpp")]
fn cpp(c: &mut Criterion) {
    use snappy_cpp::{compress, decompress};

    compress!(c, compress, "cpp", "zflat00_html", CORPUS_HTML);
    compress!(c, compress, "cpp", "zflat01_urls", CORPUS_URLS_10K);
    compress!(c, compress, "cpp", "zflat02_jpg", CORPUS_FIREWORKS);
    compress!(c, compress, "cpp", "zflat03_jpg_200", CORPUS_FIREWORKS, 200);
    compress!(c, compress, "cpp", "zflat04_pdf", CORPUS_PAPER_100K);
    compress!(c, compress, "cpp", "zflat05_html4", CORPUS_HTML_X_4);
    compress!(c, compress, "cpp", "zflat06_txt1", CORPUS_ALICE29);
    compress!(c, compress, "cpp", "zflat07_txt2", CORPUS_ASYOULIK);
    compress!(c, compress, "cpp", "zflat08_txt3", CORPUS_LCET10);
    compress!(c, compress, "cpp", "zflat09_txt4", CORPUS_PLRABN12);
    compress!(c, compress, "cpp", "zflat10_pb", CORPUS_GEOPROTO);
    compress!(c, compress, "cpp", "zflat11_gaviota", CORPUS_KPPKN);

    decompress!(c, decompress, "cpp", "uflat00_html", CORPUS_HTML);
    decompress!(c, decompress, "cpp", "uflat01_urls", CORPUS_URLS_10K);
    decompress!(c, decompress, "cpp", "uflat02_jpg", CORPUS_FIREWORKS);
    decompress!(
        c,
        decompress,
        "cpp",
        "uflat03_jpg_200",
        CORPUS_FIREWORKS,
        200
    );
    decompress!(c, decompress, "cpp", "uflat04_pdf", CORPUS_PAPER_100K);
    decompress!(c, decompress, "cpp", "uflat05_html4", CORPUS_HTML_X_4);
    decompress!(c, decompress, "cpp", "uflat06_txt1", CORPUS_ALICE29);
    decompress!(c, decompress, "cpp", "uflat07_txt2", CORPUS_ASYOULIK);
    decompress!(c, decompress, "cpp", "uflat08_txt3", CORPUS_LCET10);
    decompress!(c, decompress, "cpp", "uflat09_txt4", CORPUS_PLRABN12);
    decompress!(c, decompress, "cpp", "uflat10_pb", CORPUS_GEOPROTO);
    decompress!(c, decompress, "cpp", "uflat11_gaviota", CORPUS_KPPKN);
}

fn define(
    c: &mut Criterion,
    group_name: &str,
    bench_name: &str,
    corpus: &[u8],
    bench: impl FnMut(&mut Bencher) + 'static,
) {
    let tput = Throughput::Bytes(corpus.len() as u64);
    let benchmark = Benchmark::new(bench_name, bench)
        .throughput(tput)
        .sample_size(50)
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(3));
    c.bench(group_name, benchmark);
}

criterion_group!(g, all);
criterion_main!(g);
