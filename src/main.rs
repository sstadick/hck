use anyhow::{Context, Error, Result};
use env_logger::Env;
use flate2::Compression;
use grep_cli::{stdout, unescape};
use gzp::{deflate::Bgzf, ZBuilder};
use hcklib::{
    core::{Core, CoreConfig, CoreConfigBuilder, HckInput},
    field_range::RegexOrStr,
    line_parser::{RegexLineParser, SubStrLineParser},
    mmap::MmapChoice,
};
use lazy_static::lazy_static;
use log::{error, warn};
use regex::bytes::Regex;
use ripline::{
    line_buffer::{LineBuffer, LineBufferBuilder},
    LineTerminator,
};
use std::{
    fs::File,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    process::exit,
};
use structopt::{clap::AppSettings::ColoredHelp, StructOpt};
use termcolor::ColorChoice;

lazy_static! {
    /// Default number of compression threads to use.
    ///
    /// This will be 4 if >= 4 threads are present, otherwise it will
    /// be num_cpus - 1.
    pub static ref DEFAULT_CPUS: String = {
        let num_cores = num_cpus::get();
        if num_cores < 4 {
            num_cores.saturating_sub(1)
        } else {
            4
        }.to_string()
    };
}

pub mod built_info {
    use structopt::lazy_static::lazy_static;

    include!(concat!(env!("OUT_DIR"), "/built.rs"));

    /// Get a software version string including
    ///   - Git commit hash
    ///   - Git dirty info (whether the repo had uncommitted changes)
    ///   - Cargo package version if no git info found
    fn get_software_version() -> String {
        let prefix = if let Some(s) = GIT_COMMIT_HASH {
            format!("{}-{}", PKG_VERSION, s[0..8].to_owned())
        } else {
            // This shouldn't happen
            PKG_VERSION.to_string()
        };
        let suffix = match GIT_DIRTY {
            Some(true) => "-dirty",
            _ => "",
        };
        format!("{}{}", prefix, suffix)
    }

    lazy_static! {
        /// Version of the software with git hash
        pub static ref VERSION: String = get_software_version();
    }
}

/// Determine if we should write to a file or stdout.
fn select_output<P: AsRef<Path>>(output: Option<P>) -> Result<Box<dyn Write + Send + 'static>> {
    let writer: Box<dyn Write + Send + 'static> = match output {
        Some(path) => {
            if path.as_ref().as_os_str() == "-" {
                // TODO: verify that stdout buffers when writing to a terminal now (this was a bug in Rust at some point).
                Box::new(stdout(ColorChoice::Never))
            } else {
                Box::new(File::create(&path).with_context(|| {
                    format!("Failed to open {} for writing.", path.as_ref().display())
                })?)
            }
        }
        None => Box::new(stdout(ColorChoice::Never)),
    };
    Ok(writer)
}

/// Check if err is a broken pipe.
#[inline]
fn is_broken_pipe(err: &Error) -> bool {
    if let Some(io_err) = err.downcast_ref::<io::Error>() {
        if io_err.kind() == io::ErrorKind::BrokenPipe {
            return true;
        }
    }
    false
}
/// * `delimiter` is a regex by default and a fixed substring with `-L`
/// * `header-fields` allows for specifying a literal or a regex to match header names to select columns
/// * both `header-fields` and `fields` order dictate the order of the output columns
/// * input files (not stdin) are automatically compressed
/// * the output delimiter can specified with `-D`
///
/// ## Selection by headers
///
/// Instead of specifying fields to output by index ranges (i.e `1-2,4-`), you can specify a regex or string literal
/// to select a headered column to output with the `-F` option. By default `-F` options are treated as string literals.
/// To treat them as regexs add the `-r` flag.
///
/// ## Ordering of outputs
///
/// *Values are written only once*. So for a `fields` value of `4-,1,5-8`, which translates to "print columns 4 through
/// the end and then the last column and then columns 5 through 8", columns 5-8 won't be printed again because they
/// were already consumed by the `4-` range.
///
/// If `field-headers` is used as a regex then the headers will be be grouped together in groups that all matched the
/// same regex, and in the order of the regex as specified on the CLI.
#[derive(Debug, StructOpt)]
#[structopt(
    name = "hck",
    author,
    global_setting(ColoredHelp),
    version = built_info::VERSION.as_str()
)]
struct Opts {
    /// Input files to parse, defaults to stdin.
    ///
    /// If a file has a recognizable file extension indicating that it is compressed, and a local binary
    /// to perform decompression is found, decompression will occur automagically. This requires with `-z`.
    input: Vec<PathBuf>,

    /// Output file to write to, defaults to stdout
    #[structopt(short = "o", long)]
    output: Option<PathBuf>,

    /// Delimiter to use on input files, this is a substring literal by default. To treat it as a literal add the `-L` flag.
    #[structopt(short = "d", long, default_value = r"\s+")]
    delimiter: String,

    /// Treat the delimiter as a string literal. This can significantly improve performance, especially for single byte delimiters.
    #[structopt(short = "L", long)]
    delim_is_literal: bool,

    /// Use the input delimiter as the output delimiter if the input is literal and no other output delimiter has been set.
    #[structopt(
        short = "I",
        long,
        requires("delim-is-literal"),
        conflicts_with("output-delimiter")
    )]
    use_input_delim: bool,

    /// Delimiter string to use on outputs
    #[structopt(short = "D", long, default_value = "\t")]
    output_delimiter: String,

    /// Fields to keep in the output, ex: 1,2-,-5,2-5. Fields are 1-based and inclusive.
    #[structopt(short = "f", long)]
    fields: Option<String>,

    /// Fields to exclude from the output, ex: 3,9-11,15-. Exclude fields are 1 based and inclusive.
    /// Exclude fields take precedence over `fields`.
    #[structopt(short = "e", long)]
    exclude: Option<String>,

    /// Headers to exclude from the output, ex: '^badfield.*$`. This is a string literal by default.
    /// Add the `-r` flag to treat as a regex.
    #[structopt(short = "E", long, multiple = true, number_of_values = 1)]
    exclude_header: Option<Vec<Regex>>,

    /// A string literal or regex to select headers, ex: '^is_.*$`. This is a string literal
    /// by default. add the `-r` flag to treat it as a regex.
    #[structopt(short = "F", long, multiple = true, number_of_values = 1)]
    header_field: Option<Vec<Regex>>,

    /// Treat the header_fields as regexs instead of string literals
    #[structopt(short = "r", long)]
    header_is_regex: bool,

    /// Try to find the correct decompression method based on the file extensions
    #[structopt(short = "z", long)]
    try_decompress: bool,

    /// Try to gzip compress the output
    #[structopt(short = "Z", long)]
    try_compress: bool,

    /// Threads to use for compression, 0 will result in `hck` staying single threaded.
    #[structopt(short = "t", long, default_value=DEFAULT_CPUS.as_str())]
    compression_threads: usize,

    /// Compression level
    #[structopt(short = "l", long, default_value = "6")]
    compression_level: u32,

    /// Disallow the possibility of using mmap
    #[structopt(long)]
    no_mmap: bool,

    /// Support CRLF newlines
    #[structopt(long)]
    crlf: bool,
}

fn main() -> Result<()> {
    // TODO: move tests / add more tests
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let opts = Opts::from_args();

    let writer = select_output(opts.output.as_ref())?;
    // TODO: Support all flate2 compression targets via enum on `-Z`
    let mut writer: Box<dyn Write> = if opts.try_compress {
        Box::new(
            ZBuilder::<Bgzf, _>::new()
                .compression_level(Compression::new(opts.compression_level))
                .num_threads(opts.compression_threads)
                .from_writer(writer),
        )
    } else {
        Box::new(BufWriter::new(writer))
    };

    if opts.input.is_empty() && opts.try_decompress && opts.header_field.is_some() {
        warn!("Selections based on header fields is not currently supported on STDIN compressed data.");
    }

    let inputs: Vec<HckInput<PathBuf>> = if opts.input.is_empty() {
        vec![HckInput::Stdin]
    } else {
        opts.input
            .iter()
            .map(|p| {
                if p.as_os_str() == "-" {
                    HckInput::Stdin
                } else {
                    HckInput::Path(p.clone())
                }
            })
            .collect()
    };

    let mut conf_builder = CoreConfigBuilder::new();

    let line_term = if opts.crlf {
        LineTerminator::crlf()
    } else {
        LineTerminator::default()
    };
    conf_builder = conf_builder.line_terminator(line_term);

    let mmap = if opts.no_mmap {
        MmapChoice::never()
    } else {
        unsafe { MmapChoice::auto() }
    };

    let out_delim = if opts.delim_is_literal && opts.use_input_delim {
        unescape(&opts.delimiter)
    } else {
        unescape(&opts.output_delimiter)
    };

    let conf = conf_builder
        .mmap(mmap)
        .delimiter(opts.delimiter.as_bytes())
        .output_delimiter(&out_delim)
        .is_regex_parser(!opts.delim_is_literal)
        .try_decompress(opts.try_decompress)
        .fields(opts.fields.as_deref())
        .headers(opts.header_field.as_deref())
        .exclude(opts.exclude.as_deref())
        .exclude_headers(opts.exclude_header.as_deref())
        .header_is_regex(opts.header_is_regex)
        .build()?;

    let mut line_buffer = LineBufferBuilder::new().build();

    for input in inputs.into_iter() {
        if let Err(err) = run(input, &mut writer, &conf, &mut line_buffer) {
            if is_broken_pipe(&err) {
                exit(0)
            }
            error!("{}", err);
            exit(1)
        }
    }
    Ok(())
}

/// Run the actual parsing and writing
fn run<W: Write>(
    input: HckInput<PathBuf>,
    writer: &mut W,
    conf: &CoreConfig,
    line_buffer: &mut LineBuffer,
) -> Result<()> {
    let (extra, fields) = conf.parse_fields(&input)?;
    // No point processing empty fields
    if fields.is_empty() {
        return Ok(());
    }

    match conf.parsed_delim() {
        RegexOrStr::Regex(regex) => {
            let mut core = Core::new(
                conf,
                &fields,
                RegexLineParser::new(&fields, regex),
                line_buffer,
            );
            core.hck_input(input, writer, extra)?;
        }
        RegexOrStr::Str(s) => {
            let s = unescape(s);
            let mut core = Core::new(
                conf,
                &fields,
                SubStrLineParser::new(&fields, &s),
                line_buffer,
            );
            core.hck_input(input, writer, extra)?;
        }
    };
    Ok(())
}

#[cfg(test)]
mod test {

    use std::io::BufReader;

    use super::*;
    use bstr::io::BufReadExt;
    use rstest::rstest;
    use tempfile::TempDir;

    /// Build a set of opts for testing
    fn build_opts(
        input_file: impl AsRef<Path>,
        output_file: impl AsRef<Path>,
        fields: &str,
        no_mmap: bool,
        delimiter: &str,
    ) -> Opts {
        Opts {
            input: vec![input_file.as_ref().to_path_buf()],
            output: Some(output_file.as_ref().to_path_buf()),
            delimiter: delimiter.to_string(),
            delim_is_literal: false,
            output_delimiter: "\t".to_owned(),
            use_input_delim: false,
            fields: Some(fields.to_owned()),
            header_field: None,
            header_is_regex: true,
            try_decompress: false,
            try_compress: false,
            no_mmap,
            crlf: false,
            exclude: None,
            exclude_header: None,
            compression_level: 3,
            compression_threads: 0,
        }
    }

    /// Build a set of opts for testing
    fn build_opts_not_regex(
        input_file: impl AsRef<Path>,
        output_file: impl AsRef<Path>,
        fields: &str,
        no_mmap: bool,
        delimiter: &str,
    ) -> Opts {
        Opts {
            input: vec![input_file.as_ref().to_path_buf()],
            output: Some(output_file.as_ref().to_path_buf()),
            delimiter: delimiter.to_string(),
            delim_is_literal: true,
            output_delimiter: "\t".to_owned(),
            use_input_delim: false,
            fields: Some(fields.to_owned()),
            header_field: None,
            header_is_regex: true,
            try_decompress: false,
            try_compress: false,
            no_mmap,
            crlf: false,
            exclude: None,
            exclude_header: None,
            compression_level: 3,
            compression_threads: 0,
        }
    }

    /// Build a set of opts for testing
    #[allow(clippy::too_many_arguments)]
    fn build_opts_generic(
        input_file: impl AsRef<Path>,
        output_file: impl AsRef<Path>,
        fields: Option<&str>,
        header_field: Option<Vec<Regex>>,
        exclude: Option<&str>,
        no_mmap: bool,
        delimiter: &str,
        delim_is_literal: bool,
        header_is_regex: bool,
    ) -> Opts {
        Opts {
            input: vec![input_file.as_ref().to_path_buf()],
            output: Some(output_file.as_ref().to_path_buf()),
            delimiter: delimiter.to_string(),
            delim_is_literal,
            output_delimiter: "\t".to_owned(),
            use_input_delim: false,
            fields: fields.map(|f| f.to_owned()),
            header_field,
            header_is_regex,
            try_decompress: false,
            try_compress: false,
            no_mmap,
            crlf: false,
            exclude: exclude.map(|e| e.to_owned()),
            exclude_header: None,
            compression_threads: 0,
            compression_level: 3,
        }
    }

    /// Simple function to read a tsv into a nested list of lists.
    fn read_tsv(path: impl AsRef<Path>) -> Vec<Vec<String>> {
        let reader = BufReader::new(File::open(path).unwrap());
        let mut result = vec![];
        let r = Regex::new(r"\s+").unwrap();

        for line in reader.byte_lines() {
            let line = &line.unwrap();
            result.push(
                r.split(line)
                    .map(|s| unsafe { String::from_utf8_unchecked(s.to_vec()) })
                    .collect(),
            );
        }
        result
    }

    /// Write delmited data to a file.
    fn write_file(path: impl AsRef<Path>, data: Vec<Vec<&str>>, sep: &str) {
        let mut writer = BufWriter::new(File::create(path).unwrap());
        for row in data {
            writeln!(&mut writer, "{}", row.join(sep)).unwrap();
        }
        writer.flush().unwrap();
    }

    // Wrap the run function to create the readers and writers.
    fn run_wrapper<P: AsRef<Path>>(input: P, output: P, opts: &Opts) {
        let conf = CoreConfigBuilder::new()
            .delimiter(opts.delimiter.as_bytes())
            .is_regex_parser(!opts.delim_is_literal)
            .mmap(if opts.no_mmap {
                MmapChoice::never()
            } else {
                unsafe { MmapChoice::auto() }
            })
            .output_delimiter(opts.output_delimiter.as_bytes())
            .headers(opts.header_field.as_deref())
            .fields(opts.fields.as_deref())
            .exclude(opts.exclude.as_deref())
            .exclude_headers(opts.exclude_header.as_deref())
            .header_is_regex(opts.header_is_regex)
            .build()
            .unwrap();
        let mut line_buffer = LineBufferBuilder::new().build();
        let mut writer = BufWriter::new(File::create(output).unwrap());
        run(
            HckInput::Path(input.as_ref().to_owned()),
            &mut writer,
            &conf,
            &mut line_buffer,
        )
        .unwrap();
    }

    const FOURSPACE: &str = "    ";

    #[rstest]
    fn test_exclude_one(
        #[values(true, false)] no_mmap: bool,
        #[values(r" ", "  ")] hck_delim: &str,
        #[values(true, false)] delim_is_literal: bool,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_generic(
            &input_file,
            &output_file,
            Some("1,3"),
            None,
            Some("3"),
            no_mmap,
            hck_delim,
            delim_is_literal,
            false,
        );
        let data = vec![vec!["a", "b", "c"], vec!["1", "2", "3"]];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["a",], vec!["1"]]);
    }

    #[rstest]
    fn test_exclude_range_overlap_front(
        #[values(true, false)] no_mmap: bool,
        #[values(r" ", "  ")] hck_delim: &str,
        #[values(true, false)] delim_is_literal: bool,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_generic(
            &input_file,
            &output_file,
            Some("3-"),
            None,
            Some("-5"),
            no_mmap,
            hck_delim,
            delim_is_literal,
            false,
        );
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f"],
            vec!["1", "2", "3", "4", "5", "6"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["f",], vec!["6"]]);
    }

    #[rstest]
    fn test_exclude_range_overlap_back(
        #[values(true, false)] no_mmap: bool,
        #[values(r" ", "  ")] hck_delim: &str,
        #[values(true, false)] delim_is_literal: bool,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_generic(
            &input_file,
            &output_file,
            Some("2-5"),
            None,
            Some("3-"),
            no_mmap,
            hck_delim,
            delim_is_literal,
            false,
        );
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f"],
            vec!["1", "2", "3", "4", "5", "6"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["b",], vec!["2"]]);
    }

    #[rstest]
    fn test_exclude_range_split_fields(
        #[values(true, false)] no_mmap: bool,
        #[values(r" ", "  ")] hck_delim: &str,
        #[values(true, false)] delim_is_literal: bool,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_generic(
            &input_file,
            &output_file,
            Some("1-"),
            None,
            Some("3-5"),
            no_mmap,
            hck_delim,
            delim_is_literal,
            false,
        );
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f"],
            vec!["1", "2", "3", "4", "5", "6"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["a", "b", "f"], vec!["1", "2", "6"]]);
    }

    #[rstest]
    fn test_exclude_range_all(
        #[values(true, false)] no_mmap: bool,
        #[values(r" ", "  ")] hck_delim: &str,
        #[values(true, false)] delim_is_literal: bool,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_generic(
            &input_file,
            &output_file,
            Some("4,3"),
            None,
            Some("2-5"),
            no_mmap,
            hck_delim,
            delim_is_literal,
            false,
        );
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f"],
            vec!["1", "2", "3", "4", "5", "6"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert!(filtered.is_empty());
    }

    #[rstest]
    fn test_exclude_range_split_fields_reorder(
        #[values(true, false)] no_mmap: bool,
        #[values(r" ", "  ")] hck_delim: &str,
        #[values(true, false)] delim_is_literal: bool,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_generic(
            &input_file,
            &output_file,
            Some("4-6,1-3"),
            None,
            Some("3-5"),
            no_mmap,
            hck_delim,
            delim_is_literal,
            false,
        );
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f"],
            vec!["1", "2", "3", "4", "5", "6"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["f", "a", "b"], vec!["6", "1", "2"]]);
    }

    #[rstest]
    fn test_headers_simple(
        #[values(true, false)] no_mmap: bool,
        #[values(r" ", "  ")] hck_delim: &str,
        #[values(true, false)] delim_is_literal: bool,
        #[values(true, false)] header_is_regex: bool,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_generic(
            &input_file,
            &output_file,
            None,
            Some(vec![Regex::new("a").unwrap()]),
            None,
            no_mmap,
            hck_delim,
            delim_is_literal,
            header_is_regex,
        );
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f"],
            vec!["1", "2", "3", "4", "5", "6"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["a"], vec!["1"]]);
    }

    #[rstest]
    fn test_headers_simple2(
        #[values(true, false)] no_mmap: bool,
        #[values(r" ", "  ")] hck_delim: &str,
        #[values(true, false)] delim_is_literal: bool,
        #[values(true, false)] header_is_regex: bool,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_generic(
            &input_file,
            &output_file,
            None,
            Some(vec![Regex::new("a").unwrap(), Regex::new("c").unwrap()]),
            None,
            no_mmap,
            hck_delim,
            delim_is_literal,
            header_is_regex,
        );
        let data = vec![vec!["a", "b", "c"], vec!["1", "2", "3"]];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["a", "c"], vec!["1", "3"]]);
    }

    #[rstest]
    fn test_duplicate_field_selection_more(
        #[values(true, false)] no_mmap: bool,
        #[values(r" ", "  ")] hck_delim: &str,
        #[values(true, false)] delim_is_literal: bool,
        #[values(true, false)] header_is_regex: bool,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_generic(
            &input_file,
            &output_file,
            Some("3,3,1,2"),
            None,
            None,
            no_mmap,
            hck_delim,
            delim_is_literal,
            header_is_regex,
        );
        let data = vec![vec!["a", "b", "c", "d", "e"], vec!["1", "2", "3", "4", "5"]];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["c", "a", "b"], vec!["3", "1", "2"]]);
    }

    #[rstest]
    fn test_duplicate_field_selection_range(
        #[values(true, false)] no_mmap: bool,
        #[values(r" ", "  ")] hck_delim: &str,
        #[values(true, false)] delim_is_literal: bool,
        #[values(true, false)] header_is_regex: bool,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_generic(
            &input_file,
            &output_file,
            Some("2-3,5,1,2-4"),
            None,
            None,
            no_mmap,
            hck_delim,
            delim_is_literal,
            header_is_regex,
        );
        let data = vec![vec!["a", "b", "c", "d", "e"], vec!["1", "2", "3", "4", "5"]];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(
            filtered,
            vec![vec!["b", "c", "e", "a", "d"], vec!["2", "3", "5", "1", "4"]]
        );
    }

    #[rstest]
    fn test_headers_and_fields(
        #[values(true, false)] no_mmap: bool,
        #[values(r" ", "  ")] hck_delim: &str,
        #[values(true, false)] delim_is_literal: bool,
        #[values(true, false)] header_is_regex: bool,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_generic(
            &input_file,
            &output_file,
            Some("3"),
            Some(vec![Regex::new("b").unwrap(), Regex::new("a").unwrap()]),
            None,
            no_mmap,
            hck_delim,
            delim_is_literal,
            header_is_regex,
        );
        let data = vec![vec!["a", "b", "c", "d", "e"], vec!["1", "2", "3", "4", "5"]];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["b", "c", "a"], vec!["2", "3", "1"]]);
    }

    #[rstest]
    fn test_duplicate_field_selection(
        #[values(true, false)] no_mmap: bool,
        #[values(r" ", "  ")] hck_delim: &str,
        #[values(true, false)] delim_is_literal: bool,
        #[values(true, false)] header_is_regex: bool,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_generic(
            &input_file,
            &output_file,
            Some("3,1,3"),
            None,
            None,
            no_mmap,
            hck_delim,
            delim_is_literal,
            header_is_regex,
        );
        let data = vec![vec!["a", "b", "c", "d"], vec!["1", "2", "3", "4"]];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["c", "a"], vec!["3", "1"]]);
    }
    #[rstest]
    #[rustfmt::skip::macros(vec)]
    fn test_read_single_values(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "1", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c"],
            vec!["1", "2", "3"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["a"], vec!["1"]]);
    }

    #[rstest]
    fn test_read_several_single_values(
        #[values(true, false)] no_mmap: bool,
        #[values(r"\s+")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "1,3", no_mmap, hck_delim);
        let data = vec![vec!["a", "b", "c"], vec!["1", "2", "3"]];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["a", "c"], vec!["1", "3"]]);
    }

    #[rstest]
    fn test_read_several_single_values_with_invalid_utf8(
        #[values(true, false)] no_mmap: bool,
        #[values(r"\s+")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "1,3", no_mmap, hck_delim);
        let bad_str = unsafe { String::from_utf8_unchecked(b"a\xED\xA0\x80z".to_vec()) };
        let data = vec![vec![bad_str.as_str(), "b", "c"], vec!["1", "2", "3"]];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec![bad_str.as_str(), "c"], vec!["1", "3"]]);
    }

    #[rstest]
    fn test_read_single_range(
        #[values(true, false)] no_mmap: bool,
        #[values(r"\s+")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "2-", no_mmap, hck_delim);
        let data = vec![vec!["a", "b", "c", "d"], vec!["1", "2", "3", "4"]];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["b", "c", "d"], vec!["2", "3", "4"]]);
    }

    #[rstest]
    fn test_read_serveral_range(
        #[values(true, false)] no_mmap: bool,
        #[values(r"\s+")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "2-4,6-", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(
            filtered,
            vec![vec!["b", "c", "d", "f", "g"], vec!["2", "3", "4", "6", "7"]]
        );
    }

    #[rstest]
    fn test_read_mixed_fields1(
        #[values(true, false)] no_mmap: bool,
        #[values(r"\s+")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "2,4-", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(
            filtered,
            vec![vec!["b", "d", "e", "f", "g"], vec!["2", "4", "5", "6", "7"]]
        );
    }

    #[rstest]
    fn test_read_mixed_fields2(
        #[values(true, false)] no_mmap: bool,
        #[values(r"\s+")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "-4,7", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(
            filtered,
            vec![vec!["a", "b", "c", "d", "g"], vec!["1", "2", "3", "4", "7"]]
        );
    }

    #[rstest]
    fn test_read_no_delimis_found(
        #[values(true, false)] no_mmap: bool,
        #[values(r"\s+")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "-4,7", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, "-");
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        // We hae no concept of only-delimited, so if no delim is found the whole line
        // is treated as column 1.
        assert_eq!(filtered, vec![vec!["a-b-c-d-e-f-g"], vec!["1-2-3-4-5-6-7"]]);
    }

    #[rstest]
    fn test_read_over_end(#[values(true, false)] no_mmap: bool, #[values(r"\s+")] hck_delim: &str) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "-4,8,11-", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        // columns past end in fields are ignored
        assert_eq!(
            filtered,
            vec![vec!["a", "b", "c", "d"], vec!["1", "2", "3", "4"]]
        );
    }

    #[rstest]
    fn test_reorder1(#[values(true, false)] no_mmap: bool, #[values(r"\s+")] hck_delim: &str) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "6,-4", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        // columns past end in fields are ignored
        assert_eq!(
            filtered,
            vec![vec!["f", "a", "b", "c", "d"], vec!["6", "1", "2", "3", "4"]]
        );
    }

    #[rstest]
    fn test_reorder2(#[values(true, false)] no_mmap: bool, #[values(r"\s+")] hck_delim: &str) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        // 4-5 should not be repeated at the end and only written once.
        let opts = build_opts(&input_file, &output_file, "3-,1,4-5", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        // columns past end in fields are ignored
        assert_eq!(
            filtered,
            vec![
                vec!["c", "d", "e", "f", "g", "a"],
                vec!["3", "4", "5", "6", "7", "1"]
            ]
        );
    }

    #[rstest]
    #[rustfmt::skip::macros(vec)]
    fn test_read_single_values_not_regex(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "1", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c"],
            vec!["1", "2", "3"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["a"], vec!["1"]]);
    }

    #[rstest]
    fn test_read_several_single_values_not_regex(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "1,3", no_mmap, hck_delim);
        let data = vec![vec!["a", "b", "c"], vec!["1", "2", "3"]];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["a", "c"], vec!["1", "3"]]);
    }

    #[rstest]
    fn test_read_several_single_values_with_invalid_utf8_not_regex(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "1,3", no_mmap, hck_delim);
        let bad_str = unsafe { String::from_utf8_unchecked(b"a\xED\xA0\x80z".to_vec()) };
        let data = vec![vec![bad_str.as_str(), "b", "c"], vec!["1", "2", "3"]];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec![bad_str.as_str(), "c"], vec!["1", "3"]]);
    }

    #[rstest]
    fn test_read_single_range_not_regex(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "2-", no_mmap, hck_delim);
        let data = vec![vec!["a", "b", "c", "d"], vec!["1", "2", "3", "4"]];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["b", "c", "d"], vec!["2", "3", "4"]]);
    }

    #[rstest]
    fn test_read_serveral_range_not_regex(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "2-4,6-", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(
            filtered,
            vec![vec!["b", "c", "d", "f", "g"], vec!["2", "3", "4", "6", "7"]]
        );
    }

    #[rstest]
    fn test_read_mixed_fields1_not_regex(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "2,4-", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(
            filtered,
            vec![vec!["b", "d", "e", "f", "g"], vec!["2", "4", "5", "6", "7"]]
        );
    }

    #[rstest]
    fn test_read_mixed_fields2_not_regex(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "-4,7", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(
            filtered,
            vec![vec!["a", "b", "c", "d", "g"], vec!["1", "2", "3", "4", "7"]]
        );
    }

    #[rstest]
    fn test_read_no_delimis_found_not_regex(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "-4,7", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, "-");
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        // We hae no concept of only-delimited, so if no delim is found the whole line
        // is treated as column 1.
        assert_eq!(filtered, vec![vec!["a-b-c-d-e-f-g"], vec!["1-2-3-4-5-6-7"]]);
    }

    #[rstest]
    fn test_read_over_end_not_regex(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "-4,8,11-", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        // columns past end in fields are ignored
        assert_eq!(
            filtered,
            vec![vec!["a", "b", "c", "d"], vec!["1", "2", "3", "4"]]
        );
    }

    #[rstest]
    fn test_reorder1_not_regex(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "6,-4", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        // columns past end in fields are ignored
        assert_eq!(
            filtered,
            vec![vec!["f", "a", "b", "c", "d"], vec!["6", "1", "2", "3", "4"]]
        );
    }

    #[rstest]
    fn test_reorder2_not_regex(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        // 4-5 should not be repeated at the end and only written once.
        let opts = build_opts_not_regex(&input_file, &output_file, "3-,1,4-5", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        // columns past end in fields are ignored
        assert_eq!(
            filtered,
            vec![
                vec!["c", "d", "e", "f", "g", "a"],
                vec!["3", "4", "5", "6", "7", "1"]
            ]
        );
    }

    /// Tests from users
    #[rstest]
    fn test_reorder_no_split_found(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        // 4-5 should not be repeated at the end and only written once.
        let opts = build_opts_not_regex(&input_file, &output_file, "3-,1,4-5", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, "-");
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        // columns past end in fields are ignored
        assert_eq!(filtered, vec![vec!["a-b-c-d-e-f-g"], vec!["1-2-3-4-5-6-7"]]);
    }

    /// Tests from users
    #[rstest]
    fn test_reorder_no_split_found_regex(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        // 4-5 should not be repeated at the end and only written once.
        let opts = build_opts(&input_file, &output_file, "3-,1,4-5", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, "---");
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        // columns past end in fields are ignored
        assert_eq!(
            filtered,
            vec![
                vec!["a---b---c---d---e---f---g"],
                vec!["1---2---3---4---5---6---7"]
            ]
        );
    }

    #[rstest]
    fn test_issue_12_with_regex(
        #[values(true, false)] no_mmap: bool,
        #[values(r"\s+")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        // 4-5 should not be repeated at the end and only written once.
        let opts = build_opts(&input_file, &output_file, "2,3,4-", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, "  ");
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        // columns past end in fields are ignored
        assert_eq!(
            filtered,
            vec![
                vec!["b", "c", "d", "e", "f", "g"],
                vec!["2", "3", "4", "5", "6", "7"]
            ]
        );
    }

    #[rstest]
    fn test_issue_12_no_regex(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        // 4-5 should not be repeated at the end and only written once.
        let opts = build_opts(&input_file, &output_file, "2,3,4-", no_mmap, hck_delim);
        let data = vec![
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        // columns past end in fields are ignored
        assert_eq!(
            filtered,
            vec![
                vec!["b", "c", "d", "e", "f", "g"],
                vec!["2", "3", "4", "5", "6", "7"]
            ]
        );
    }

    #[rstest]
    fn test_issue_38_not_regex(
        #[values(true, false)] no_mmap: bool,
        #[values("    ", " ")] hck_delim: &str,
    ) {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        // 4-5 should not be repeated at the end and only written once.
        let opts = build_opts_not_regex(&input_file, &output_file, "1,2", no_mmap, hck_delim);
        let data = vec![
            vec![""],
            vec![""],
            vec!["a", "b", "c", "d", "e", "f", "g"],
            vec![""],
            vec![""],
            vec!["1", "2", "3", "4", "5", "6", "7"],
        ];
        write_file(&input_file, data, hck_delim);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        // columns past end in fields are ignored
        assert_eq!(
            filtered,
            vec![
                vec![""],
                vec![""],
                vec!["a", "b"],
                vec![""],
                vec![""],
                vec!["1", "2"]
            ]
        );
    }
}
