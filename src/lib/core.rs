use bstr::ByteSlice;

use crate::field_range::FieldRange;
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

    pub fn process_reader_regex<R>(&mut self, reader: R, regex: &Regex) -> Result<(), io::Error>
    where
        R: Read,
    {
        let mut reader = BufReader::new(reader);
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
}
