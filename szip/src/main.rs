extern crate docopt;
extern crate filetime;
extern crate rustc_serialize;
extern crate snap;

use std::error;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process;
use std::result;

use docopt::Docopt;
use filetime::{FileTime, set_file_times};

const USAGE: &'static str = "
szip works similarly to gzip. It takes files as parameters, compresses them to
a new file with a .sz extension, and removes the original. File access and
modification times are preserved.

Alternatively, data can be sent on stdin and its compressed form will be sent
to stdout.

The -d (short for --decompress) flag changes the mode from compression to
decompression.

The --raw flag can be used for compressing/decompressing the raw Snappy format.
Note that this requires reading the entire input/output into memory. In
general, you shouldn't use this flag unless you have a specific need to.

Usage:
    snappy [options] [<file> ...]
    snappy --help
    snappy --version

Options:
    -d, --decompress   Decompress files (default is compression).
    -f, --force        Force (de)compression even if the corresponding output
                       file already exists.
    -h, --help         Show this help message.
    -k, --keep         Keep (don't delete) input files during (de)compression.
    -r, --raw          Use the \"raw\" snappy format (no framing).
    --version      Show version.
";

type Result<T> = result::Result<T, Error>;

type Error = Box<error::Error + Send + Sync>;

macro_rules! fail {
    ($($tt:tt)*) => {
        return Err(From::from(format!($($tt)*)));
    }
}

macro_rules! errln {
    ($($tt:tt)*) => { let _ = writeln!(&mut ::std::io::stderr(), $($tt)*); }
}

#[derive(Debug, RustcDecodable)]
struct Args {
    arg_file: Vec<String>,
    flag_decompress: bool,
    flag_force: bool,
    flag_keep: bool,
    flag_raw: bool,
}

fn main() {
    let args: Args =
        Docopt::new(USAGE)
        .and_then(|d| d.version(Some(version())).decode())
        .unwrap_or_else(|e| e.exit());
    if let Err(err) = args.run() {
        errln!("{}", err);
        process::exit(1);
    }
}

impl Args {
    fn run(&self) -> Result<()> {
        if self.arg_file.is_empty() {
            let stdin = io::stdin();
            let mut stdin = stdin.lock();
            let stdout = io::stdout();
            let mut stdout = stdout.lock();
            if self.flag_decompress {
                try!(self.decompress(&mut stdin, &mut stdout));
            } else {
                try!(self.compress(&mut stdin, &mut stdout));
            }
        } else {
            for f in &self.arg_file {
                let r =
                    if self.flag_decompress {
                        self.decompress_file(f)
                    } else {
                        self.compress_file(f)
                    };
                if let Err(err) = r {
                    errln!("{}: {}", f, err);
                }
            }
        }
        Ok(())
    }

    fn compress_file<P: AsRef<Path>>(&self, file_path: P) -> Result<()> {
        let old_path = file_path.as_ref();
        let new_path = match old_path.file_name() {
            None => fail!("missing file name for {}", old_path.display()),
            Some(name) => {
                let name = name.to_string_lossy();
                if name.ends_with(".sz") {
                    fail!("skipping compressed file");
                }
                old_path.with_file_name(format!("{}.sz", name))
            }
        };
        if !self.flag_force && fs::metadata(&new_path).is_ok() {
            fail!("skipping, file already exists: {}", new_path.display());
        }
        let old_file = io::BufReader::new(try!(File::open(old_path)));
        let new_file = io::BufWriter::new(try!(File::create(&new_path)));

        try!(self.compress(old_file, new_file));
        try!(copy_atime_mtime(old_path, new_path));
        if !self.flag_keep {
            try!(fs::remove_file(old_path));
        }
        Ok(())
    }

    fn decompress_file<P: AsRef<Path>>(&self, file_path: P) -> Result<()> {
        let old_path = file_path.as_ref();
        let new_path = match old_path.file_name() {
            None => fail!("missing file name for {}", old_path.display()),
            Some(name) => {
                let name = name.to_string_lossy();
                if name.len() <= 3 || !name.ends_with(".sz") {
                    fail!("skipping uncompressed file");
                }
                old_path.with_file_name(format!("{}", &name[0..name.len()-3]))
            }
        };
        if !self.flag_force && fs::metadata(&new_path).is_ok() {
            fail!("skipping, file already exists: {}", new_path.display());
        }
        let old_file = io::BufReader::new(try!(File::open(old_path)));
        let new_file = io::BufWriter::new(try!(File::create(&new_path)));

        try!(self.decompress(old_file, new_file));
        try!(copy_atime_mtime(old_path, new_path));
        if !self.flag_keep {
            try!(fs::remove_file(old_path));
        }
        Ok(())
    }

    fn compress<R: Read, W: Write>(
        &self,
        mut src: R,
        mut dst: W,
    ) -> Result<()> {
        if self.flag_raw {
            // Read the entire src into memory and compress it.
            let mut buf = Vec::with_capacity(10 * (1 << 20));
            try!(src.read_to_end(&mut buf));
            let compressed = try!(snap::Encoder::new().compress_vec(&buf));
            try!(dst.write_all(&compressed));
        } else {
            let mut dst = snap::Writer::new(dst);
            try!(io::copy(&mut src, &mut dst));
        }
        Ok(())
    }

    fn decompress<R: Read, W: Write>(
        &self,
        mut src: R,
        mut dst: W,
    ) -> Result<()> {
        if self.flag_raw {
            // Read the entire src into memory and decompress it.
            let mut buf = Vec::with_capacity(10 * (1 << 20));
            try!(src.read_to_end(&mut buf));
            let decompressed = try!(snap::Decoder::new().decompress_vec(&buf));
            try!(dst.write_all(&decompressed));
        } else {
            let mut src = snap::Reader::new(src);
            try!(io::copy(&mut src, &mut dst));
        }
        Ok(())
    }
}

fn copy_atime_mtime<P, Q>(
    src: P,
    dst: Q,
) -> Result<()> where P: AsRef<Path>, Q: AsRef<Path> {
    let md = try!(fs::metadata(src));
    let last_access = FileTime::from_last_access_time(&md);
    let last_mod = FileTime::from_last_modification_time(&md);
    try!(set_file_times(dst, last_access, last_mod));
    Ok(())
}

fn version() -> String {
    let (maj, min, pat) = (
        option_env!("CARGO_PKG_VERSION_MAJOR"),
        option_env!("CARGO_PKG_VERSION_MINOR"),
        option_env!("CARGO_PKG_VERSION_PATCH"),
    );
    match (maj, min, pat) {
        (Some(maj), Some(min), Some(pat)) =>
            format!("{}.{}.{}", maj, min, pat),
        _ => "".to_owned(),
    }
}
