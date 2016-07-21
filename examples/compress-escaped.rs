/// compress-escaped is a utility program that accepts a single command line
/// parameter, compresses it and prints it to stdout after escaping it.

extern crate snap;

use std::env;
use std::io::{self, Write};
use std::process;

fn main() {
    let bytes = match env::args().nth(1) {
        None => {
            writeln!(
                &mut io::stderr(), "Usage: compress-escaped string").unwrap();
            process::exit(1);
        }
        Some(arg) => arg.into_bytes(),
    };
    let mut compressed = vec![0; snap::max_compressed_len(bytes.len())];
    let n = snap::compress(&bytes, &mut compressed).unwrap();
    println!("{}", escape(&compressed[0..n]));
}

fn escape(bytes: &[u8]) -> String {
    use std::ascii::escape_default;
    bytes.iter().flat_map(|&b| escape_default(b)).map(|b| b as char).collect()
}
