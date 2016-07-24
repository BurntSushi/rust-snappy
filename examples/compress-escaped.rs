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
    let compressed = snap::Encoder::new().compress_vec(&bytes).unwrap();
    println!("{}", escape(&compressed));
}

fn escape(bytes: &[u8]) -> String {
    use std::ascii::escape_default;
    bytes.iter().flat_map(|&b| escape_default(b)).map(|b| b as char).collect()
}
