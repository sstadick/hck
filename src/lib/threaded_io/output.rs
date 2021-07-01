//! Create a Writer File Handle for ChunkWriter
use crate::chunk::{chunk_processor::ProcessedChunk, chunk_writer::ChunkWriter, Chunk};
use crossbeam::channel::{Receiver, Sender};
use std::{
    fs::File,
    io::{self, Write},
    path::PathBuf,
    thread::{self, JoinHandle},
};
use termcolor::ColorChoice;

pub struct WriterFileHandle {
    pub thread_handle: JoinHandle<Result<(), io::Error>>,
}

impl WriterFileHandle {
    pub fn new(
        rx: Receiver<ProcessedChunk>,
        path: Option<PathBuf>,
        output_delim: String,
        buf_tx: Sender<Chunk>,
    ) -> Self {
        let t = thread::spawn(move || -> Result<(), io::Error> {
            let writer: Box<dyn Write> = if let Some(path) = path {
                Box::new(File::create(path)?)
            } else {
                Box::new(grep_cli::stdout(ColorChoice::Never))
            };
            let mut chunk_writer = ChunkWriter::new(writer, &output_delim);
            for chunk in rx.iter() {
                let chunk = chunk_writer
                    .write_chunk(chunk)
                    .expect("Failed to write chunk.");
                buf_tx.try_send(chunk);
            }
            chunk_writer.flush()?;
            Ok(())
        });
        Self { thread_handle: t }
    }
}
