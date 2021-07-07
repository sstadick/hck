//! Core processing module
//!
//! It causes me great pain that I can't figure out how split these methods up. The fact that we are relying on
//! lifetime coersion to reuse the `shuffler` vector really locks down the possible options.
//!
//! If we go with a dyn trait on the line splitter function it is appreciably slower.
use crate::{field_range::FieldRange, line_parser::LineParser, mmap::MmapChoice};
use bstr::ByteSlice;
use grep_cli::DecompressionReaderBuilder;
use memchr;
use ripline::{
    line_buffer::{LineBuffer, LineBufferReader},
    lines::{self, LineIter},
    LineTerminator,
};
use std::{
    cmp::min,
    fs::File,
    io::{self, BufRead, BufReader, Read, Write},
    path::Path,
};

const DEFAULT_DELIM: &[u8] = &[b'\t'];

/// The input types that `hck` can parse.
pub enum HckInput<P: AsRef<Path>> {
    Stdin,
    Path(P),
}

impl<P: AsRef<Path>> HckInput<P> {
    /// Read the first line of an input and return it.
    ///
    /// It's up to the user to make sure that any consumed bytes are properly handed
    /// off to the line parsers later on.
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

/// The config object for [`Core`].
#[derive(Debug, Clone, Copy)]
pub struct CoreConfig<'a> {
    delimiter: &'a [u8],
    output_delimiter: &'a [u8],
    line_terminator: LineTerminator,
    mmap_choice: MmapChoice,
    is_parser_regex: bool,
    try_decompress: bool,
}

impl<'a> Default for CoreConfig<'a> {
    fn default() -> Self {
        Self {
            delimiter: DEFAULT_DELIM,
            output_delimiter: DEFAULT_DELIM,
            line_terminator: LineTerminator::default(),
            mmap_choice: unsafe { MmapChoice::auto() },
            is_parser_regex: false,
            try_decompress: false,
        }
    }
}

impl<'a> CoreConfig<'a> {
    #[inline]
    pub fn is_parser_regex(&self) -> bool {
        self.is_parser_regex
    }

    #[inline]
    pub fn delimiter(&self) -> &[u8] {
        self.delimiter
    }
}

/// A builder for the [`CoreConfig`] which drives [`Core`].
#[derive(Copy, Clone, Debug)]
pub struct CoreConfigBuilder<'a> {
    config: CoreConfig<'a>,
}

impl<'a> CoreConfigBuilder<'a> {
    pub fn new() -> Self {
        Self {
            config: CoreConfig::default(),
        }
    }

    pub fn build(self) -> CoreConfig<'a> {
        self.config
    }

    /// The substr to split lines on.
    pub fn delimiter(&mut self, delim: &'a [u8]) -> &mut Self {
        self.config.delimiter = delim;
        self
    }

    /// The substr to use as the output delimiter
    pub fn output_delimiter(&mut self, delim: &'a [u8]) -> &mut Self {
        self.config.output_delimiter = delim;
        self
    }

    /// The line terminator to use when looking for linebreaks and stripping linebreach chars.
    pub fn line_terminator(&mut self, term: LineTerminator) -> &mut Self {
        self.config.line_terminator = term;
        self
    }

    /// Whether or not to try to use mmap mode
    pub fn mmap(&mut self, mmap_choice: MmapChoice) -> &mut Self {
        self.config.mmap_choice = mmap_choice;
        self
    }

    /// Whether or not the parser is a regex
    pub fn is_regex_parser(&mut self, is_regex: bool) -> &mut Self {
        self.config.is_parser_regex = is_regex;
        self
    }

    /// Try to decompress an input file
    pub fn try_decompress(&mut self, try_decompress: bool) -> &mut Self {
        self.config.try_decompress = try_decompress;
        self
    }
}

impl<'a> Default for CoreConfigBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// The main processing loop
pub struct Core<'a, L> {
    /// The [`CoreConfig`] object that determines how [`Core`] is run
    config: &'a CoreConfig<'a>,
    /// The [`FieldRange`]'s to keep, in the order to output them
    fields: &'a [FieldRange],
    /// The reusable line parse that defines how to parse a line (regex or substr).
    line_parser: L,
    /// The reusable line buffer that holds bytes from reads
    line_buffer: &'a mut LineBuffer,
}

impl<'a, L> Core<'a, L>
where
    L: LineParser<'a>,
{
    /// Create a new "core" the can be used to parse multiple inputs
    pub fn new(
        config: &'a CoreConfig,
        fields: &'a [FieldRange],
        line_parser: L,
        line_buffer: &'a mut LineBuffer,
    ) -> Self {
        Self {
            config,
            fields,
            line_parser,
            line_buffer,
        }
    }

    /// Check if we can run in `fast mode`.
    ///
    /// delimiter is 1 byte, newline is 1 bytes, and we are not using a regex
    fn allow_fastmode(&self) -> bool {
        self.config.delimiter.len() == 1
            && self.config.line_terminator.as_bytes().len() == 1
            && !self.config.is_parser_regex
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
        // Dispatch to a given `hck_*` runner depending on configuration
        match input {
            HckInput::Stdin => {
                let reader = io::stdin();
                if self.allow_fastmode() {
                    self.hck_reader_fast(reader, &mut output)
                } else {
                    self.hck_reader(reader, &mut output)
                }
            }
            HckInput::Path(path) => {
                if self.config.try_decompress {
                    let reader = DecompressionReaderBuilder::new().build(&path)?;
                    if self.allow_fastmode() {
                        self.hck_reader_fast(reader, &mut output)
                    } else {
                        self.hck_reader(reader, &mut output)
                    }
                } else {
                    let file = File::open(&path)?;
                    if let Some(mmap) = self.config.mmap_choice.open(&file, Some(&path)) {
                        if self.allow_fastmode() {
                            self.hck_bytes_fast(mmap.as_bytes(), &mut output)
                        } else {
                            self.hck_bytes(mmap.as_bytes(), &mut output)
                        }
                    } else if self.allow_fastmode() {
                        self.hck_reader_fast(file, &mut output)
                    } else {
                        self.hck_reader(file, &mut output)
                    }
                }
            }
        }
    }

    /// Iterate over the lines in a slice of bytes.
    ///
    /// The input slice of bytes is assumed to end in a newline.
    pub fn hck_bytes<W>(&mut self, bytes: &[u8], mut output: W) -> Result<(), io::Error>
    where
        W: Write,
    {
        let iter = LineIter::new(self.config.line_terminator.as_byte(), bytes.as_bytes());
        let mut shuffler: Vec<Vec<&'static [u8]>> = vec![vec![]; self.fields.len()];
        for line in iter {
            let mut s: Vec<Vec<&[u8]>> = shuffler;
            self.line_parser.parse_line(
                lines::without_terminator(&line, self.config.line_terminator),
                &mut s,
            );
            let items = s.iter_mut().flat_map(|s| s.drain(..));
            output.join_append(
                self.config.output_delimiter,
                items,
                &self.config.line_terminator,
            )?;
            shuffler = unsafe { core::mem::transmute(s) };
        }
        Ok(())
    }

    /// Fast mode iteration over lines in a slice of bytes.
    ///
    /// This expects the seperator to be a single byte and the newline to be a singel byte.
    ///
    /// Instead of  seaching for linebreaks, then splitting up the line on the `sep`,
    /// fast mode looks for either `sep` or `newline` at the same time, so instead of two passes
    /// over the bytes we only make one pass.
    pub fn hck_bytes_fast<W: Write>(
        &mut self,
        bytes: &[u8],
        mut output: W,
    ) -> Result<(), io::Error> {
        let sep = self.config.delimiter[0];
        let newline = self.config.line_terminator.as_byte();

        let iter = memchr::memchr2_iter(sep, newline, bytes);

        let mut line = vec![];
        let mut start = 0;

        for index in iter {
            if bytes[index] == sep {
                line.push((start, index - 1));
                start = index + 1;
            } else if bytes[index] == newline {
                let items = self.fields.iter().flat_map(|f| {
                    line[f.low..=min(f.high, line.len() - 1)]
                        .iter()
                        .map(|(start, stop)| &bytes[*start..=*stop])
                });
                output.join_append(
                    self.config.output_delimiter,
                    items,
                    &self.config.line_terminator,
                )?;
                start = index + 1;
                line.clear();
            } else {
                unreachable!()
            }
        }
        Ok(())
    }

    /// Fast mode iteration over lines in a reader.
    ///
    /// This expects the seperator to be a single byte and the newline to be a singel byte.
    ///
    /// Instead of  seaching for linebreaks, then splitting up the line on the `sep`,
    /// fast mode looks for either `sep` or `newline` at the same time, so instead of two passes
    /// over the bytes we only make one pass.
    pub fn hck_reader_fast<R: Read, W: Write>(
        &mut self,
        reader: R,
        mut output: W,
    ) -> Result<(), io::Error> {
        let sep = self.config.delimiter[0];
        let newline = self.config.line_terminator.as_byte();

        let mut reader = LineBufferReader::new(reader, &mut self.line_buffer);
        let mut line = vec![];
        while reader.fill().unwrap() {
            let bytes = reader.buffer();
            let iter = memchr::memchr2_iter(sep, newline, bytes);
            let mut start = 0;

            for index in iter {
                if bytes[index] == sep {
                    line.push((start, index - 1));
                    start = index + 1;
                } else if bytes[index] == newline {
                    let items = self.fields.iter().flat_map(|f| {
                        line[f.low..=min(f.high, line.len() - 1)]
                            .iter()
                            .map(|(start, stop)| &bytes[*start..=*stop])
                    });
                    output.join_append(
                        self.config.output_delimiter,
                        items,
                        &self.config.line_terminator,
                    )?;
                    start = index + 1;
                    line.clear();
                } else {
                    unreachable!()
                }
            }

            reader.consume(reader.buffer().len());
        }
        Ok(())
    }

    /// Process lines from a reader.
    pub fn hck_reader<R: Read, W: Write>(
        &mut self,
        reader: R,
        mut output: W,
    ) -> Result<(), io::Error> {
        let mut reader = LineBufferReader::new(reader, &mut self.line_buffer);
        let mut shuffler: Vec<Vec<&'static [u8]>> = vec![vec![]; self.fields.len()];
        while reader.fill().unwrap() {
            let iter = LineIter::new(self.config.line_terminator.as_byte(), reader.buffer());

            for line in iter {
                let mut s: Vec<Vec<&[u8]>> = shuffler;
                self.line_parser.parse_line(
                    lines::without_terminator(&line, self.config.line_terminator),
                    &mut s,
                );

                let items = s.iter_mut().flat_map(|s| s.drain(..));
                output.join_append(
                    self.config.output_delimiter,
                    items,
                    &self.config.line_terminator,
                )?;
                shuffler = unsafe { core::mem::transmute(s) };
            }
            reader.consume(reader.buffer().len());
        }
        Ok(())
    }
}

/// A trait for adding `join_append` to a writer.
trait JoinAppend {
    /// Given an input iterator of items, write them with a serparator and a newline.
    fn join_append<'b>(
        &mut self,
        sep: &[u8],
        items: impl Iterator<Item = &'b [u8]>,
        term: &LineTerminator,
    ) -> Result<(), io::Error>;
}

/// [`JoinAppend`] for [`Write`].
impl<W: Write> JoinAppend for W {
    /// Given an input iterator of items, write them with a serparator and a newline.
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
