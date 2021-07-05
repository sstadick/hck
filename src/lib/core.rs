//! Core processing module
//!
//! It causes me great pain that I can't figure out how split these methods up. The fact that we are relying on
//! lifetime coersion to reuse the `shuffler` vector really locks down the possible options.
//!
//! If we go with a dyn trait on the line splitter function it is appreciably slower.
use crate::{field_range::FieldRange, line_parser::LineParser, mmap::MmapChoice};
use bstr::ByteSlice;
use memchr;
use ripline::{
    line_buffer::{LineBuffer, LineBufferBuilder, LineBufferReader},
    lines::{self, LineIter},
    LineTerminator,
};
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read, Write},
    path::Path,
};

pub enum HckInput<P: AsRef<Path>> {
    Stdin,
    Path(P),
}

impl<P: AsRef<Path>> HckInput<P> {
    pub fn peek_first_line(&self) -> Result<String, io::Error> {
        let mut buffer = String::new();
        match self {
            HckInput::Stdin => {
                io::stdin().read_line(&mut buffer)?;
            }

            HckInput::Path(path) => {
                BufReader::new(File::open(path)?).read_line(&mut buffer)?;
            }
        }
        Ok(buffer)
    }
}

/// The main processing loop
pub struct Core<'a, L> {
    output_delimiter: &'a [u8],
    fields: &'a [FieldRange],
    line_parser: L,
    line_terminator: LineTerminator,
    mmap_choice: MmapChoice,
    line_buffer: LineBuffer,
}

impl<'a, L> Core<'a, L>
where
    L: LineParser<'a>,
{
    pub fn new(
        output_delimiter: &'a [u8],
        fields: &'a [FieldRange],
        line_parser: L,
        line_terminator: LineTerminator,
        mmap_choice: MmapChoice,
    ) -> Self {
        // Avoid allocating a big line buffer if we are likely not going to use it.
        let line_buffer = LineBufferBuilder::new()
            .capacity(if mmap_choice.is_enabled() {
                0
            } else {
                64 * (1 << 10)
            })
            .build();
        Self {
            output_delimiter,
            fields,
            line_parser,
            line_terminator,
            mmap_choice,
            line_buffer,
        }
    }

    pub fn hck_input<P, W>(
        &mut self,
        input: HckInput<P>,
        mut output: W,
        header: Option<String>,
    ) -> Result<(), io::Error>
    where
        P: AsRef<Path>,
        W: Write,
    {
        if let Some(header) = header {
            self.hck_bytes(header.as_bytes(), &mut output)?;
        }
        match input {
            HckInput::Stdin => {
                let reader = io::stdin();
                self.hck_reader(reader, &mut output)
            }
            HckInput::Path(path) => {
                let file = File::open(&path)?;
                if let Some(mmap) = self.mmap_choice.open(&file, Some(&path)) {
                    self.hck_bytes(mmap.as_bytes(), &mut output)
                } else {
                    self.hck_reader(file, &mut output)
                }
            }
        }
    }

    pub fn hck_bytes<W>(&mut self, bytes: &[u8], mut output: W) -> Result<(), io::Error>
    where
        W: Write,
    {
        // let bytes = unsafe { Mmap::map(file).unwrap() };
        let iter = LineIter::new(self.line_terminator.as_byte(), bytes.as_bytes());
        let mut shuffler: Vec<Vec<&'static [u8]>> = vec![vec![]; self.fields.len()];
        for line in iter {
            let mut s: Vec<Vec<&[u8]>> = shuffler;
            self.line_parser.parse_line(
                lines::without_terminator(&line, self.line_terminator),
                &mut s,
            );
            let items = s.iter_mut().flat_map(|s| s.drain(..));
            output.join_append(self.output_delimiter, items, &self.line_terminator)?;
            shuffler = unsafe { core::mem::transmute(s) };
        }
        Ok(())
    }

    // pub fn hck_bytes_fast<W>(&mut self, bytes: &[u8], &output: W) -> Result<(), io::Error> {
    //     // find all occurances of delim and newline
    //     // assert each are only one byte
    //     let mut iter = memchr::memchr2_iter(
    //         self.output_delimiter[0],
    //         self.line_terminator.as_byte(),
    //         bytes,
    //     );
    //     for i in iter {
    //         let b = bytes[i];
    //         if b == self.[0] {
    //             todo!()
    //         } else if b == self.line_terminator.as_byte() {
    //             todo!()
    //         }
    //     }
    //     Ok(())
    // }

    /// Process the bytes from a reader line by line
    pub fn hck_reader<R: Read, W: Write>(
        &mut self,
        reader: R,
        mut output: W,
    ) -> Result<(), io::Error> {
        // let mut lb = self.line_buffer.borrow_mut();
        let mut reader = LineBufferReader::new(reader, &mut self.line_buffer);
        // let terminator = LineTerminator::byte(b'\n');
        // let mut line_buffer = LineBufferBuilder::new().build();
        // let mut line_buffer_reader = LineBufferReader::new(reader, &mut line_buffer);
        let mut shuffler: Vec<Vec<&'static [u8]>> = vec![vec![]; self.fields.len()];
        while reader.fill().unwrap() {
            let iter = LineIter::new(self.line_terminator.as_byte(), reader.buffer());
            {
                for line in iter {
                    let mut s: Vec<Vec<&[u8]>> = shuffler;
                    self.line_parser.parse_line(
                        lines::without_terminator(&line, self.line_terminator),
                        &mut s,
                    );

                    let items = s.iter_mut().flat_map(|s| s.drain(..));
                    output.join_append(self.output_delimiter, items, &self.line_terminator)?;
                    shuffler = unsafe { core::mem::transmute(s) };
                }
            }
            reader.consume(reader.buffer().len());
        }
        Ok(())
    }
}

trait JoinAppend {
    fn join_append<'b>(
        &mut self,
        sep: &[u8],
        items: impl Iterator<Item = &'b [u8]>,
        term: &LineTerminator,
    ) -> Result<(), io::Error>;
}

impl<W: Write> JoinAppend for W {
    #[inline(always)]
    fn join_append<'b>(
        &mut self,
        sep: &[u8],
        mut items: impl Iterator<Item = &'b [u8]>,
        term: &LineTerminator,
    ) -> Result<(), io::Error> {
        if let Some(item) = items.next() {
            self.write_all(item)?;
        }

        for item in items {
            self.write_all(sep)?;
            self.write_all(item)?;
        }
        self.write_all(term.as_bytes())?;
        Ok(())
    }
}
