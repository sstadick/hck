//! Process a [`chunk_reader::Chunk`]

use crate::field_range::FieldRange;
use bstr::ByteSlice;
use object_pool::Reusable;
use ouroboros::self_referencing;
use regex::Regex;

use super::Chunk;

type ProcessedLine<'chunk> = Vec<Vec<&'chunk str>>;
/// Hold onto the underyling chunk and the processed lines that rely on it.
#[self_referencing]
pub struct ProcessedChunk<'pool> {
    chunk: Box<Reusable<'pool, Chunk>>,
    #[borrows(chunk)]
    #[covariant]
    lines: Vec<ProcessedLine<'this>>,
}

impl<'pool> ProcessedChunk<'pool> {
    pub fn lines(&self) -> &Vec<ProcessedLine<'_>> {
        self.borrow_lines()
    }
}

/// Process a chunk of lines and produce a [`ProcessedChunk`] that bundles the chunk with
/// a [`Vec<ProcessedLine<'chunk>>`] that hold the extracted columns.
pub struct ChunkProcessor<'chunk> {
    line_processor: LineProcessor<'chunk>,
}

impl<'chunk> ChunkProcessor<'chunk> {
    /// Create a new [`ChunkProcessor`] based on a [`LineProcessor`].
    pub fn new(line_processor: LineProcessor<'chunk>) -> Self {
        Self { line_processor }
    }

    /// Process a chunk.
    #[inline]
    pub fn process<'pool>(&self, chunk: Reusable<'pool, Chunk>) -> ProcessedChunk<'pool> {
        // A ProcessedChunk is self referential, which is why we jump throught the ororobors hoops
        // to allow it store lines
        ProcessedChunk::new(Box::new(chunk), |c: &Reusable<Chunk>| {
            c.lines()
                .map(|line| self.line_processor.process(line))
                .collect()
        })
    }
}

/// Process a given line, splitting it on the delimiter and storing the columns
/// based on the [`FieldRange`] `pos` fields.
pub struct LineProcessor<'a> {
    /// The delimiter to split the line on
    delimiter: &'a Regex,
    /// The fields to parse out of the line
    fields: &'a [FieldRange],
}

impl<'a> LineProcessor<'a> {
    /// Create a new line processor
    pub fn new(delimiter: &'a Regex, fields: &'a [FieldRange]) -> Self {
        Self { delimiter, fields }
    }

    /// Process a line into its component parts
    // TODO: Maybe throw some rayon at this
    // TODO: Maybe figure out how to reuse the `cache` lines? or allocate them better?
    #[inline]
    pub fn process<'chunk>(&self, line: &'chunk [u8]) -> ProcessedLine<'chunk> {
        let mut cache = vec![vec![]; self.fields.len()];
        let mut parts = self
            .delimiter
            .split(unsafe { line.to_str_unchecked() })
            .peekable();
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
                        if let Some(staged_range) = cache.get_mut(pos) {
                            staged_range.push(part)
                        }
                    }
                    None => break,
                }
                iterator_index += 1;
            }
        }
        cache
    }
}
