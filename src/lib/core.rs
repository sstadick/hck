//! Core processing module
//!
//! It causes me great pain that I can't figure out how split these methods up. The fact that we are relying on
//! lifetime coersion to reuse the `shuffler` vector really locks down the possible options.
//!
//! If we go with a dyn trait on the line splitter function it is appreciably slower.
use crate::{
    field_range::{FieldRange, RegexOrStr},
    line_parser::LineParser,
    mmap::MmapChoice,
};
use anyhow::Result;
use bstr::ByteSlice;
use flate2::read::GzDecoder;
use grep_cli::DecompressionReaderBuilder;
use memchr;
use regex::bytes::Regex;
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

/// The config object for [`Core`].
#[derive(Debug, Clone)]
pub struct CoreConfig<'a> {
    delimiter: &'a [u8],
    output_delimiter: &'a [u8],
    line_terminator: LineTerminator,
    mmap_choice: MmapChoice,
    is_parser_regex: bool,
    try_decompress: bool,
    raw_fields: Option<&'a str>,
    raw_header_fields: Option<&'a [Regex]>,
    raw_exclude: Option<&'a str>,
    raw_exclude_headers: Option<&'a [Regex]>,
    header_is_regex: bool,
    parsed_delim: RegexOrStr<'a>,
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
            raw_fields: Some("1-"),
            raw_header_fields: None,
            raw_exclude: None,
            raw_exclude_headers: None,
            header_is_regex: false,
            parsed_delim: RegexOrStr::Str(DEFAULT_DELIM.to_str().unwrap()),
        }
    }
}

impl<'a> CoreConfig<'a> {
    /// Get the parsed delimiter
    pub fn parsed_delim(&self) -> &RegexOrStr<'a> {
        &self.parsed_delim
    }

    /// Read the first line of an input and return it.
    ///
    /// It's up to the user to make sure that any consumed bytes are properly handed
    /// off to the line parsers later on.
    pub fn peek_first_line<P: AsRef<Path>>(
        &self,
        input: &HckInput<P>,
    ) -> Result<Vec<u8>, io::Error> {
        let mut buffer = String::new();
        match input {
            HckInput::Stdin => {
                // TODO: work out how to decode just a byte slice
                if self.try_decompress {
                    unimplemented!("Header selections not supported when piping gzipped stdin")
                }
                io::stdin().read_line(&mut buffer)?;
            }

            HckInput::Path(path) => {
                if self.try_decompress {
                    let reader: Box<dyn Read> = if path
                        .as_ref()
                        .to_str()
                        .map(|p| p.ends_with(".gz"))
                        .unwrap_or(false)
                    {
                        Box::new(GzDecoder::new(File::open(&path)?))
                    } else {
                        Box::new(
                            DecompressionReaderBuilder::new()
                                // .matcher(matcher)
                                .build(&path)?,
                        )
                    };
                    let mut reader = BufReader::new(reader);
                    reader.read_line(&mut buffer)?;
                } else {
                    BufReader::new(File::open(path)?).read_line(&mut buffer)?;
                }
            }
        }
        Ok(lines::without_terminator(buffer.as_bytes(), self.line_terminator).to_owned())
    }

    /// Parse the raw user input fields and header fields. Returns any header bytes read and the parsed fields
    pub fn parse_fields<P>(&self, input: &HckInput<P>) -> Result<(Option<Vec<u8>>, Vec<FieldRange>)>
    where
        P: AsRef<Path>,
    {
        // Parser the fields in the context of the files being looked at
        let (mut extra, fields) = match (self.raw_fields, self.raw_header_fields) {
            (Some(field_list), Some(header_fields)) => {
                let first_line = self.peek_first_line(&input)?;
                let mut fields = FieldRange::from_list(field_list)?;
                let header_fields = FieldRange::from_header_list(
                    header_fields,
                    first_line.as_bytes(),
                    &self.parsed_delim,
                    self.header_is_regex,
                    false,
                )?;
                fields.extend(header_fields.into_iter());
                FieldRange::post_process_ranges(&mut fields);
                (Some(first_line), fields)
            }
            (Some(field_list), None) => (None, FieldRange::from_list(field_list)?),
            (None, Some(header_fields)) => {
                let first_line = self.peek_first_line(&input)?;
                let fields = FieldRange::from_header_list(
                    header_fields,
                    first_line.as_bytes(),
                    &self.parsed_delim,
                    self.header_is_regex,
                    false,
                )?;
                (Some(first_line), fields)
            }
            (None, None) => (None, FieldRange::from_list("1-")?),
        };

        let fields = match (&self.raw_exclude, &self.raw_exclude_headers) {
            (Some(exclude), Some(exclude_header)) => {
                let exclude = FieldRange::from_list(exclude)?;
                let fields = FieldRange::exclude(fields, exclude);
                let first_line = if let Some(first_line) = extra {
                    first_line
                } else {
                    self.peek_first_line(&input)?
                };
                let exclude_headers = FieldRange::from_header_list(
                    &exclude_header,
                    first_line.as_bytes(),
                    &self.parsed_delim,
                    self.header_is_regex,
                    true,
                )?;
                extra = Some(first_line);
                FieldRange::exclude(fields, exclude_headers)
            }
            (Some(exclude), None) => {
                let exclude = FieldRange::from_list(exclude)?;
                FieldRange::exclude(fields, exclude)
            }
            (None, Some(exclude_header)) => {
                let first_line = if let Some(first_line) = extra {
                    first_line
                } else {
                    self.peek_first_line(&input)?
                };
                let exclude_headers = FieldRange::from_header_list(
                    &exclude_header,
                    first_line.as_bytes(),
                    &self.parsed_delim,
                    self.header_is_regex,
                    true,
                )?;
                extra = Some(first_line);
                FieldRange::exclude(fields, exclude_headers)
            }
            (None, None) => fields,
        };
        Ok((extra, fields))
    }
}

/// A builder for the [`CoreConfig`] which drives [`Core`].
#[derive(Clone, Debug)]
pub struct CoreConfigBuilder<'a> {
    config: CoreConfig<'a>,
}

impl<'a> CoreConfigBuilder<'a> {
    pub fn new() -> Self {
        Self {
            config: CoreConfig::default(),
        }
    }

    pub fn build(mut self) -> Result<CoreConfig<'a>> {
        let delim = if self.config.is_parser_regex {
            RegexOrStr::Regex(Regex::new(self.config.delimiter.to_str()?)?)
        } else {
            RegexOrStr::Str(self.config.delimiter.to_str()?)
        };
        self.config.parsed_delim = delim;
        Ok(self.config)
    }

    /// The substr to split lines on.
    pub fn delimiter(mut self, delim: &'a [u8]) -> Self {
        self.config.delimiter = delim;
        self
    }

    /// The substr to use as the output delimiter
    pub fn output_delimiter(mut self, delim: &'a [u8]) -> Self {
        self.config.output_delimiter = delim;
        self
    }

    /// The line terminator to use when looking for linebreaks and stripping linebreach chars.
    pub fn line_terminator(mut self, term: LineTerminator) -> Self {
        self.config.line_terminator = term;
        self
    }

    /// Whether or not to try to use mmap mode
    pub fn mmap(mut self, mmap_choice: MmapChoice) -> Self {
        self.config.mmap_choice = mmap_choice;
        self
    }

    /// Whether or not the parser is a regex
    #[allow(clippy::wrong_self_convention)]
    pub fn is_regex_parser(mut self, is_regex: bool) -> Self {
        self.config.is_parser_regex = is_regex;
        self
    }

    /// Try to decompress an input file
    pub fn try_decompress(mut self, try_decompress: bool) -> Self {
        self.config.try_decompress = try_decompress;
        self
    }

    /// The raw user input fields to output
    pub fn fields(mut self, fields: Option<&'a str>) -> Self {
        self.config.raw_fields = fields;
        self
    }

    /// The raw user input header to output
    pub fn headers(mut self, headers: Option<&'a [Regex]>) -> Self {
        self.config.raw_header_fields = headers;
        self
    }

    /// The raw user input fields to exclude
    pub fn exclude(mut self, exclude: Option<&'a str>) -> Self {
        self.config.raw_exclude = exclude;
        self
    }

    /// The raw user input headers to exclude
    pub fn exclude_headers(mut self, exclude_headers: Option<&'a [Regex]>) -> Self {
        self.config.raw_exclude_headers = exclude_headers;
        self
    }

    /// Whether or not to treat the headers as regex
    pub fn header_is_regex(mut self, header_is_regex: bool) -> Self {
        self.config.header_is_regex = header_is_regex;
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

    /// Check if no reordering of fields is happening
    #[inline]
    fn are_fields_pos_sorted(&self) -> bool {
        let mut test = 0;
        for field in self.fields {
            if field.pos < test {
                return false;
            }
            test = field.pos
        }
        true
    }

    /// Check if we can run in `fast mode`.
    ///
    /// delimiter is 1 byte, newline is 1 bytes, and we are not using a regex
    fn allow_fastmode(&self) -> bool {
        // false
        self.config.delimiter.len() == 1
            && self.config.line_terminator.as_bytes().len() == 1
            && !self.config.is_parser_regex
            && self.are_fields_pos_sorted()
    }

    pub fn hck_input<P, W>(
        &mut self,
        input: HckInput<P>,
        mut output: W,
        header: Option<Vec<u8>>,
    ) -> Result<(), io::Error>
    where
        P: AsRef<Path>,
        W: Write,
    {
        // Dispatch to a given `hck_*` runner depending on configuration
        match input {
            HckInput::Stdin => {
                if let Some(header) = header {
                    self.hck_bytes(header.as_bytes(), &mut output)?;
                }
                let reader: Box<dyn Read> = if self.config.try_decompress {
                    Box::new(GzDecoder::new(io::stdin()))
                } else {
                    Box::new(io::stdin())
                };
                if self.allow_fastmode() {
                    self.hck_reader_fast(reader, &mut output)
                } else {
                    self.hck_reader(reader, &mut output)
                }
            }
            HckInput::Path(path) => {
                if self.config.try_decompress {
                    let reader: Box<dyn Read> = if path
                        .as_ref()
                        .to_str()
                        .map(|p| p.ends_with(".gz"))
                        .unwrap_or(false)
                    {
                        Box::new(GzDecoder::new(File::open(&path)?))
                    } else {
                        Box::new(
                            DecompressionReaderBuilder::new()
                                // .matcher(matcher)
                                .build(&path)?,
                        )
                    };
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
        let mut shuffler: Vec<Vec<&'static [u8]>> =
            vec![vec![]; self.fields.iter().map(|f| f.pos).max().unwrap() + 1];
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
                line.push((start, index - 1));
                let items = self.fields.iter().flat_map(|f| {
                    let slice = line
                        .get(f.low..=min(f.high, line.len().saturating_sub(1)))
                        .unwrap_or(&[]);
                    slice.iter().map(|(start, stop)| &bytes[*start..=*stop])
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
                    line.push((start, index - 1));
                    let items = self.fields.iter().flat_map(|f| {
                        let slice = line
                            .get(f.low..=min(f.high, line.len().saturating_sub(1)))
                            .unwrap_or(&[]);
                        slice.iter().map(|(start, stop)| &bytes[*start..=*stop])
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
        let mut shuffler: Vec<Vec<&'static [u8]>> =
            vec![vec![]; self.fields.iter().map(|f| f.pos).max().unwrap() + 1];
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
