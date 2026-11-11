use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

/// Reads a file in chunks of `chunk_size` bytes.
pub struct ChunkReader {
    reader: BufReader<File>,
    chunk_size: usize,
    eof: bool,
}

pub trait ReadChunks {
    fn next_chunk(&mut self) -> io::Result<Option<Vec<u8>>>;
}

impl ChunkReader {
    pub fn open<P: AsRef<Path>>(path: P, chunk_size: usize) -> io::Result<Self> {
        let file = File::open(path)?;
        Ok(Self {
            reader: BufReader::new(file),
            chunk_size,
            eof: false,
        })
    }
}

impl ReadChunks for ChunkReader {
    fn next_chunk(&mut self) -> io::Result<Option<Vec<u8>>> {
        if self.eof {
            return Ok(None);
        }
        let mut buf = vec![0u8; self.chunk_size];
        let read = self.reader.read(&mut buf)?;
        if read == 0 {
            self.eof = true;
            return Ok(None);
        }
        buf.truncate(read);
        Ok(Some(buf))
    }
}

/// Memory-mapped variant of [`ChunkReader`]; each chunk is copied out of the mapping.
#[cfg(feature = "mmap")]
pub struct MmapChunkReader {
    mmap: memmap2::Mmap,
    offset: usize,
    chunk_size: usize,
}

#[cfg(feature = "mmap")]
impl MmapChunkReader {
    pub fn open<P: AsRef<Path>>(path: P, chunk_size: usize) -> io::Result<Self> {
        let file = File::open(path)?;
        // safety: file is not mutated while mapped
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };
        Ok(Self {
            mmap,
            offset: 0,
            chunk_size,
        })
    }
}

#[cfg(feature = "mmap")]
impl ReadChunks for MmapChunkReader {
    fn next_chunk(&mut self) -> io::Result<Option<Vec<u8>>> {
        if self.offset >= self.mmap.len() {
            return Ok(None);
        }
        let end = (self.offset + self.chunk_size).min(self.mmap.len());
        let slice = &self.mmap[self.offset..end];
        let out = slice.to_vec();
        self.offset = end;
        Ok(Some(out))
    }
}
