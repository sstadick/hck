use crate::field_range::FieldRange;
use bstr::ByteSlice;
use regex::bytes::Regex;

/// Methods for parsing a line into a reordered `shuffler`
pub trait LineParser<'a> {
    /// Fills the shuffler with values parsed from the line.
    fn parse_line<'b>(&self, line: &'b [u8], shuffler: &mut Vec<Vec<&'b [u8]>>)
    where
        'a: 'b;
}

/// A line parser that works on fixed substrings
pub struct SubStrLineParser<'a> {
    field_ranges: &'a [FieldRange],
    delimiter: &'a [u8],
}

impl<'a> SubStrLineParser<'a> {
    pub fn new(field_ranges: &'a [FieldRange], delimiter: &'a [u8]) -> Self {
        Self {
            field_ranges,
            delimiter,
        }
    }
}
impl<'a> LineParser<'a> for SubStrLineParser<'a> {
    #[inline]
    fn parse_line<'b>(&self, line: &'b [u8], shuffler: &mut Vec<Vec<&'b [u8]>>)
    where
        'a: 'b,
    {
        let mut parts = line.split_str(self.delimiter).peekable();
        let mut iterator_index = 0;

        // Iterate over our ranges and write any fields that are contained by them.
        for &FieldRange { low, high, pos } in self.field_ranges {
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
                        // Guaranteed to be in range since staging is created based on field pos anyways
                        if let Some(reshuffled_range) = shuffler.get_mut(pos) {
                            reshuffled_range.push(part)
                        }
                    }
                    None => break,
                }
                iterator_index += 1;
            }
        }
    }
}

/// A line parser that works on fixed substrings
pub struct RegexLineParser<'a> {
    field_ranges: &'a [FieldRange],
    delimiter: &'a Regex,
}

impl<'a> RegexLineParser<'a> {
    pub fn new(field_ranges: &'a [FieldRange], delimiter: &'a Regex) -> Self {
        Self {
            field_ranges,
            delimiter,
        }
    }
}
impl<'a> LineParser<'a> for RegexLineParser<'a> {
    #[inline]
    fn parse_line<'b>(&self, line: &'b [u8], shuffler: &mut Vec<Vec<&'b [u8]>>)
    where
        'a: 'b,
    {
        let mut parts = self.delimiter.split(line).peekable();
        let mut iterator_index = 0;

        // Iterate over our ranges and write any fields that are contained by them.
        for &FieldRange { low, high, pos } in self.field_ranges {
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
                        // Guaranteed to be in range since staging is created based on field pos anyways
                        if let Some(reshuffled_range) = shuffler.get_mut(pos) {
                            reshuffled_range.push(part)
                        } else {
                            unreachable!()
                        }
                    }
                    None => break,
                }
                iterator_index += 1;
            }
        }
    }
}
