//! Read chunks of lines at a time.
use super::chunk::Chunk;
use super::BUF_SIZE;
use linereader::LineReader;
use object_pool::{Pool, Reusable};
use std::io::{self, Read};

/// A reader that returns pooled buffers of lines that are guarenteed to end with a newline
pub struct ChunkReader<R: Read> {
    buf_size: usize,
    reader: LineReader<R>,
    pool: Pool<Chunk>,
}

impl<R: Read> ChunkReader<R> {
    /// Create a new [`ChunkReader`].
    pub fn new(reader: R, pool: Pool<Chunk>) -> Self {
        Self::with_capacity(reader, BUF_SIZE, pool)
    }

    /// Create a ChunkReader with the given capacity and pool size
    pub fn with_capacity(reader: R, buf_size: usize, pool: Pool<Chunk>) -> Self {
        let line_reader = LineReader::with_capacity(buf_size, reader);
        Self {
            reader: line_reader,
            pool,
            buf_size,
        }
    }

    /// Get a chunk of bytes that for sure ends with a newline
    pub fn next_chunk(&mut self) -> Option<Result<Reusable<Chunk>, io::Error>> {
        let buf_size = self.buf_size;
        let mut pool_chunk = self.pool.pull(|| Chunk::with_capacity(buf_size));
        pool_chunk.clear();
        while let Some(chunk) = self.reader.next_batch() {
            match chunk {
                Ok(chunk) => {
                    pool_chunk.extend(chunk);
                    // `next_batch` may not end with a newline if no newline is found with the buffer size.
                    if pool_chunk.ends_with(&[b'\n']) {
                        return Some(Ok(pool_chunk));
                    }
                }
                Err(err) => return Some(Err(err)),
            }
        }
        // If there is anything in the pool_chunk, add a newline andsend
        if pool_chunk.len() > 0 {
            pool_chunk.extend(&[b'\n']);
            return Some(Ok(pool_chunk));
        }
        None
    }
}
