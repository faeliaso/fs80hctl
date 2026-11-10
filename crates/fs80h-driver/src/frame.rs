//! Robust frame chunk assembler and safe parser for FS80H sensor data.

use fs80h_core::{CoreError, FRAME_LEN, RawFrame};
use thiserror::Error;

/// Errors arising during raw frame assembly and parsing.
#[derive(Debug, Error)]
pub enum FrameError {
    #[error("frame size mismatch: expected {FRAME_LEN} bytes, got {actual}")]
    SizeMismatch { actual: usize },

    #[error("chunk offset out of bounds: offset {offset} + len {len} > {FRAME_LEN}")]
    ChunkOutOfBounds { offset: usize, len: usize },

    #[error(transparent)]
    Core(#[from] CoreError),
}

/// Fallible parser for raw FS80H frame bytes.
pub fn parse_raw_frame(buf: &[u8]) -> Result<RawFrame, FrameError> {
    if buf.len() != FRAME_LEN {
        return Err(FrameError::SizeMismatch { actual: buf.len() });
    }
    RawFrame::new(buf.to_vec()).map_err(FrameError::Core)
}

/// Frame chunk assembler that validates contiguous arrival of bulk packets.
#[derive(Debug, Clone)]
pub struct FrameAssembler {
    buffer: Vec<u8>,
    written: usize,
}

impl Default for FrameAssembler {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameAssembler {
    pub fn new() -> Self {
        Self {
            buffer: vec![0u8; FRAME_LEN],
            written: 0,
        }
    }

    pub fn append_chunk(&mut self, chunk: &[u8]) -> Result<usize, FrameError> {
        let remaining = FRAME_LEN.saturating_sub(self.written);
        let to_copy = chunk.len().min(remaining);
        if to_copy == 0 && !chunk.is_empty() {
            return Err(FrameError::ChunkOutOfBounds {
                offset: self.written,
                len: chunk.len(),
            });
        }

        if let (Some(dest), Some(src)) = (
            self.buffer.get_mut(self.written..self.written + to_copy),
            chunk.get(..to_copy),
        ) {
            dest.copy_from_slice(src);
            self.written += to_copy;
        }
        Ok(self.written)
    }

    pub fn is_complete(&self) -> bool {
        self.written == FRAME_LEN
    }

    pub fn written_bytes(&self) -> usize {
        self.written
    }

    pub fn finalize(self) -> Result<RawFrame, FrameError> {
        if !self.is_complete() {
            return Err(FrameError::SizeMismatch {
                actual: self.written,
            });
        }
        parse_raw_frame(&self.buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_frame_assembler_complete() {
        let mut assembler = FrameAssembler::new();
        let chunk = vec![128u8; 4096];
        while !assembler.is_complete() {
            assembler.append_chunk(&chunk).unwrap();
        }
        assert!(assembler.is_complete());
        let frame = assembler.finalize().unwrap();
        assert_eq!(frame.pixels().len(), FRAME_LEN);
    }

    proptest! {
        #[test]
        fn test_arbitrary_chunks_never_panic(
            chunks in proptest::collection::vec(proptest::collection::vec(any::<u8>(), 0..2048), 0..20)
        ) {
            let mut assembler = FrameAssembler::new();
            for chunk in chunks {
                let _ = assembler.append_chunk(&chunk);
            }
            let _ = assembler.finalize();
        }

        #[test]
        fn test_arbitrary_raw_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let _ = parse_raw_frame(&bytes);
        }
    }
}
