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
    --raw              Use the \"raw\" snappy format (no framing).
    -h, --help         Show this help message.
    -v, --version      Show version.
";

type Result<T> = result::Result<T, Error>;

type Error = Box<error::Error + Send + Sync>;

#[derive(Debug, RustcDecodable)]
struct Args {
    arg_file: Vec<String>,
    flag_decompress: bool,
    flag_raw: bool,
}

impl Args {
    fn run(&self) -> Result<()> {
        if !self.arg_file.is_empty() {
            unimplemented!()
        }
        let stdin = io::stdin();
        let mut stdin = stdin.lock();
        let stdout = io::stdout();
        let mut stdout = stdout.lock();

        if self.flag_raw {
            let mut src = Vec::with_capacity(1 << 16);
            try!(stdin.read_to_end(&mut src));
            let mut dst = vec![0; snap::max_compress_len(src.len())];
            let n = try!(snap::Encoder::new().compress(&src, &mut dst));
            try!(stdout.write_all(&dst[..n]));
        } else {
            if self.flag_decompress {
                let mut rdr = snap::Reader::new(stdin);
                try!(io::copy(&mut rdr, &mut stdout));
            } else {
                let mut wtr = snap::Writer::new(stdout);
                try!(io::copy(&mut stdin, &mut wtr));
            }
        }
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
