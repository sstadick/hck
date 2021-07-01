//! Parse ranges like `-2,5,8-10,13-`.
//!
//! # Examples
//!
//! TODO

use bstr::ByteSlice;
use regex::bytes::Regex;
use std::{cmp::max, str::FromStr};
use thiserror::Error;

/// Errors for parsing / validating [`FieldRange`] strings.
#[derive(Error, Debug)]
pub enum FieldError {
    #[error("Fields and positions are numbered from 1: {0}")]
    InvalidField(usize),
    #[error("High end of range less than low end of range: {0}-{1}")]
    InvalidOrder(usize, usize),
    #[error("Failed to parse field: {0}")]
    FailedParse(String),
    #[error("No headers matched")]
    NoHeadersMatched,
}

pub enum RegexOrStr<'a> {
    Regex(&'a Regex),
    Str(&'a str),
}

// impl<'a> RegexOrStr<'a> {
//     fn split<'b>(&self, line: &[u8]) -> Box<dyn Iterator<Item = &[u8]>> {
//         match self {
//             RegexOrStr::Regex(r) => Box::new(r.split(line)),
//             RegexOrStr::Str(s) => Box::new(line.split_str(s)),
//         }
//     }
// }

/// Represent a range of columns to keep.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct FieldRange {
    pub low: usize,
    pub high: usize,
    // The initial position of this range in the user input
    pub pos: usize,
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
                            pos: 0,
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
                            pos: 0,
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
                            pos: 0,
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
                            pos: 0,
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
        for (i, item) in list.split(',').enumerate() {
            let mut rnge: FieldRange = FromStr::from_str(item)?;
            rnge.pos = i;
            ranges.push(rnge);
        }
        FieldRange::post_process_ranges(&mut ranges);

        Ok(ranges)
    }

    /// Get the indices of the headers that match any of the provided regex's.
    pub fn from_header_list(
        list: &[Regex],
        header: &[u8],
        delim: &str,
        literal: bool,
    ) -> Result<Vec<FieldRange>, FieldError> {
        let mut ranges = vec![];
        for (i, header) in delim.split(header).enumerate() {
            for (j, regex) in list.iter().enumerate() {
                if literal {
                    if regex.as_str().as_bytes() == header {
                        ranges.push(FieldRange {
                            low: i,
                            high: i,
                            pos: j,
                        });
                    }
                } else if regex.is_match(header) {
                    ranges.push(FieldRange {
                        low: i,
                        high: i,
                        pos: j,
                    });
                }
            }
        }

        if ranges.is_empty() {
            return Err(FieldError::NoHeadersMatched);
        }

        FieldRange::post_process_ranges(&mut ranges);

        Ok(ranges)
    }

    /// Sort and merge overlaps in a set of [`Vec<FieldRange>`].
    pub fn post_process_ranges(ranges: &mut Vec<FieldRange>) {
        ranges.sort();
        // merge overlapping ranges
        for i in 0..ranges.len() {
            let j = i + 1;

            while j < ranges.len()
                && ranges[j].low <= ranges[i].high + 1
                && (ranges[j].pos == ranges[i].pos || ranges[j].pos - 1 == ranges[i].pos)
            {
                let j_high = ranges.remove(j).high;
                ranges[i].high = max(ranges[i].high, j_high);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[rustfmt::skip::macros(assert_eq)]
    fn test_parse_fields_good() {
        assert_eq!(vec![FieldRange { low: 0, high: 0, pos: 0}], FieldRange::from_list("1").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 0, pos: 0},  FieldRange { low: 3, high: 3, pos: 1}], FieldRange::from_list("1,4").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 1, pos: 0},  FieldRange { low: 3, high: usize::MAX - 1, pos: 2}], FieldRange::from_list("1,2,4-").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 0, pos: 0},  FieldRange { low: 3, high: usize::MAX - 1, pos: 1}], FieldRange::from_list("1,4-,5-8").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 0, pos: 1},  FieldRange { low: 3, high: usize::MAX - 1, pos: 0}, FieldRange { low: 4, high: 7, pos: 2}], FieldRange::from_list("4-,1,5-8").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 3, pos: 0}], FieldRange::from_list("-4").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 7, pos: 0}], FieldRange::from_list("-4,5-8").unwrap());
    }

    #[test]
    fn test_parse_fields_bad() {
        assert!(FieldRange::from_list("0").is_err());
        assert!(FieldRange::from_list("4-1").is_err());
        assert!(FieldRange::from_list("cat").is_err());
        assert!(FieldRange::from_list("1-dog").is_err());
        assert!(FieldRange::from_list("mouse-4").is_err());
    }

    #[test]
    fn test_parse_header_fields() {
        let header = b"is_cat-isdog-wascow-was_is_apple-12345-!$%*(_)";
        let delim = Regex::new("-").unwrap();
        let header_fields = vec![
            Regex::new(r"^is_.*$").unwrap(),
            Regex::new("dog").unwrap(),
            Regex::new(r"\$%").unwrap(),
        ];
        let fields = FieldRange::from_header_list(&header_fields, header, &delim, false).unwrap();
        assert_eq!(
            vec![
                FieldRange {
                    low: 0,
                    high: 1,
                    pos: 0
                },
                FieldRange {
                    low: 5,
                    high: 5,
                    pos: 2 // pos 2 because it's the 3rd regex in header_fields
                }
            ],
            fields
        );
    }

    #[test]
    fn test_parse_header_fields_literal() {
        let header = b"is_cat-is-isdog-wascow-was_is_apple-12345-!$%*(_)";
        let delim = Regex::new("-").unwrap();
        let header_fields = vec![
            Regex::new(r"^is_.*$").unwrap(),
            Regex::new("dog").unwrap(),
            Regex::new(r"\$%").unwrap(),
            Regex::new(r"is").unwrap(),
        ];
        let fields = FieldRange::from_header_list(&header_fields, header, &delim, true).unwrap();
        assert_eq!(
            vec![FieldRange {
                low: 1,
                high: 1,
                pos: 3
            },],
            fields
        );
    }
}
