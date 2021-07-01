//! Create a Reader File Handle for ChunkReader
use crate::chunk::{chunk_reader::ChunkReader, Chunk, BUF_SIZE};
use anyhow::Context;
use crossbeam::channel::{Receiver, Sender};
use grep_cli::DecompressionReaderBuilder;
use std::{
    fs::File,
    io::{self, Read},
    path::PathBuf,
    thread::{self, JoinHandle},
};

pub struct ReaderFileHandle {
    pub thread_handle: JoinHandle<Result<(), io::Error>>,
}

impl ReaderFileHandle {
    pub fn new(
        rx: Receiver<Option<PathBuf>>,
        tx: Sender<Chunk>,
        try_decompress: bool,
        buf_rx: Receiver<Chunk>,
    ) -> Self {
        let t = thread::spawn(move || -> Result<(), io::Error> {
            rx.recv().into_iter().for_each(|input| {
                let reader: Box<dyn Read> = if let Some(path) = input {
                    if path.to_str().unwrap_or("") == "-" {
                        Box::new(std::io::stdin())
                    } else if try_decompress {
                        Box::new(
                            DecompressionReaderBuilder::new()
                                .build(&path)
                                .with_context(|| {
                                    format!("Failed to open {} for reading", path.display())
                                })
                                .unwrap(),
                        )
                    } else {
                        Box::new(
                            File::open(&path)
                                .with_context(|| {
                                    format!("Failed to open {} for reading", path.display())
                                })
                                .unwrap(),
                        )
                    }
                } else {
                    Box::new(std::io::stdin())
                };
                let mut chunk_reader = ChunkReader::new(reader);
                while let Some(chunk) =
                    chunk_reader.next_chunk(if let Ok(chunk) = buf_rx.try_recv() {
                        chunk
                    } else {
                        Chunk::with_capacity(BUF_SIZE)
                    })
                {
                    let chunk = chunk.expect("Failed read chunk.");
                    tx.send(chunk).expect("Failed to send reader chunk.");
                }
            });
            Ok(())
        });
        Self { thread_handle: t }
    }
}
