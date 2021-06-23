use regex::Regex;
use std::{
    cmp::max,
    error::Error,
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
};
use structopt::StructOpt;
use thiserror::Error;

/// Errors for parsing / validating [`FieldRange`] strings.
#[derive(Error, Debug)]
enum FieldError {
    #[error("Fields and positions are numbered from 1: {0}")]
    InvalidField(usize),
    #[error("High end of range less than low end of range: {0}-{1}")]
    InvalidOrder(usize, usize),
    #[error("Failed to parse field: {0}")]
    FailedParse(String),
}

/// Represent a range of columns to keep.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
struct FieldRange {
    low: usize,
    high: usize,
}

impl FromStr for FieldRange {
    type Err = FieldError;

    /// Convert a [`str`] into a [`FieldRange`]
    fn from_str(s: &str) -> Result<FieldRange, FieldError> {
        const MAX: usize = usize::MAX;

        let mut parts = s.splitn(2, '-');

        match (parts.next(), parts.next()) {
            (Some(nm), None) => {
                if let Ok(nm) = nm.parse::<usize>() {
                    if nm > 0 {
                        Ok(FieldRange {
                            low: nm - 1,
                            high: nm - 1,
                        })
                    } else {
                        Err(FieldError::InvalidField(nm))
                    }
                } else {
                    Err(FieldError::FailedParse(nm.to_owned()))
                }
            }
            (Some(n), Some(m)) if m.is_empty() => {
                if let Ok(low) = n.parse::<usize>() {
                    if low > 0 {
                        Ok(FieldRange {
                            low: low - 1,
                            high: MAX - 1,
                        })
                    } else {
                        Err(FieldError::InvalidField(low))
                    }
                } else {
                    Err(FieldError::FailedParse(n.to_owned()))
                }
            }
            (Some(n), Some(m)) if n.is_empty() => {
                if let Ok(high) = m.parse::<usize>() {
                    if high > 0 {
                        Ok(FieldRange {
                            low: 0,
                            high: high - 1,
                        })
                    } else {
                        Err(FieldError::InvalidField(high))
                    }
                } else {
                    Err(FieldError::FailedParse(m.to_owned()))
                }
            }
            (Some(n), Some(m)) => match (n.parse::<usize>(), m.parse::<usize>()) {
                (Ok(low), Ok(high)) => {
                    if low > 0 && low <= high {
                        Ok(FieldRange {
                            low: low - 1,
                            high: high - 1,
                        })
                    } else if low == 0 {
                        Err(FieldError::InvalidField(low))
                    } else {
                        Err(FieldError::InvalidOrder(low, high))
                    }
                }
                _ => Err(FieldError::FailedParse(format!("{}-{}", n, m))),
            },
            _ => unreachable!(),
        }
    }
}

impl FieldRange {
    /// Parse a comma separated list of fields and merge any overlaps
    pub fn from_list(list: &str) -> Result<Vec<FieldRange>, FieldError> {
        let mut ranges: Vec<FieldRange> = vec![];
        for item in list.split(',') {
            ranges.push(FromStr::from_str(item)?);
        }
        ranges.sort();
        // merge overlapping ranges
        for i in 0..ranges.len() {
            let j = i + 1;

            while j < ranges.len() && ranges[j].low <= ranges[i].high {
                let j_high = ranges.remove(j).high;
                ranges[i].high = max(ranges[i].high, j_high);
            }
        }

        Ok(ranges)
    }
}

/// Determine whether we should read from a file or stdin.
fn select_input<P: AsRef<Path>>(input: Option<P>) -> Result<Box<dyn Read>, io::Error> {
    let reader: Box<dyn Read> = match input {
        Some(path) => {
            if path.as_ref().as_os_str() == "-" {
                Box::new(io::stdin())
            } else {
                Box::new(File::open(path)?)
            }
        }
        None => Box::new(io::stdin()),
    };
    Ok(reader)
}

/// Determine if we should write to a file or stdout.
fn select_output<P: AsRef<Path>>(output: Option<P>) -> Result<Box<dyn Write>, io::Error> {
    let writer: Box<dyn Write> = match output {
        Some(path) => {
            if path.as_ref().as_os_str() == "-" {
                // TODO: verify that stdout buffers when writing to a terminal now (this was a bug in Rust at some point).
                Box::new(io::stdout())
            } else {
                Box::new(File::create(path)?)
            }
        }
        None => Box::new(io::stdout()),
    };
    Ok(writer)
}

/// A rougher form of the unix tool `cut` that uses a regex delimiter instead of a fixed string.
#[derive(Debug, StructOpt)]
#[structopt(name = "hck", about = "A regex based version of cut.")]
struct Opts {
    /// Input files to parse, defaults to stdin
    #[structopt(short, long)]
    input: Option<PathBuf>,

    /// Output file to write to, defaults to stdout
    #[structopt(short, long)]
    output: Option<PathBuf>,

    /// Regex delimiter to use on input files
    #[structopt(short, long, default_value = r"\t")]
    delimiter: Regex,

    /// Delimiter string to use on outputs
    #[structopt(short = "D", long, default_value = "\t")]
    output_delimiter: String,

    /// Fields to keep in the output, ex: 1,2-,-5,2-5. Fields are 1-based and inclusive.
    #[structopt(short, long, default_value = "1-")]
    fields: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    // TODO: add the complement argument to flip the FieldRange
    // TODO: handle errors and pipe closing more gracefully
    // TODO: allow header selectors in fields.
    let opts = Opts::from_args();
    run(&opts)?;
    Ok(())
}

/// Run the actual parsing and writing
fn run(opts: &Opts) -> Result<(), Box<dyn Error>> {
    let mut reader = BufReader::new(select_input(opts.input.as_ref())?);
    let mut writer = BufWriter::new(select_output(opts.output.as_ref())?);
    let fields = FieldRange::from_list(&opts.fields)?;
    // here get fields from either opts.fields or opts.headers (-F)
    // if opts.headers read and parse the first line, figure out how make it uniform though

    let mut buffer = String::new();
    loop {
        if reader.read_line(&mut buffer)? == 0 {
            break;
        }
        // pop the newline off the string
        buffer.pop();

        // Create a lazy splitter
        let mut parts = opts.delimiter.split(&buffer).peekable();
        let mut iterator_index = 0;
        let mut print_delim = false;

        // Iterate over our ranges and write any fields that are contained by them.
        for &FieldRange { low, high } in &fields {
            // Advance up to low end of range
            if low > iterator_index {
                match parts.nth(low - iterator_index - 1) {
                    Some(_part) => {
                        iterator_index = low;
                    }
                    None => break,
                }
            }

            // Advance through the range
            for _ in 0..=high - low {
                match parts.next() {
                    Some(part) => {
                        if print_delim {
                            write!(&mut writer, "{}", &opts.output_delimiter)?;
                        }
                        write!(&mut writer, "{}", part)?;
                        print_delim = true;
                    }
                    None => break,
                }
                iterator_index += 1;
            }
        }
        // Write endline
        writeln!(&mut writer)?;

        buffer.clear()
    }

    writer.flush()?;

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use tempfile::TempDir;

    #[test]
    #[rustfmt::skip::macros(assert_eq)]
    fn test_parse_fields_good() {
        assert_eq!(vec![FieldRange { low: 0, high: 0 }], FieldRange::from_list("1").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 0 },  FieldRange { low: 3, high: 3 }], FieldRange::from_list("1,4").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 0 },  FieldRange { low: 3, high: usize::MAX - 1 }], FieldRange::from_list("1,4-").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 0 },  FieldRange { low: 3, high: usize::MAX - 1 }], FieldRange::from_list("1,4-,5-8").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 3 }], FieldRange::from_list("-4").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 3 },  FieldRange { low: 4, high: 7 }], FieldRange::from_list("-4,5-8").unwrap());
    }

    #[test]
    fn test_parse_fields_bad() {
        assert!(FieldRange::from_list("0").is_err());
        assert!(FieldRange::from_list("4-1").is_err());
        assert!(FieldRange::from_list("cat").is_err());
        assert!(FieldRange::from_list("1-dog").is_err());
        assert!(FieldRange::from_list("mouse-4").is_err());
    }

    /// Build a set of opts for testing
    fn build_opts(
        input_file: impl AsRef<Path>,
        output_file: impl AsRef<Path>,
        fields: &str,
    ) -> Opts {
        Opts {
            input: Some(input_file.as_ref().to_path_buf()),
            output: Some(output_file.as_ref().to_path_buf()),
            delimiter: Regex::new(r"\s+").unwrap(),
            output_delimiter: "\t".to_owned(),
            fields: fields.to_owned(),
        }
    }

    /// Simple function to read a tsv into a nested list of lists.
    fn read_tsv(path: impl AsRef<Path>) -> Vec<Vec<String>> {
        let reader = BufReader::new(File::open(path).unwrap());
        let mut result = vec![];
        for line in reader.lines() {
            let line = line.unwrap();
            result.push(
                line.split('\t')
                    .map(|s| s.to_owned())
                    .collect::<Vec<String>>(),
            )
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
        write_file(input_file, data, FOURSPACE);
        run(&opts).unwrap();
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
        write_file(input_file, data, FOURSPACE);
        run(&opts).unwrap();
        let filtered = read_tsv(output_file);

        assert_eq!(filtered, vec![vec!["a", "c"], vec!["1", "3"]]);
    }

    #[test]
    fn test_read_single_range() {
        let tmp = TempDir::new().unwrap();
        let input_file = tmp.path().join("input.txt");
        let output_file = tmp.path().join("output.txt");
        let opts = build_opts(&input_file, &output_file, "2-");
        let data = vec![vec!["a", "b", "c", "d"], vec!["1", "2", "3", "4"]];
        write_file(input_file, data, FOURSPACE);
        run(&opts).unwrap();
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
        write_file(input_file, data, FOURSPACE);
        run(&opts).unwrap();
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
        write_file(input_file, data, FOURSPACE);
        run(&opts).unwrap();
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
        write_file(input_file, data, FOURSPACE);
        run(&opts).unwrap();
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
        write_file(input_file, data, "-");
        run(&opts).unwrap();
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
        write_file(input_file, data, FOURSPACE);
        run(&opts).unwrap();
        let filtered = read_tsv(output_file);

        // columns past end in fields are ignored
        assert_eq!(
            filtered,
            vec![vec!["a", "b", "c", "d"], vec!["1", "2", "3", "4"]]
        );
    }
}
