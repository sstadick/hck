use std::{
    cmp::min,
    io::{self, Write},
};

use ripline::LineTerminator;

use crate::{core::JoinAppend, field_range::FieldRange};

pub struct BufferParser<'a> {
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
}

impl<'a> BufferParser<'a> {
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
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.offset = 0;
    }

    #[inline]
    pub fn process_buffer<W: Write>(
        &mut self,
        buffer: &[u8],
        mut output: W,
    ) -> Result<(), io::Error> {
        let mut line = vec![];
        while self.offset < buffer.len() {
            self.fill_line(&mut line, buffer);
            let items = self.fields.iter().flat_map(|f| {
                let slice = line
                    .get(f.low..=min(f.high, line.len().saturating_sub(1)))
                    .unwrap_or(&[]);
                slice.iter().map(|(start, stop)| &buffer[*start..=*stop])
            });
            output.join_append(self.output_delimiter, items, &self.line_terminator)?;
            line.clear();
        }
        Ok(())
    }

    /// Fill `line` with the start/end positions of found columns
    /// The positions are relative to the held buffer
    #[inline]
    fn fill_line(&mut self, line: &mut Vec<(usize, usize)>, buffer: &[u8]) {
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

            line.push((self.offset + line_offset, self.offset + index - 1));
            line_offset = index + 1;

            if found_newline || field_count == self.max_field {
                break;
            }
        }

        if !found_newline {
            let end = memchr::memchr(self.newline, &buffer[self.offset + line_offset..])
                .expect("Can't find newline");
            self.offset += line_offset + end + 1;
        } else {
            self.offset += line_offset;
        }
    }
}
