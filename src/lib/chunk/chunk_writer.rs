//! Write a processed chunk of lines to a writer. (raw, ideally)

use super::{chunk_processor::ProcessedChunk, Chunk, BUF_SIZE};
use std::io::{self, Write};

pub struct ChunkWriter<'a, W>
where
    W: Write,
{
    writer: W,
    out_delim: &'a str,
    buffer: Vec<u8>,
}

impl<'a, W> ChunkWriter<'a, W>
where
    W: Write,
{
    /// Take an ideally raw writer.
    pub fn new(writer: W, out_delim: &'a str) -> Self {
        Self::with_capacity(writer, out_delim, BUF_SIZE)
    }

    pub fn with_capacity(writer: W, out_delim: &'a str, cap: usize) -> Self {
        Self {
            writer,
            out_delim,
            buffer: Vec::with_capacity(cap),
        }
    }

    #[inline]
    fn join_appender<'chunk>(&mut self, mut items: impl Iterator<Item = &'chunk str>) {
        if let Some(item) = items.next() {
            self.buffer.extend_from_slice(item.as_bytes())
        }
        for item in items {
            self.buffer.extend_from_slice(self.out_delim.as_bytes());
            self.buffer.extend_from_slice(item.as_bytes());
        }
        self.buffer.extend_from_slice(&[b'\n']);
    }

    pub fn write_chunk(&mut self, chunk: ProcessedChunk) -> Result<(), io::Error> {
        for line in chunk.lines() {
            // TODO: make sure this ins't doing something dumb like clone
            self.join_appender(line.iter().flat_map(|values| values.iter().copied()));
        }
        self.writer.write_all(&self.buffer)?;
        self.buffer.clear();
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), io::Error> {
        self.writer.flush()
    }
}
