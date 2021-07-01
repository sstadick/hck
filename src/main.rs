use anyhow::{Context, Error, Result};
use bstr::ByteSlice;
use env_logger::Env;
use grep_cli::{stdout, DecompressionReaderBuilder};
use hcklib::{
    core::Core,
    field_range::{FieldRange, RegexOrStr},
};
use log::error;
use regex::bytes::Regex;
use std::{
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
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
            format!({}", PKG_VERSION)
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
/// Determine whether we should read from a file or stdin.
fn select_input<P: AsRef<Path>>(path: P, try_decompress: bool) -> Result<Box<dyn Read>> {
    let reader: Box<dyn Read> =
        if path.as_ref().as_os_str() == "-" {
            get_stdin()
        } else if try_decompress {
            Box::new(
                DecompressionReaderBuilder::new()
                    .build(&path)
                    .with_context(|| {
                        format!("Failed to open {} for reading", path.as_ref().display())
                    })?,
            )
        } else {
            Box::new(File::open(&path).with_context(|| {
                format!("Failed to open {} for reading", path.as_ref().display())
            })?)
        };
    Ok(reader)
}

#[inline]
fn get_stdin() -> Box<dyn Read> {
    Box::new(io::stdin())
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
/// * `delimiter` is a fixed substring by default, and a regex with `-R`
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

    /// Delimiter to use on input files, this is a substring literal by default. To treat it as a regex add the
    /// `-R` flag.
    #[structopt(short = "d", long, default_value = "\t")]
    delimiter: String,

    /// Treat the delimiter as a regex
    #[structopt(short = "R", long)]
    delim_is_regex: bool,

    /// Delimiter string to use on outputs
    #[structopt(short = "D", long, default_value = "\t")]
    output_delimiter: String,

    /// Fields to keep in the output, ex: 1,2-,-5,2-5. Fields are 1-based and inclusive.
    #[structopt(short = "f", long)]
    fields: Option<String>,

    /// A string literal or regex to select headers, ex: '^is_.*$`. This is a string literal
    /// by deafult. add the `-r` flag to treat it as a regex.
    #[structopt(short = "F", long)]
    header_fields: Option<Vec<Regex>>,

    /// Treat the header_fields as regexs instead of string literals
    #[structopt(short = "r", long)]
    header_is_regex: bool,

    /// Try to find the correct decompression method based on the file extensions
    #[structopt(short = "z", long)]
    try_decompress: bool,
}

fn main() -> Result<()> {
    // TODO: add the complement argument to flip the FieldRange / excludes
    // TODO: parameterize newline
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let opts = Opts::from_args();
    let mut writer = select_output(opts.output.as_ref())?;

    let readers: Vec<Result<Box<dyn Read>>> = if opts.input.is_empty() {
        vec![Ok(get_stdin())]
    } else {
        opts.input
            .iter()
            .map(|p| select_input(p, opts.try_decompress))
            .collect()
    };

    for r in readers {
        let mut r = r?;
        if let Err(err) = run(&mut r, &mut writer, &opts) {
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
fn run<R: Read, W: Write>(reader: &mut R, writer: &mut W, opts: &Opts) -> Result<()> {
    let mut writer = BufWriter::new(writer);
    let mut reader = BufReader::new(reader);
    reader.fill_buf()?;
    let first_line = reader
        .buffer()
        .find_byte(b'\n')
        .expect("no first line found");

    let delim = if opts.delim_is_regex {
        RegexOrStr::Regex(Regex::new(&opts.delimiter)?)
    } else {
        RegexOrStr::Str(&opts.delimiter)
    };

    let fields = match (&opts.fields, &opts.header_fields) {
        (Some(field_list), Some(header_fields)) => {
            let mut fields = FieldRange::from_list(field_list)?;
            let header_fields = FieldRange::from_header_list(
                header_fields,
                &reader.buffer()[..first_line - 1],
                &delim,
                opts.header_is_regex,
            )?;
            fields.extend(header_fields.into_iter());
            FieldRange::post_process_ranges(&mut fields);
            fields
        }
        (Some(field_list), None) => FieldRange::from_list(field_list)?,
        (None, Some(header_fields)) => FieldRange::from_header_list(
            header_fields,
            &reader.buffer()[..first_line - 1],
            &delim,
            opts.header_is_regex,
        )?,
        (None, None) => {
            eprintln!("Must select one or both `fields` and 'header-fields`.");
            exit(1);
        }
    };

    let mut core = Core::new(&mut writer, &opts.output_delimiter.as_bytes(), &fields);

    match &delim {
        RegexOrStr::Regex(regex) => core.process_reader_regex(&mut reader, &regex)?,
        RegexOrStr::Str(s) => core.process_reader_substr(&mut reader, s.as_bytes())?,
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use bstr::io::BufReadExt;
    use tempfile::TempDir;

    /// Build a set of opts for testing
    fn build_opts(
        input_file: impl AsRef<Path>,
        output_file: impl AsRef<Path>,
        fields: &str,
    ) -> Opts {
        Opts {
            input: vec![input_file.as_ref().to_path_buf()],
            output: Some(output_file.as_ref().to_path_buf()),
            delimiter: String::from(r"\s+"),
            delim_is_regex: true,
            output_delimiter: "\t".to_owned(),
            fields: Some(fields.to_owned()),
            header_fields: None,
            header_is_regex: true,
            try_decompress: false,
        }
    }

    /// Build a set of opts for testing
    fn build_opts_not_regex(
        input_file: impl AsRef<Path>,
        output_file: impl AsRef<Path>,
        fields: &str,
    ) -> Opts {
        Opts {
            input: vec![input_file.as_ref().to_path_buf()],
            output: Some(output_file.as_ref().to_path_buf()),
            delimiter: String::from(FOURSPACE),
            delim_is_regex: false,
            output_delimiter: "\t".to_owned(),
            fields: Some(fields.to_owned()),
            header_fields: None,
            header_is_regex: true,
            try_decompress: false,
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
        let mut reader = BufReader::new(File::open(input).unwrap());
        let mut writer = BufWriter::new(File::create(output).unwrap());
        // let mut writer = BufferedOutput::new(writer, FLUSH_SIZE, RESERVE_SIZE, MAX_SIZE);
        run(&mut reader, &mut writer, opts).unwrap();
    }

    const FOURSPACE: &str = "    ";

    #[test]
    #[rustfmt::skip::macros(vec)]
    fn test_read_single_values() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "1");
        let data = vec![
            vec!["a", "b", "c"],
            vec!["1", "2", "3"],
        ];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["a"], vec!["1"]]);
    }

    #[test]
    fn test_read_several_single_values() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "1,3");
        let data = vec![vec!["a", "b", "c"], vec!["1", "2", "3"]];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["a", "c"], vec!["1", "3"]]);
    }

    #[test]
    fn test_read_several_single_values_with_invalid_utf8() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "1,3");
        let bad_str = unsafe { String::from_utf8_unchecked(b"a\xED\xA0\x80z".to_vec()) };
        let data = vec![vec![bad_str.as_str(), "b", "c"], vec!["1", "2", "3"]];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec![bad_str.as_str(), "c"], vec!["1", "3"]]);
    }

    #[test]
    fn test_read_single_range() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "2-");
        let data = vec![vec!["a", "b", "c", "d"], vec!["1", "2", "3", "4"]];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["b", "c", "d"], vec!["2", "3", "4"]]);
    }

    #[test]
    fn test_read_serveral_range() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "2-4,6-");
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

    #[test]
    fn test_read_mixed_fields1() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "2,4-");
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

    #[test]
    fn test_read_mixed_fields2() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "-4,7");
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

    #[test]
    fn test_read_no_delimis_found() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "-4,7");
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

    #[test]
    fn test_read_over_end() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "-4,8,11-");
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

    #[test]
    fn test_reorder1() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "6,-4");
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

    #[test]
    fn test_reorder2() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        // 4-5 should not be repeated at the end and only written once.
        let opts = build_opts(&input_file, &output_file, "3-,1,4-5");
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

    #[test]
    #[rustfmt::skip::macros(vec)]
    fn test_read_single_values_not_regex() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "1");
        let data = vec![
            vec!["a", "b", "c"],
            vec!["1", "2", "3"],
        ];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["a"], vec!["1"]]);
    }

    #[test]
    fn test_read_several_single_values_not_regex() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "1,3");
        let data = vec![vec!["a", "b", "c"], vec!["1", "2", "3"]];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["a", "c"], vec!["1", "3"]]);
    }

    #[test]
    fn test_read_several_single_values_with_invalid_utf8_not_regex() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "1,3");
        let bad_str = unsafe { String::from_utf8_unchecked(b"a\xED\xA0\x80z".to_vec()) };
        let data = vec![vec![bad_str.as_str(), "b", "c"], vec!["1", "2", "3"]];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec![bad_str.as_str(), "c"], vec!["1", "3"]]);
    }

    #[test]
    fn test_read_single_range_not_regex() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "2-");
        let data = vec![vec!["a", "b", "c", "d"], vec!["1", "2", "3", "4"]];
        write_file(&input_file, data, FOURSPACE);
        run_wrapper(&input_file, &output_file, &opts);
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["b", "c", "d"], vec!["2", "3", "4"]]);
    }

    #[test]
    fn test_read_serveral_range_not_regex() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "2-4,6-");
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

    #[test]
    fn test_read_mixed_fields1_not_regex() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "2,4-");
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

    #[test]
    fn test_read_mixed_fields2_not_regex() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "-4,7");
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

    #[test]
    fn test_read_no_delimis_found_not_regex() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "-4,7");
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

    #[test]
    fn test_read_over_end_not_regex() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "-4,8,11-");
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

    #[test]
    fn test_reorder1_not_regex() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts_not_regex(&input_file, &output_file, "6,-4");
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

    #[test]
    fn test_reorder2_not_regex() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        // 4-5 should not be repeated at the end and only written once.
        let opts = build_opts_not_regex(&input_file, &output_file, "3-,1,4-5");
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
}
