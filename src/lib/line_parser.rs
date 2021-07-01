use std::iter::Peekable;

use crate::field_range::FieldRange;
use bstr::ByteSlice;
use regex::bytes::Regex;

///
pub trait LineParser {
    /// Fills the shuffler with values parsed from the line.
    fn parse_line(&self, line: &[u8], shuffler: &mut Vec<Vec<&[u8]>>) {
        let mut parts = self.split(line);
        let mut iterator_index = 0;

        // Iterate over our ranges and write any fields that are contained by them.
        for &FieldRange { low, high, pos } in self.fields() {
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

    /// Return a slice of [`FieldRange`]'s
    fn fields(&self) -> &[FieldRange];

    /// Return an iterator of elements resulting from splitting the line.
    fn split<'l, I>(&self, line: &[u8]) -> I
    where
        I: Iterator<Item = &'l [u8]>;
}

/// A line parser that works on fixed substrings
pub struct SubStrLineParser<'a> {
    field_ranges: &'a [FieldRange],
    delimiter: &'a [u8],
}

impl<'a> LineParser for SubStrLineParser<'a> {
    /// Get the field ranges associated with this splitter
    #[inline]
    fn fields(&self) -> &[FieldRange] {
        self.field_ranges
    }

    /// Split the line
    #[inline]
    fn split<'l, I>(&self, line: &'l [u8]) -> I
    where
        I: Iterator<Item = &'l [u8]>,
    {
        line.split_str(self.delimiter).peekable()
    }
}

/// A line parser that works on regex's (bytes)
pub struct RegexLineParser<'a> {
    field_ranges: &'a [FieldRange],
    regex: &'a Regex,
}

impl<'a> LineParser for RegexLineParser<'a> {
    /// Get the ranges associated with this splitter
    #[inline]
    fn fields(&self) -> &[FieldRange] {
        self.field_ranges
    }

    /// Split the line
    #[inline]
    fn split<'l, I>(&self, line: &'l [u8]) -> I
    where
        I: Iterator<Item = &'l [u8]>,
    {
        self.regex.split(line).peekable()
    }
}
