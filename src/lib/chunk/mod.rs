#[allow(clippy::module_inception)]
mod chunk;
pub mod chunk_processor;
pub mod chunk_reader;
pub mod chunk_writer;

pub use self::chunk::*;

/// Default buffer size
pub const BUF_SIZE: usize = 0x20000;
// pub const BUF_SIZE: usize =
/// Default number of buffers in the pool
pub const NUM_BUF: usize = 10;
