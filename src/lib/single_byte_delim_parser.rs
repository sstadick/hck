//! [`SingleByteDelimParser`] is a fast mode parser that is to be used when the
//! field separator character is a single byte. It works by using `memchr2` to
//! first look for both the line terminator and the separator in a single pass.
//! Once the furthest right field has been parsed it switches to searching for
//! just newlines.
use std::{
    cmp::min,
    io::{self, Write},
};

use ripline::LineTerminator;

use crate::{core::JoinAppend, field_range::FieldRange};

/// A `SingleByteDelimParser` is a fast parser of fields from from a buffer.
pub struct SingleByteDelimParser<'a> {
    /// newline aligned buffer, must end in newline
    line_terminator: LineTerminator,
    output_delimiter: &'a [u8],
    fields: &'a [FieldRange],
    sep: u8,
    /// The furthers right field
    max_field: usize,
    /// Current offset into the buffer
    offset: usize,
    newline: u8,
    line: Vec<(usize, usize)>,
}

impl<'a> SingleByteDelimParser<'a> {
    /// Create a [`SingleByteDelimParser`] to process buffers using the input configuration.
    pub fn new(
        line_terminator: LineTerminator,
        output_delimiter: &'a [u8],
        fields: &'a [FieldRange],
        sep: u8,
    ) -> Self {
        Self {
            line_terminator,
            output_delimiter,
            fields,
            sep,
            max_field: fields.last().map_or(usize::MAX, |f| f.high + 1),
            offset: 0,
            newline: line_terminator.as_byte(),
            line: vec![],
        }
    }

    /// Clear all fields of the [`SingleByteDelimParser`].
    #[inline]
    pub fn reset(&mut self) {
        self.offset = 0;
    }

    /// Parse fields from the lines found in buffer and write them to `output`.
    ///
    /// **Note** The input buffer _must_ end with a newline.
    #[inline]
    pub fn process_buffer<W: Write>(
        &mut self,
        buffer: &[u8],
        mut output: W,
    ) -> Result<(), io::Error> {
        // Advance pasts first newline
        if let Some(byte) = buffer.first()
            && *byte == self.newline
        {
            output.join_append(
                self.output_delimiter,
                std::iter::empty(),
                &self.line_terminator,
            )?;
            self.offset += 1;
        }

        while self.offset < buffer.len() {
            self.fill_line(buffer)?;
            let items = self.fields.iter().flat_map(|f| {
                let slice = self
                    .line
                    .get(f.low..=min(f.high, self.line.len().saturating_sub(1)))
                    .unwrap_or(&[]);
                slice.iter().map(|(start, stop)| &buffer[*start..=*stop])
            });
            output.join_append(self.output_delimiter, items, &self.line_terminator)?;
            self.line.clear();
        }
        Ok(())
    }

    /// Fill `line` with the start/end positions of found columns
    /// The positions are relative to the held buffer
    #[inline]
    fn fill_line(&mut self, buffer: &[u8]) -> Result<(), io::Error> {
        let mut field_count = 0;
        let iter = memchr::memchr2_iter(self.sep, self.newline, &buffer[self.offset..]);

        let mut line_offset = 0;
        let mut found_newline = false;

        for index in iter {
            if buffer[self.offset + index] == self.sep {
                field_count += 1;
            } else {
                found_newline = true;
            }

            self.line
                .push((self.offset + line_offset, self.offset + index - 1));
            line_offset = index + 1;

            if found_newline || field_count == self.max_field {
                break;
            }
        }

        if !found_newline {
            let end = memchr::memchr(self.newline, &buffer[self.offset + line_offset..])
                .ok_or(io::ErrorKind::InvalidData)?;
            self.offset += line_offset + end + 1;
        } else {
            self.offset += line_offset;
        }
        Ok(())
    }
}
