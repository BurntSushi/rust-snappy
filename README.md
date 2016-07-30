snap
====
A pure Rust implementation of the
[Snappy compression algorithm](http://google.github.io/snappy/).
Includes streaming compression and decompression using the Snappy frame format.
This implementation is ported from both the
[reference C++ implementation](https://github.com/google/snappy)
and the
[Go implementation](https://github.com/golang/snappy).

[![Linux build status](https://api.travis-ci.org/BurntSushi/snap.png)](https://travis-ci.org/BurntSushi/snap)
[![Windows build status](https://ci.appveyor.com/api/projects/status/github/BurntSushi/snap?svg=true)](https://ci.appveyor.com/project/BurntSushi/snap)
[![](http://meritbadge.herokuapp.com/snap)](https://crates.io/crates/snap)

Dual-licensed under MIT or the [UNLICENSE](http://unlicense.org).

### Documentation

[http://burntsushi.net/rustdoc/snap/](http://burntsushi.net/rustdoc/snap/)

### Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
snap = "0.1"
```

and this to your crate root:

```rust
extern crate snap;
```

### Example: compress data on `stdin`

This program reads data from `stdin`, compresses it and emits it to `stdout`.
This example can be found in `examples/compress.rs`:

```rust
extern crate snap;

use std::io;

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();

    let mut rdr = stdin.lock();
    // Wrap the stdout writer in a Snappy writer.
    let mut wtr = snap::Writer::new(stdout.lock());
    io::copy(&mut rdr, &mut wtr).expect("I/O operation failed");
}
```

### Example: decompress data on `stdin`

This program reads data from `stdin`, decompresses it and emits it to `stdout`.
This example can be found in `examples/decompress.rs`:

```rust
extern crate snap;

use std::io;

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();

    // Wrap the stdin reader in a Snappy reader.
    let mut rdr = snap::Reader::new(stdin.lock());
    let mut wtr = stdout.lock();
    io::copy(&mut rdr, &mut wtr).expect("I/O operation failed");
}
```

### Testing

This crate is tested against the reference C++ implementation of Snappy.
Currently, compression is byte-for-byte equivalent with the C++ implementation.
This seems like a reasonable starting point, although it is not necessarily
a goal to always maintain byte-for-byte equivalence.

Tests against the reference C++ implementation can be run with
`cargo test --features cpp`. Note that you will need to have the C++ Snappy
library in your `LD_LIBRARY_PATH` (or equivalent).

### Performance

The performance of this implementation should roughly match the performance
of the C++ implementation. Below are the results of the microbenchmarks (as
defined in the C++ library):

```
name             cpp ns/iter           rust ns/iter          diff ns/iter   diff %
uflat00_html     49,130 (2,084 MB/s)   48,708 (2,102 MB/s)           -422   -0.86%
uflat01_urls     519,112 (1,352 MB/s)  500,274 (1,403 MB/s)       -18,838   -3.63%
uflat02_jpg      5,219 (23,585 MB/s)   5,734 (21,467 MB/s)            515    9.87%
uflat03_jpg_200  147 (1,360 MB/s)      136 (1,470 MB/s)               -11   -7.48%
uflat04_pdf      7,987 (12,820 MB/s)   7,138 (14,345 MB/s)           -849  -10.63%
uflat05_html4    207,788 (1,971 MB/s)  201,960 (2,028 MB/s)        -5,828   -2.80%
uflat06_txt1     161,859 (939 MB/s)    161,453 (942 MB/s)            -406   -0.25%
uflat07_txt2     150,726 (830 MB/s)    143,303 (873 MB/s)          -7,423   -4.92%
uflat08_txt3     436,503 (977 MB/s)    426,687 (1,000 MB/s)        -9,816   -2.25%
uflat09_txt4     635,595 (758 MB/s)    607,344 (793 MB/s)         -28,251   -4.44%
uflat10_pb       48,102 (2,465 MB/s)   43,647 (2,716 MB/s)         -4,455   -9.26%
uflat11_gaviota  168,227 (1,095 MB/s)  172,169 (1,070 MB/s)         3,942    2.34%
zflat00_html     115,532 (886 MB/s)    110,474 (926 MB/s)          -5,058   -4.38%
zflat01_urls     1,518,622 (462 MB/s)  1,469,408 (477 MB/s)       -49,214   -3.24%
zflat02_jpg      9,481 (12,983 MB/s)   9,263 (13,288 MB/s)           -218   -2.30%
zflat03_jpg_200  366 (546 MB/s)        296 (675 MB/s)                 -70  -19.13%
zflat04_pdf      13,212 (7,750 MB/s)   12,503 (8,190 MB/s)           -709   -5.37%
zflat05_html4    475,279 (861 MB/s)    467,080 (876 MB/s)          -8,199   -1.73%
zflat06_txt1     490,221 (310 MB/s)    456,371 (333 MB/s)         -33,850   -6.91%
zflat07_txt2     436,043 (287 MB/s)    493,155 (253 MB/s)          57,112   13.10%
zflat08_txt3     1,243,148 (343 MB/s)  1,216,920 (350 MB/s)       -26,228   -2.11%
zflat09_txt4     1,699,077 (283 MB/s)  1,670,186 (288 MB/s)       -28,891   -1.70%
zflat10_pb       103,676 (1,143 MB/s)  100,874 (1,175 MB/s)        -2,802   -2.70%
zflat11_gaviota  363,727 (506 MB/s)    333,110 (553 MB/s)         -30,617   -8.42%
```

Notes: These benchmarks were run with Snappy/C++ on commit `32d6d7`. Both the
C++ and Rust benchmarks were run with the same benchmark harness. Benchmarks
were run on an Intel i7-3520M.
