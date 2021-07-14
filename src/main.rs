use anyhow::{Context, Error, Result};
use bstr::ByteSlice;
use env_logger::Env;
use grep_cli::stdout;
use hcklib::{
    core::{Core, CoreConfig, CoreConfigBuilder, HckInput},
    field_range::{FieldRange, RegexOrStr},
    line_parser::{RegexLineParser, SubStrLineParser},
    mmap::MmapChoice,
};
use log::error;
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
fn select_output<P: AsRef<Path>>(output: Option<P>) -> Result<Box<dyn Write>> {
    let writer: Box<dyn Write> = match output {
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

/// Select and reorder columns.
///
/// This tool behaves like unix `cut` with a few exceptions:
///
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
    #[structopt(short = "E", long)]
    exclude_header: Option<Vec<Regex>>,

    /// A string literal or regex to select headers, ex: '^is_.*$`. This is a string literal
    /// by default. add the `-r` flag to treat it as a regex.
    #[structopt(short = "F", long)]
    header_field: Option<Vec<Regex>>,

    /// Treat the header_fields as regexs instead of string literals
    #[structopt(short = "r", long)]
    header_is_regex: bool,

    /// Try to find the correct decompression method based on the file extensions
    #[structopt(short = "z", long)]
    try_decompress: bool,

    /// Be greedy with regex delimiter, i.e. `[[:space:]]` instead of `[[:space:]]+` and "empty"
    /// fields will be thrown away. This is the default behavior of awk and is significantly more
    /// preferment than a regex with a `+`.
    #[structopt(short = "g", long, conflicts_with = "delimiter_is_literal")]
    greedy_regex: bool,

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

    let mut writer = select_output(opts.output.as_ref())?;

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
    conf_builder.line_terminator(line_term);

    let mmap = if opts.no_mmap {
        MmapChoice::never()
    } else {
        unsafe { MmapChoice::auto() }
    };
    conf_builder.mmap(mmap);
    conf_builder.delimiter(&opts.delimiter.as_bytes());
    conf_builder.output_delimiter(&opts.output_delimiter.as_bytes());
    conf_builder.is_regex_parser(!opts.delim_is_literal);
    conf_builder.try_decompress(opts.try_decompress);
    let conf = conf_builder.build();

    let mut line_buffer = LineBufferBuilder::new().build();

    for input in inputs.into_iter() {
        if let Err(err) = run(input, &mut writer, &opts, conf, &mut line_buffer) {
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
    opts: &Opts,
    conf: CoreConfig,
    line_buffer: &mut LineBuffer,
) -> Result<()> {
    let writer = BufWriter::new(writer);

    let delim = if conf.is_parser_regex() {
        RegexOrStr::Regex(Regex::new(conf.delimiter().to_str()?)?)
    } else {
        RegexOrStr::Str(conf.delimiter().to_str()?)
    };

    // Parser the fields in the context of the files being looked at
    let (mut extra, fields) = match (&opts.fields, &opts.header_field) {
        (Some(field_list), Some(header_fields)) => {
            let first_line = input.peek_first_line()?;
            let mut fields = FieldRange::from_list(field_list)?;
            let header_fields = FieldRange::from_header_list(
                header_fields,
                first_line.as_bytes(),
                &delim,
                opts.header_is_regex,
            )?;
            fields.extend(header_fields.into_iter());
            FieldRange::post_process_ranges(&mut fields);
            (Some(first_line), fields)
        }
        (Some(field_list), None) => (None, FieldRange::from_list(field_list)?),
        (None, Some(header_fields)) => {
            let first_line = input.peek_first_line()?;
            let fields = FieldRange::from_header_list(
                header_fields,
                first_line.as_bytes(),
                &delim,
                opts.header_is_regex,
            )?;
            (Some(first_line), fields)
        }
        (None, None) => (None, FieldRange::from_list("1-")?),
    };

    let fields = match (&opts.exclude, &opts.exclude_header) {
        (Some(exclude), Some(exclude_header)) => {
            let exclude = FieldRange::from_list(exclude)?;
            let fields = FieldRange::exclude(fields, exclude);
            let first_line = if let Some(first_line) = extra {
                first_line
            } else {
                input.peek_first_line()?
            };
            let exclude_headers = FieldRange::from_header_list(
                &exclude_header,
                first_line.as_bytes(),
                &delim,
                opts.header_is_regex,
            )?;
            extra = Some(first_line);
            FieldRange::exclude(fields, exclude_headers)
        }
        (Some(exclude), None) => {
            let exclude = FieldRange::from_list(exclude)?;
            FieldRange::exclude(fields, exclude)
        }
        (None, Some(exclude_header)) => {
            let first_line = if let Some(first_line) = extra {
                first_line
            } else {
                input.peek_first_line()?
            };
            let exclude_headers = FieldRange::from_header_list(
                &exclude_header,
                first_line.as_bytes(),
                &delim,
                opts.header_is_regex,
            )?;
            extra = Some(first_line);
            FieldRange::exclude(fields, exclude_headers)
        }
        (None, None) => fields,
    };

    let fields = if let Some(exclude) = &opts.exclude {
        let exclude = FieldRange::from_list(exclude)?;
        // remove all ranges in the exclude list from the fields list
        FieldRange::exclude(fields, exclude)
    } else {
        fields
    };

    match &delim {
        RegexOrStr::Regex(regex) => {
            let mut core = Core::new(
                &conf,
                &fields,
                RegexLineParser::new(&fields, &regex, opts.greedy_regex),
                line_buffer,
            );
            core.hck_input(input, writer, extra)?;
        }
        RegexOrStr::Str(s) => {
            let mut core = Core::new(
                &conf,
                &fields,
                SubStrLineParser::new(&fields, s.as_bytes()),
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
            fields: Some(fields.to_owned()),
            header_field: None,
            header_is_regex: true,
            try_decompress: false,
            no_mmap,
            crlf: false,
            exclude: None,
            exclude_header: None,
            greedy_regex: false,
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
            fields: Some(fields.to_owned()),
            header_field: None,
            header_is_regex: true,
            try_decompress: false,
            no_mmap,
            crlf: false,
            exclude: None,
            exclude_header: None,
            greedy_regex: false,
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
            .build();
        let mut line_buffer = LineBufferBuilder::new().build();
        let mut writer = BufWriter::new(File::create(output).unwrap());
        run(
            HckInput::Path(input.as_ref().to_owned()),
            &mut writer,
            opts,
            conf,
            &mut line_buffer,
        )
        .unwrap();
    }

    const FOURSPACE: &str = "    ";

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
}
