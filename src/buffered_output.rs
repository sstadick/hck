use std::io::{self, Write};

/// See https://github.com/eBay/tsv-utils/blob/38ed0a1c31742bd8b59196517e89ff0b51e8fb80/common/src/tsv_utils/common/utils.d#L411

pub mod BufferedOutputDefaults {
    pub const FLUSH_SIZE: usize = 10_240;
    pub const LINE_BUF_FLUSH_SIZE: usize = 1;
    pub const RESERVE_SIZE: usize = 11_264;
    pub const MAX_SIZE: usize = 4_194_304;
}

pub struct BufferedOutput<W>
where
    W: Write,
{
    writer: W,
    output_buffer: Vec<u8>,
    flush_size: usize,
    max_size: usize,
}

impl<W: Write> BufferedOutput<W> {
    #[inline]
    pub fn new(writer: W, flush_size: usize, reserve_size: usize, max_size: usize) -> Self {
        assert!(flush_size <= max_size);
        BufferedOutput {
            writer,
            output_buffer: Vec::with_capacity(reserve_size),
            max_size,
            flush_size,
        }
    }

    #[inline]
    fn flush_buffer(&mut self) -> Result<(), io::Error> {
        self.writer.write_all(&self.output_buffer)?;
        self.output_buffer.clear();
        Ok(())
    }

    #[inline]
    pub fn flush(&mut self) -> Result<(), io::Error> {
        self.flush_buffer()?;
        self.writer.flush()?;
        Ok(())
    }

    /// Flushes the internal buffer if flushSize has been reached
    #[inline]
    fn flush_if_full(&mut self) -> Result<bool, io::Error> {
        let is_full = self.output_buffer.len() >= self.flush_size;
        if is_full {
            self.flush_buffer()?;
        }
        Ok(is_full)
    }

    /// Safety check to avoid runaway buffer growth
    #[inline]
    fn flush_if_max_size(&mut self) -> Result<(), io::Error> {
        if self.output_buffer.len() >= self.max_size {
            self.flush_buffer()?;
        }
        Ok(())
    }

    /// Maybe flush is used when data is added with a trailing newline.
    ///
    /// Flushing occurs if the buffer has a trailing newline and has reached flush size
    /// Flushing also occurs if the buffer has reached max size.
    #[inline]
    fn maybe_flush(&mut self) -> Result<bool, io::Error> {
        let do_flush = self.output_buffer.len() >= self.flush_size
            && (self.output_buffer.ends_with(&[b'\n'])
                || self.output_buffer.len() >= self.max_size);
        if do_flush {
            self.flush()?
        }
        Ok(do_flush)
    }

    /// Appends data to the output buffer without checking for flush conditions. This is intended for cases
    /// where an `appendln` or `append` ending in newline will shortly follow.
    #[inline]
    fn append_raw(&mut self, stuff: &[u8]) {
        self.output_buffer.extend_from_slice(stuff)
    }

    /// Appends data to the output buffer. The output buffer is flushed if the appended data
    /// ends in a newline and the output buffer has reached flush_size.
    #[inline]
    pub fn append<'a>(&mut self, stuffs: impl Iterator<Item = &'a [u8]>) -> Result<(), io::Error> {
        for stuff in stuffs {
            self.append_raw(stuff);
        }
        self.maybe_flush()?;
        Ok(())
    }

    /// Appends data plua a newline to the output buffer. The output buffer is flushed if it has
    /// reached flush_size.
    #[inline]
    pub fn appendln<'a>(
        &mut self,
        stuffs: impl Iterator<Item = &'a [u8]>,
    ) -> Result<bool, io::Error> {
        for stuff in stuffs {
            self.append_raw(stuff);
        }
        self.append_raw(&[b'\n']);
        self.flush_if_full()
    }

    /// An optimization of append with delimiter.
    #[inline]
    pub fn join_append<'a>(
        &mut self,
        mut stuffs: impl Iterator<Item = &'a [u8]>,
        delim: &[u8],
    ) -> Result<(), io::Error> {
        if let Some(stuff) = stuffs.next() {
            self.append_raw(stuff);
        }

        for stuff in stuffs {
            self.append_raw(delim);
            self.append_raw(stuff);
        }
        self.flush_if_max_size()?;
        Ok(())
    }

    #[inline]
    pub fn put_str(&mut self, stuff: &[u8]) -> Result<(), io::Error> {
        if stuff == &[b'\n'] {
            self.appendln(std::iter::empty())?;
        } else {
            self.append(std::iter::once(stuff))?;
        }
        Ok(())
    }

    #[inline]
    pub fn put_chr(&mut self, stuff: u8) -> Result<(), io::Error> {
        if stuff == b'\n' {
            self.appendln(std::iter::empty())?;
        } else {
            self.append_raw(&[stuff]);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {}

// impl drop?
// impl<W: Write> BufferedOutput<W> {

// }
