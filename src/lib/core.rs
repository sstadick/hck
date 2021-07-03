//! Core processing module
//!
//! It causes me great pain that I can't figure out how split these methods up. The fact that we are relying on
//! lifetime coersion to reuse the `shuffler` vector really locks down the possible options.
//!
//! If we go with a dyn trait on the line splitter function it is appreciably slower.
use crate::{
    field_range::FieldRange,
    line_parser::{LineParser, SubStrLineParser},
};
use bstr::ByteSlice;
use memmap::Mmap;
use regex::bytes::Regex;
use ripline::{
    line_buffer::{LineBufferBuilder, LineBufferReader},
    lines::{self, LineIter},
    LineTerminator,
};
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read, Write},
};

/// The main processing loop
pub struct Core<'a, L, W> {
    writer: &'a mut W,
    output_delimiter: &'a [u8],
    fields: &'a [FieldRange],
    line_parser: L,
}

impl<'a, L, W> Core<'a, L, W>
where
    W: Write,
    L: LineParser<'a>,
{
    pub fn new(
        writer: &'a mut W,
        output_delimiter: &'a [u8],
        fields: &'a [FieldRange],
        line_parser: L,
    ) -> Self {
        Self {
            writer,
            output_delimiter,
            fields,
            line_parser,
        }
    }

    // Write a lines worth of items, properly delimited and with a newline.
    #[inline]
    fn join_appender<'c>(
        &mut self,
        mut items: impl Iterator<Item = &'c [u8]>,
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

    // Formalize ripline and make it a crate with good docs showing how to use it for both mmap
    // and regular files
    // Need to configure more like ripgrep so that we can optionally work off of a Reader or off of a byte slice only
    pub fn process_reader<R: Read>(&mut self, reader: R, file: &File) -> Result<(), io::Error> {
        let terminator = LineTerminator::byte(b'\n');
        let bytes = unsafe { Mmap::map(file).unwrap() };
        let iter = LineIter::new(terminator.as_byte(), bytes.as_bytes());
        let mut shuffler: Vec<Vec<&'static [u8]>> = vec![vec![]; self.fields.len()];
        for line in iter {
            let mut s: Vec<Vec<&[u8]>> = shuffler;
            self.line_parser
                .parse_line(lines::without_terminator(&line, terminator), &mut s);
            self.join_appender(s.iter_mut().flat_map(|s| s.drain(..)))
                .unwrap();
            shuffler = unsafe { core::mem::transmute(s) };
        }

        // let terminator = LineTerminator::byte(b'\n');
        // let mut line_buffer = LineBufferBuilder::new().build();
        // let mut line_buffer_reader = LineBufferReader::new(reader, &mut line_buffer);
        // let mut shuffler: Vec<Vec<&'static [u8]>> = vec![vec![]; self.fields.len()];
        // while line_buffer_reader.fill().unwrap() {
        //     let iter = LineIter::new(terminator.as_byte(), line_buffer_reader.buffer());
        //     {
        //         for line in iter {
        //             let mut s: Vec<Vec<&[u8]>> = shuffler;
        //             self.line_parser
        //                 .parse_line(lines::without_terminator(&line, terminator), &mut s);
        //             self.join_appender(s.iter_mut().flat_map(|s| s.drain(..)))
        //                 .unwrap();
        //             shuffler = unsafe { core::mem::transmute(s) };
        //         }
        //     }
        //     line_buffer_reader.consume(line_buffer_reader.buffer().len());
        // }

        // let mut reader = BufReader::with_capacity(64 * (1 << 10), reader);
        // let mut shuffler: Vec<Vec<&'static [u8]>> = vec![vec![]; self.fields.len()];
        // let mut line = vec![];
        // while reader.read_until(b'\n', &mut line)? > 0 {
        //     let mut s: Vec<Vec<&[u8]>> = shuffler;
        //     self.line_parser
        //         .parse_line(lines::without_terminator(&line, terminator), &mut s);
        //     self.join_appender(s.iter_mut().flat_map(|s| s.drain(..)))
        //         .unwrap();
        //     shuffler = unsafe { core::mem::transmute(s) };
        //     line.clear();
        // }
        Ok(())
    }
}
