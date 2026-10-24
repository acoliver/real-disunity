use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::ReadbackError;

/// Maps caller-visible staging after submission. The host drives device polling.
/// Dropping an unfinished readback unmaps it without waiting for GPU completion.
pub struct AsyncReadback {
    buffer: wgpu::Buffer,
    pending: Option<Receiver<Result<(), wgpu::BufferAsyncError>>>,
}
impl AsyncReadback {
    pub fn begin(buffer: &wgpu::Buffer) -> Result<Self, ReadbackError> {
        if !buffer.usage().contains(wgpu::BufferUsages::MAP_READ)
            || !buffer.size().is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT)
        {
            return Err(ReadbackError::Limit);
        }
        let (send, receive) = mpsc::sync_channel(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = send.send(result);
            });
        Ok(Self {
            buffer: buffer.clone(),
            pending: Some(receive),
        })
    }
    /// Returns immediately. None means the host has not received completion yet.
    pub fn try_read(&mut self) -> Result<Option<Vec<u8>>, ReadbackError> {
        let Some(receiver) = &self.pending else {
            return Ok(None);
        };
        match receiver.try_recv() {
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                self.buffer.unmap();
                Err(ReadbackError::Callback)
            }
            Ok(result) => {
                self.pending = None;
                let bytes = result.map_err(ReadbackError::Map).and_then(|()| {
                    let mapped = self.buffer.slice(..).get_mapped_range()?;
                    Ok(mapped.to_vec())
                });
                self.buffer.unmap();
                bytes.map(Some)
            }
        }
    }
}
impl Drop for AsyncReadback {
    fn drop(&mut self) {
        if self.pending.is_some() {
            self.buffer.unmap();
        }
    }
}
