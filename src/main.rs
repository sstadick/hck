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

    /// Only output lines where a delimiter was found
    #[structopt(short = "s", long)]
    only_delimited: bool,

    /// Fields to keep in the output, ex: 1,2-,-5,2-5. Fields are 1-based and inclusive.
    #[structopt(short, long, default_value = "1-")]
    fields: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let opts = Opts::from_args();
    let fields = FieldRange::from_list(&opts.fields)?;
    let mut reader = BufReader::new(select_input(opts.input.as_ref())?);
    let mut writer = BufWriter::new(select_output(opts.output.as_ref())?);

    // TODO: add the complement argument to flip the FieldRange

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

        // If no delim is found, maybe write line and continue
        if parts.peek().is_none() {
            if !opts.only_delimited {
                writeln!(&mut writer, "{}", buffer)?;
            }
            continue;
        }

        // Iterate over our ranges and write any fields that are contained by them.
        for (i, &FieldRange { low, high }) in fields.iter().enumerate() {
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
            for j in 0..=high - low {
                match parts.next() {
                    Some(part) => {
                        write!(&mut writer, "{}", part)?;
                        // Print the separator if there is a next value AND we are not at the end of the ranges, or not at the end of the current range
                        if (i < fields.len() || j < high - low) && parts.peek().is_some() {
                            write!(&mut writer, "{}", &opts.output_delimiter)?;
                        }
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
