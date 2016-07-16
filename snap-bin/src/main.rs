extern crate docopt;
extern crate rustc_serialize;
extern crate snap;

use std::error;
use std::io::{self, Read, Write};
use std::process;
use std::result;

use docopt::Docopt;

const USAGE: &'static str = "
Usage:
    snappy [options] [file ...]
    snappy --help
    snappy --version

Options:
    -d, --decompress   Decompress files (default is compression).
    -h, --help     Show this help message.
    -v, --version  Show version.
";

type Result<T> = result::Result<T, Error>;

type Error = Box<error::Error + Send + Sync>;

#[derive(Debug, RustcDecodable)]
struct Args {
    arg_file: Vec<String>,
    flag_decompress: bool,
}

impl Args {
    fn run(&self) -> Result<()> {
        if !self.arg_file.is_empty() {
            unimplemented!()
        }
        if self.flag_decompress {
            unimplemented!()
        }
        let stdin = io::stdin();
        let mut stdin = stdin.lock();

        let mut input = Vec::with_capacity(1 << 16);
        try!(stdin.read_to_end(&mut input));

        let buf_size = snap::max_compressed_len(input.len());
        let mut output = vec![0; buf_size];
        let n = try!(snap::compress(&input, &mut output));

        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        try!(stdout.write_all(&output[..n]));
        Ok(())
    }
}

fn main() {
    let args: Args = Docopt::new(USAGE).and_then(|d| d.decode())
                                       .unwrap_or_else(|e| e.exit());
    if let Err(err) = args.run() {
        writeln!(&mut io::stderr(), "{}", err).unwrap();
        process::exit(1);
    }
}
