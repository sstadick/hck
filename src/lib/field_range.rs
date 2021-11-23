//! Parse ranges like `-2,5,8-10,13-`.
//!
//! # Examples
//!
//! TODO

use bstr::ByteSlice;
use regex::bytes::Regex;
use std::{cmp::max, collections::VecDeque, str::FromStr};
use thiserror::Error;

/// The fartest right possible field
const MAX: usize = usize::MAX;

/// Errors for parsing / validating [`FieldRange`] strings.
#[derive(Error, Debug, PartialEq)]
pub enum FieldError {
    #[error("Header not found: {0}")]
    HeaderNotFound(String),
    #[error("Fields and positions are numbered from 1: {0}")]
    InvalidField(usize),
    #[error("High end of range less than low end of range: {0}-{1}")]
    InvalidOrder(usize, usize),
    #[error("Failed to parse field: {0}")]
    FailedParse(String),
    #[error("No headers matched")]
    NoHeadersMatched,
}

#[derive(Debug, Clone)]
pub enum RegexOrString {
    Regex(Regex),
    String(String),
}

impl RegexOrString {
    fn split<'a>(&'a self, line: &'a [u8]) -> Box<dyn Iterator<Item = &'a [u8]> + 'a> {
        match self {
            RegexOrString::Regex(r) => Box::new(r.split(line)),
            RegexOrString::String(s) => Box::new(line.split_str(s)),
        }
    }
}

/// Represent a range of columns to keep.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Copy, Clone)]
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
    pub const fn default() -> Self {
        Self {
            low: 0,
            high: MAX - 1,
            pos: 0,
        }
    }

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
        delim: &RegexOrString,
        header_is_regex: bool,
        allow_missing: bool,
    ) -> Result<Vec<FieldRange>, FieldError> {
        let mut ranges = vec![];
        let mut found = vec![false; list.len()];
        for (i, header) in delim.split(header).enumerate() {
            for (j, regex) in list.iter().enumerate() {
                if !header_is_regex {
                    if regex.as_str().as_bytes() == header {
                        found[j] = true;
                        ranges.push(FieldRange {
                            low: i,
                            high: i,
                            pos: j,
                        });
                    }
                } else if regex.is_match(header) {
                    found[j] = true;
                    ranges.push(FieldRange {
                        low: i,
                        high: i,
                        pos: j,
                    });
                }
            }
        }

        if !allow_missing {
            if ranges.is_empty() {
                return Err(FieldError::NoHeadersMatched);
            }
            for (i, was_found) in found.into_iter().enumerate() {
                if !was_found {
                    return Err(FieldError::HeaderNotFound(list[i].as_str().to_owned()));
                }
            }
        }

        FieldRange::post_process_ranges(&mut ranges);

        Ok(ranges)
    }

    /// Sort and merge overlaps in a set of [`Vec<FieldRange>`].
    pub fn post_process_ranges(ranges: &mut Vec<FieldRange>) {
        ranges.sort();
        // merge overlapping ranges
        let mut shifted = 0;
        for i in 0..ranges.len() {
            let j = i + 1;
            if let Some(rng) = ranges.get_mut(i) {
                rng.pos = rng.pos.saturating_sub(shifted);
            }

            while j < ranges.len()
                && ranges[j].low <= ranges[i].high + 1
                && (ranges[j].pos == ranges[i].pos
                    || ranges[j].pos.saturating_sub(1) == ranges[i].pos)
            {
                let j_high = ranges.remove(j).high;
                ranges[i].high = max(ranges[i].high, j_high);
                shifted += 1;
            }
        }
    }

    /// Test if a value is contained in this range
    pub fn contains(&self, value: usize) -> bool {
        value >= self.low && value <= self.high
    }

    /// Test if two ranges overlap
    pub fn overlap(&self, other: &Self) -> bool {
        self.low <= other.high && self.high >= other.low
    }

    /// Remove ranges in exclude from fields.
    ///
    /// This assumes both fields and exclude are in ascending order by `low` value.
    pub fn exclude(fields: Vec<FieldRange>, exclude: Vec<FieldRange>) -> Vec<FieldRange> {
        let mut fields: VecDeque<_> = fields.into_iter().collect();
        let mut result = vec![];
        let mut exclude_iter = exclude.into_iter();
        let mut exclusion = if let Some(ex) = exclude_iter.next() {
            ex
        } else {
            // Early return, no exclusions
            return fields.into_iter().collect();
        };
        let mut field = fields.pop_front().unwrap(); // Must have at least one field
        loop {
            // Determine if there is any overlap at all
            if exclusion.overlap(&field) {
                // Determine the type of overlap
                match (
                    exclusion.contains(field.low),
                    exclusion.contains(field.high),
                ) {
                    // Field: XXXXXXXX
                    // Exclu:      XXXXXXXX
                    (false, true) => {
                        if exclusion.low != 0 {
                            field.high = exclusion.low - 1;
                        }
                    }

                    // Field:    XXXXXXXX
                    // Exclu: XXXXX
                    (true, false) => {
                        if exclusion.high != MAX - 1 {
                            field.low = exclusion.high + 1;
                        }
                    }
                    // Field:    XXXXX
                    // Exclu: XXXXXXXXXX
                    (true, true) => {
                        // Skip since we are excluding all fields in this range
                        if let Some(f) = fields.pop_front() {
                            field = f;
                        } else {
                            break;
                        }
                    }

                    // Field: XXXXXXXXXX
                    // exclu:     XXXX
                    (false, false) => {
                        // Split the field
                        // high side
                        if exclusion.high != MAX - 1 {
                            let mut high_field = field;
                            high_field.low = exclusion.high + 1;
                            fields.push_front(high_field)
                        }

                        // low side
                        if exclusion.low != 0 {
                            field.high = exclusion.low - 1;
                        }
                    }
                }
            } else if field.low > exclusion.high {
                // if the exclusion is behind the field, advance the exclusion
                if let Some(ex) = exclude_iter.next() {
                    exclusion = ex;
                } else {
                    result.push(field);
                    result.extend(fields.into_iter());
                    break;
                }
            } else if field.high < exclusion.low {
                // if the exclusion is ahead of the field, push the field
                result.push(field);
                if let Some(f) = fields.pop_front() {
                    field = f;
                } else {
                    break;
                }
            } else {
                unreachable!()
            }
        }
        result
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
        assert_eq!(vec![FieldRange { low: 0, high: 1, pos: 0},  FieldRange { low: 3, high: usize::MAX - 1, pos: 1}], FieldRange::from_list("1,2,4-").unwrap());
        assert_eq!(vec![FieldRange { low: 1, high: 2, pos: 0},  FieldRange { low: 3, high: usize::MAX - 1, pos: 1} ], FieldRange::from_list("2,3,4-").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 0, pos: 0},  FieldRange { low: 3, high: usize::MAX - 1, pos: 1}], FieldRange::from_list("1,4-,5-8").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 0, pos: 1},  FieldRange { low: 3, high: usize::MAX - 1, pos: 0}, FieldRange { low: 4, high: 7, pos: 2}], FieldRange::from_list("4-,1,5-8").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 3, pos: 0}], FieldRange::from_list("-4").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 7, pos: 0}], FieldRange::from_list("-4,5-8").unwrap());
        assert_eq!(vec![FieldRange { low: 0, high: 0, pos: 1 }, FieldRange { low: 2, high: 2, pos: 0}, FieldRange { low: 2, high: 2, pos: 2}], FieldRange::from_list("3,1,3").unwrap());
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
        let delim = RegexOrString::Regex(delim);
        let header_fields = vec![
            Regex::new(r"^is_.*$").unwrap(),
            Regex::new("dog").unwrap(),
            Regex::new(r"\$%").unwrap(),
        ];
        let fields =
            FieldRange::from_header_list(&header_fields, header, &delim, true, false).unwrap();
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
                    pos: 1
                }
            ],
            fields
        );
    }

    #[test]
    fn test_parse_header_fields_literal() {
        let header = b"is_cat-is-isdog-wascow-was_is_apple-12345-!$%*(_)";
        let delim = Regex::new("-").unwrap();
        let delim = RegexOrString::Regex(delim);
        let header_fields = vec![Regex::new(r"is").unwrap()];
        let fields =
            FieldRange::from_header_list(&header_fields, header, &delim, false, false).unwrap();
        assert_eq!(
            vec![FieldRange {
                low: 1,
                high: 1,
                pos: 0
            },],
            fields
        );
    }

    #[test]
    fn test_parse_header_fields_literal_header_not_found() {
        let header = b"is_cat-is-isdog-wascow-was_is_apple-12345-!$%*(_)";
        let delim = Regex::new("-").unwrap();
        let delim = RegexOrString::Regex(delim);
        let header_fields = vec![
            Regex::new(r"^is_.*$").unwrap(),
            Regex::new("dog").unwrap(),
            Regex::new(r"\$%").unwrap(),
            Regex::new(r"is").unwrap(),
        ];
        let result = FieldRange::from_header_list(&header_fields, header, &delim, false, false);
        assert_eq!(
            result.unwrap_err(),
            FieldError::HeaderNotFound(String::from(r"^is_.*$"))
        );
    }

    #[test]
    #[rustfmt::skip::macros(assert_eq)]
    fn test_exclude_simple() {
        assert_eq!(
            vec![
                FieldRange { low: 1, high: MAX - 1, pos: 0}
            ],
            FieldRange::exclude(
                vec![FieldRange { low: 0, high: MAX - 1, pos: 0}],
                vec![FieldRange { low: 0, high: 0,       pos: 0}]
            ),
            "1"
        );
        assert_eq!(
            vec![
                FieldRange { low: 1, high: 2,       pos: 0},
                FieldRange { low: 4, high: MAX - 1, pos: 0},
            ],
            FieldRange::exclude(
                vec![FieldRange { low: 0, high: MAX - 1, pos: 0}],
                vec![
                    FieldRange { low: 0, high: 0,        pos: 0},
                    FieldRange { low: 3, high: 3,        pos: 0}
                ]
            ),
            "1,4"
        );
        assert_eq!(
            vec![
                FieldRange { low: 2, high: 2,            pos: 0},
            ],
            FieldRange::exclude(
                vec![FieldRange { low: 0, high: MAX - 1, pos: 0}],
                vec![
                    FieldRange { low: 0, high: 1,              pos: 0},
                    FieldRange { low: 3, high: usize::MAX - 1, pos: 1}
                ]
            ),
            "1,2,4-"
        );
        assert_eq!(
            vec![
                FieldRange { low: 0, high: 0,              pos: 0},
            ],
            FieldRange::exclude(
                vec![FieldRange { low: 0, high: MAX - 1, pos: 0}],
                vec![
                    FieldRange { low: 1, high: 2,       pos: 0},
                    FieldRange { low: 3, high: MAX - 1, pos: 1}
                ]
            ),
            "2,3,4-"
        );
        assert_eq!(
            vec![
                FieldRange { low: 1, high: 2,       pos: 0},
            ],
            FieldRange::exclude(
                vec![FieldRange { low: 0, high: MAX - 1, pos: 0}],
                vec![
                    FieldRange { low: 0, high: 0,       pos: 0},
                    FieldRange { low: 3, high: MAX - 1, pos: 1}
                ]
            ),
            "1,4-,5-8"
        );
        assert_eq!(
            vec![
                FieldRange { low: 1, high: 2,       pos: 0},
            ],
            FieldRange::exclude(
                vec![FieldRange { low: 0, high: MAX - 1, pos: 0}],
                vec![
                    FieldRange { low: 0, high: 0,       pos: 1},
                    FieldRange { low: 3, high: MAX - 1, pos: 0},
                    FieldRange { low: 4, high: 7,       pos: 2}
                ]
            ),
            "4-,1,5-8"
        );
        assert_eq!(
            vec![
                FieldRange { low: 4, high: MAX - 1, pos: 0},
            ],
            FieldRange::exclude(
                vec![FieldRange { low: 0, high: MAX - 1, pos: 0}],
                vec![
                    FieldRange { low: 0, high: 3,       pos: 0}
                ]
            ),
            "-4"
        );
        assert_eq!(
            vec![
                FieldRange { low: 8, high: MAX - 1, pos: 0},
            ],
            FieldRange::exclude(
                vec![FieldRange { low: 0, high: MAX - 1, pos: 0}],
                vec![
                    FieldRange { low: 0, high: 7,       pos: 0}
                ]
            ),
            "-4,5-8"
        );
    }
    #[test]
    #[rustfmt::skip::macros(assert_eq)]
    fn test_exclude_complex() {
        assert_eq!(
            vec![
                FieldRange { low: 1, high: 3, pos: 0},
                FieldRange { low: 7, high: 14, pos: 1},
            ],
            FieldRange::exclude(
                vec![FieldRange { low: 0, high: 3, pos: 0}, FieldRange { low: 7, high: MAX - 1, pos: 1}],
                vec![FieldRange { low: 0, high: 0, pos: 0}, FieldRange { low: 15, high: MAX - 1, pos: 0}]
            ),
            "-f1-4,8- : -e1,16-"
        );
        let empty: Vec<FieldRange> = vec![];
        assert_eq!(
            empty,
            FieldRange::exclude(
                vec![FieldRange { low: 0, high: MAX-1, pos: 0}],
                vec![FieldRange { low: 0, high: MAX-1, pos: 0}]
            ),
            "-f1- : -e1-"
        );
        assert_eq!(
            vec![
                FieldRange { low: 0, high: 0, pos: 0},
                FieldRange { low: 9, high: 9, pos: 3},
            ],
            FieldRange::exclude(
                vec![FieldRange { low: 0, high: 0, pos: 0}, FieldRange { low: 3, high: 3, pos: 1 }, FieldRange { low: 7, high: 7, pos: 2}, FieldRange { low: 9, high: 9, pos: 3}],
                vec![FieldRange { low: 3, high: 7, pos: 0}]
            ),
            "-f1,4,8,10 : -e4-8"
        );
        // Fields: XXXXXXXXXXX
        // Exclud:      XXXXXXXXX
        assert_eq!(
            vec![
                FieldRange { low: 0, high: 3, pos: 0},
            ],
            FieldRange::exclude(
                vec![FieldRange { low: 0, high: 9, pos: 0}],
                vec![FieldRange { low: 4, high: MAX - 1, pos: 0}]
            ),
            "-f1-10 : -e5-"
        );
        // Fields:      XXXXXXXXXXXXX
        // Exclud:  XXXXXXXX
        assert_eq!(
            vec![
                FieldRange { low: 15, high: 19, pos: 0},
            ],
            FieldRange::exclude(
                vec![FieldRange { low: 9, high: 19, pos: 0}],
                vec![FieldRange { low: 4, high: 14, pos: 0}]
            ),
            "-f10-20 : -e5-15"
        );
        // Fields: XXXXXXXXXXXXXX
        // Exclud:    XXXXXXX
        assert_eq!(
            vec![
                FieldRange { low: 9, high: 11, pos: 0},
                FieldRange { low: 16, high: 19, pos: 0},
            ],
            FieldRange::exclude(
                vec![FieldRange { low: 9, high: 19, pos: 0}],
                vec![FieldRange { low: 12, high: 15, pos: 0}]
            ),
            "-f10-20 : -e13-16"
        );
        // Fields:      XXX
        // Exclud:    XXXXXXX
        assert_eq!(
            empty,
            FieldRange::exclude(
                vec![FieldRange { low: 12, high: 15, pos: 0}],
                vec![FieldRange { low: 9, high: 19, pos: 0}]
            ),
            "-f13-16 : -e10-20"
        );
        // Fields: XXXXXXXX      XXXXX
        // Exclud:     XXXXXXXXXXXXX
        assert_eq!(
            vec![FieldRange { low: 4, high: 8, pos: 0 }, FieldRange { low: 25, high: 29, pos: 1}],
            FieldRange::exclude(
                vec![FieldRange { low: 4, high: 15, pos: 0}, FieldRange { low: 19, high: 29, pos: 1}],
                vec![FieldRange { low: 9, high: 24, pos: 0}]
            ),
            "-f5-16,20-30 : -e10-25"
        );
    }
}
