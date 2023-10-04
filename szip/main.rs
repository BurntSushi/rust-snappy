use std::fs::{self, File};
use std::io::{self, Read, stdout, Write};
use std::path::{Path, PathBuf};

use anyhow::bail;
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

fn app() -> clap::App<'static, 'static> {
    use clap::{crate_authors, crate_version, App, Arg};

    App::new("szip")
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
        .arg(
            Arg::with_name("keep").long("keep").short("k").help(
                "Keep (don't delete) input files during (de)compression.",
            ),
        )
        .arg(
            Arg::with_name("stdout").long("stdout").short("s").help(
                "Write output to stdout without modifying existing files."
            )
        )
        .arg(
            Arg::with_name("raw")
                .long("raw")
                .short("r")
                .help("Use the \"raw\" Snappy format (no framing)."),
        )
}

fn main() -> anyhow::Result<()> {
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
            if let Err(err) = args.do_file(p, args.stdout) {
                writeln!(
                    &mut std::io::stderr(),
                    "{}: {:?}",
                    p.display(),
                    err
                )?;
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
    stdout: bool,
}

impl Args {
    fn parse() -> anyhow::Result<Args> {
        let parsed = app().get_matches();
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
            stdout: parsed.is_present("stdout"),
        })
    }

    fn do_file(&self, old_path: &Path, to_stdout: bool) -> anyhow::Result<()> {
        let old_md = old_path.metadata()?;
        if old_md.is_dir() {
            bail!("is a directory");
        }
        let new_path = self.new_path(old_path)?;

        let dst = io::BufWriter::new(if to_stdout {
            Box::new(stdout().lock()) as Box<dyn Write>
        } else {
            if !self.force && new_path.exists() {
                bail!("skipping, file already exists: {}", new_path.display());
            }

            Box::new(File::create(&new_path)?) as Box<dyn Write>
        });

        let old_file = io::BufReader::new(File::open(old_path)?);
        if self.decompress {
            self.decompress(old_file, dst)?;
        } else {
            self.compress(old_file, dst)?;
        }

        if !to_stdout {
            let last_access = FileTime::from_last_access_time(&old_md);
            let last_mod = FileTime::from_last_modification_time(&old_md);
            set_file_times(new_path, last_access, last_mod)?;
            if !self.keep {
                fs::remove_file(old_path)?;
            }
        }
        Ok(())
    }

    fn new_path(&self, old_path: &Path) -> anyhow::Result<PathBuf> {
        let name = match old_path.file_name() {
            None => bail!("missing file name"),
            Some(name) => name,
        };
        if self.decompress {
            let name = name.to_string_lossy();
            if name.len() <= 3 || !name.ends_with(".sz") {
                bail!("skipping uncompressed file");
            }
            Ok(old_path
                .with_file_name(format!("{}", &name[0..name.len() - 3])))
        } else {
            let name = name.to_string_lossy();
            if name.ends_with(".sz") {
                bail!("skipping compressed file");
            }
            Ok(old_path.with_file_name(format!("{}.sz", name)))
        }
    }

    fn compress<R: Read, W: Write>(
        &self,
        mut src: R,
        mut dst: W,
    ) -> anyhow::Result<()> {
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
    ) -> anyhow::Result<()> {
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
