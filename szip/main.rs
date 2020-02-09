use std::error;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::result;

use filetime::{set_file_times, FileTime};

const ABOUT: &'static str = "
szip compresses and decompresses data in the Snappy format.

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
";

type Result<T> = result::Result<T, Error>;

type Error = Box<dyn error::Error + Send + Sync>;

macro_rules! fail {
    ($($tt:tt)*) => {
        return Err(From::from(format!($($tt)*)));
    }
}

macro_rules! errln {
    ($($tt:tt)*) => { let _ = writeln!(&mut std::io::stderr(), $($tt)*); }
}

fn main() {
    if let Err(err) = try_main() {
        errln!("{}", err);
        process::exit(1);
    }
}

fn try_main() -> Result<()> {
    let args = Args::parse()?;
    if args.paths.is_empty() {
        let stdin = io::stdin();
        let mut stdin = stdin.lock();
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        if args.decompress {
            args.decompress(&mut stdin, &mut stdout)?;
        } else {
            args.compress(&mut stdin, &mut stdout)?;
        }
    } else {
        for p in &args.paths {
            let r = if args.decompress {
                args.decompress_file(p)
            } else {
                args.compress_file(p)
            };
            if let Err(err) = r {
                errln!("{}: {}", p.display(), err);
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
struct Args {
    paths: Vec<PathBuf>,
    decompress: bool,
    force: bool,
    keep: bool,
    raw: bool,
}

impl Args {
    fn parse() -> Result<Args> {
        use clap::{crate_authors, crate_version, App, Arg};

        let parsed = App::new("szip")
            .about(ABOUT)
            .author(crate_authors!())
            .version(crate_version!())
            .max_term_width(100)
            .arg(
                Arg::with_name("paths")
                    .help("A list of file paths to compress (or decompress)."),
            )
            .arg(
                Arg::with_name("decompress")
                    .long("decompress")
                    .short("d")
                    .help("Decompress data (default is compression)."),
            )
            .arg(Arg::with_name("force").long("force").short("f").help(
                "Force (de)compression even if the corresponding \
                 output file already exists.",
            ))
            .arg(Arg::with_name("keep").long("keep").short("k").help(
                "Keep (don't delete) input files during (de)compression.",
            ))
            .arg(
                Arg::with_name("raw")
                    .long("raw")
                    .short("r")
                    .help("Use the \"raw\" Snappy format (no framing)."),
            )
            .get_matches();

        let paths = parsed
            .values_of_os("paths")
            .map(|paths| paths.into_iter().map(PathBuf::from).collect())
            .unwrap_or(vec![]);
        Ok(Args {
            paths,
            decompress: parsed.is_present("decompress"),
            force: parsed.is_present("force"),
            keep: parsed.is_present("keep"),
            raw: parsed.is_present("raw"),
        })
    }

    fn compress_file(&self, path: &Path) -> Result<()> {
        let old_path = path;
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
        if !self.force && fs::metadata(&new_path).is_ok() {
            fail!("skipping, file already exists: {}", new_path.display());
        }
        let old_file = io::BufReader::new(File::open(old_path)?);
        let new_file = io::BufWriter::new(File::create(&new_path)?);

        self.compress(old_file, new_file)?;
        copy_atime_mtime(old_path, new_path)?;
        if !self.keep {
            fs::remove_file(old_path)?;
        }
        Ok(())
    }

    fn decompress_file(&self, path: &Path) -> Result<()> {
        let old_path = path;
        let new_path = match old_path.file_name() {
            None => fail!("missing file name for {}", old_path.display()),
            Some(name) => {
                let name = name.to_string_lossy();
                if name.len() <= 3 || !name.ends_with(".sz") {
                    fail!("skipping uncompressed file");
                }
                old_path
                    .with_file_name(format!("{}", &name[0..name.len() - 3]))
            }
        };
        if !self.force && fs::metadata(&new_path).is_ok() {
            fail!("skipping, file already exists: {}", new_path.display());
        }
        let old_file = io::BufReader::new(File::open(old_path)?);
        let new_file = io::BufWriter::new(File::create(&new_path)?);

        self.decompress(old_file, new_file)?;
        copy_atime_mtime(old_path, new_path)?;
        if !self.keep {
            fs::remove_file(old_path)?;
        }
        Ok(())
    }

    fn compress<R: Read, W: Write>(
        &self,
        mut src: R,
        mut dst: W,
    ) -> Result<()> {
        if self.raw {
            // Read the entire src into memory and compress it.
            let mut buf = Vec::with_capacity(10 * (1 << 20));
            src.read_to_end(&mut buf)?;
            let compressed = snap::raw::Encoder::new().compress_vec(&buf)?;
            dst.write_all(&compressed)?;
        } else {
            let mut dst = snap::write::FrameEncoder::new(dst);
            io::copy(&mut src, &mut dst)?;
        }
        Ok(())
    }

    fn decompress<R: Read, W: Write>(
        &self,
        mut src: R,
        mut dst: W,
    ) -> Result<()> {
        if self.raw {
            // Read the entire src into memory and decompress it.
            let mut buf = Vec::with_capacity(10 * (1 << 20));
            src.read_to_end(&mut buf)?;
            let decompressed =
                snap::raw::Decoder::new().decompress_vec(&buf)?;
            dst.write_all(&decompressed)?;
        } else {
            let mut src = snap::read::FrameDecoder::new(src);
            io::copy(&mut src, &mut dst)?;
        }
        Ok(())
    }
}

fn copy_atime_mtime<P, Q>(src: P, dst: Q) -> Result<()>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let md = fs::metadata(src)?;
    let last_access = FileTime::from_last_access_time(&md);
    let last_mod = FileTime::from_last_modification_time(&md);
    set_file_times(dst, last_access, last_mod)?;
    Ok(())
}
