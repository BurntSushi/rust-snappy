szip
====
A pure Rust command line tool for compressing and decompressing Snappy files.
By default, this tool uses the Snappy frame format.

[![Build status](https://github.com/BurntSushi/rust-snappy/workflows/ci/badge.svg)](https://github.com/BurntSushi/rust-snappy/actions)
[![](http://meritbadge.herokuapp.com/szip)](https://crates.io/crates/szip)

Licensed under the BSD 3-Clause.


### Documentation

See `szip --help`.


### Installation

`szip` is on crates.io:

```
$ cargo install szip
```


### Usage

szip works similarly to gzip.

To compress a file:

```
$ szip some-file
```

`some-file.sz` will be written as the Snappy compressed form of `some-file`,
and `some-file` will be deleted. To keep the original file, use the `-k/--keep`
flag:

```
$ szip -k some-file
```

To decompress a file, use the `-d/--decompress` flag:

```
$ szip -d some-file.sz
```

Like compression, `some-file` will be written with the uncompressed data
and `some-file.sz` will be removed. Use the `-k/--keep` flag to retain
`some-file.sz`.

szip can only compress or decompress streams:

```
$ szip < some-file | szip -d > same-file
```

Finally, the Snappy frame format can be disabled in lieu of the Snappy raw
format with the `-r/--raw` flag. Generally, using the raw format is not
recommended unless you know you need it.

```
$ szip -r some-file
```
