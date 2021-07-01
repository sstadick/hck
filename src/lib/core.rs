//! Core processing module
//!
//! It causes me great pain that I can't figure out how split these methods up. The fact that we are relying on
//! lifetime coersion to reuse the `shuffler` vector really locks down the possible options.
//!
//! If we go with a dyn trait on the line splitter function it is appreciably slower.
use crate::field_range::FieldRange;
use bstr::ByteSlice;
use regex::bytes::Regex;
use std::io::{self, BufRead, BufReader, Read, Write};

/// The main processing loop
pub struct Core<'a, W>
where
    W: Write,
{
    writer: &'a mut W,
    output_delimiter: &'a [u8],
    fields: &'a [FieldRange],
}

impl<'a, W> Core<'a, W>
where
    W: Write,
{
    pub fn new(writer: &'a mut W, output_delimiter: &'a [u8], fields: &'a [FieldRange]) -> Self {
        Self {
            writer,
            output_delimiter,
            fields,
        }
    }

    /// Write a lines worth of items, properly delimited and with a newline.
    #[inline]
    fn join_appender<'b>(
        &mut self,
        mut items: impl Iterator<Item = &'b [u8]>,
    ) -> Result<(), io::Error> {
        if let Some(item) = items.next() {
            self.writer.write_all(item)?;
        }

        for item in items {
            self.writer.write_all(self.output_delimiter)?;
            self.writer.write_all(item)?;
        }
        self.writer.write_all(&[b'\n'])?;
        Ok(())
    }

    /// Process a reader using regex byte splitting
    pub fn process_reader_regex<R>(
        &mut self,
        reader: &mut BufReader<R>,
        regex: &Regex,
    ) -> Result<(), io::Error>
    where
        R: Read,
    {
        let mut empty_shuffler: Vec<Vec<&'static [u8]>> = vec![vec![]; self.fields.len()];

        let mut bytes = vec![];
        let mut consumed = 0;

        loop {
            // Process the buffer
            {
                let mut buf = reader.fill_buf()?;
                while let Some(index) = buf.find_byte(b'\n') {
                    let mut shuffler = empty_shuffler;
                    let (record, rest) = buf.split_at(index + 1);
                    buf = rest;
                    consumed += record.len();
                    let mut parts = regex.split(&record[..record.len() - 1]).peekable();
                    let mut iterator_index = 0;

                    // Iterate over our ranges and write any fields that are contained by them.
                    for &FieldRange { low, high, pos } in self.fields {
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
                    // Now write the values in the correct order
                    self.join_appender(shuffler.iter_mut().flat_map(|values| values.drain(..)))?;
                    empty_shuffler = unsafe { core::mem::transmute(shuffler) };
                }
                bytes.extend_from_slice(&buf);
                consumed += buf.len();
            }

            // Get next buffer
            let mut shuffler = empty_shuffler;
            reader.consume(consumed);
            consumed = 0;
            reader.read_until(b'\n', &mut bytes)?;
            if bytes.is_empty() {
                break;
            }
            // Do stuff with record - new scope so that parts are dropped before bytes are cleared
            {
                let mut parts = regex.split(&bytes[..bytes.len() - 1]).peekable();
                let mut iterator_index = 0;

                // Iterate over our ranges and write any fields that are contained by them.
                for &FieldRange { low, high, pos } in self.fields {
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
                // Now write the values in the correct order
                self.join_appender(shuffler.iter_mut().flat_map(|values| values.drain(..)))?;
            }
            empty_shuffler = unsafe { core::mem::transmute(shuffler) };
            bytes.clear();
        }
        reader.consume(consumed);
        // TODO: Could maybe defer this?
        self.writer.flush()?;
        Ok(())
    }

    /// Process a reader using substr splitting.
    pub fn process_reader_substr<R>(
        &mut self,
        reader: &mut BufReader<R>,
        delimiter: &[u8],
    ) -> Result<(), io::Error>
    where
        R: Read,
    {
        let mut empty_shuffler: Vec<Vec<&'static [u8]>> = vec![vec![]; self.fields.len()];

        let mut bytes = vec![];
        let mut consumed = 0;

        loop {
            // Process the buffer
            {
                let mut buf = reader.fill_buf()?;
                while let Some(index) = buf.find_byte(b'\n') {
                    let mut shuffler = empty_shuffler;
                    let (record, rest) = buf.split_at(index + 1);
                    buf = rest;
                    consumed += record.len();
                    let mut parts = record[..record.len() - 1].split_str(delimiter).peekable();
                    let mut iterator_index = 0;

                    // Iterate over our ranges and write any fields that are contained by them.
                    for &FieldRange { low, high, pos } in self.fields {
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
                    // Now write the values in the correct order
                    self.join_appender(shuffler.iter_mut().flat_map(|values| values.drain(..)))?;
                    empty_shuffler = unsafe { core::mem::transmute(shuffler) };
                }
                bytes.extend_from_slice(&buf);
                consumed += buf.len();
            }

            // Get next buffer
            let mut shuffler = empty_shuffler;
            reader.consume(consumed);
            consumed = 0;
            reader.read_until(b'\n', &mut bytes)?;
            if bytes.is_empty() {
                break;
            }
            // Do stuff with record - new scope so that parts are dropped before bytes are cleared
            {
                let mut parts = bytes[..bytes.len() - 1].split_str(delimiter).peekable();
                let mut iterator_index = 0;

                // Iterate over our ranges and write any fields that are contained by them.
                for &FieldRange { low, high, pos } in self.fields {
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
                // Now write the values in the correct order
                self.join_appender(shuffler.iter_mut().flat_map(|values| values.drain(..)))?;
            }
            empty_shuffler = unsafe { core::mem::transmute(shuffler) };
            bytes.clear();
        }
        reader.consume(consumed);
        // TODO: Could maybe defer this?
        self.writer.flush()?;
        Ok(())
    }
}
