//! A block of bytes that is guarenteed to end in a neline.

use bstr::ByteSlice;

/// A thin wrapper over a vec of bytes.
#[derive(Debug)]
pub struct Chunk {
    buffer: Vec<u8>,
}

impl Chunk {
    /// Create a new chunk with capacity 0
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// Create a chunk with the given capacity.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(cap),
        }
    }

    /// Clear the inner chunk buffer
    pub fn clear(&mut self) {
        self.buffer.clear()
    }

    /// Extend chunk with the given bytes
    pub fn extend(&mut self, other: &[u8]) {
        // TODO: make sure this is the most effient way to add bytes
        self.buffer.extend(other)
    }

    /// Check [`Chunk`] ends with the given slice
    pub fn ends_with(&self, needle: &[u8]) -> bool {
        self.buffer.ends_with(needle)
    }

    /// Get the number of bytes in [`Chunk`]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if [`Chunk`] is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get a Line iterator from the chunk
    pub fn lines(&self) -> impl Iterator<Item = &[u8]> {
        self.buffer.lines()
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}
