//! Reusable upload staging for prepared frames.
//!
//! Frame uploads are written into mapped staging chunks that the submission authority keeps
//! across frames, instead of a new buffer per upload. A chunk belongs to one transaction while
//! it records. Submission unmaps the transaction's chunks before the queue sees them and asks
//! to map them again right after; wgpu completes that mapping only once the submission that
//! read them has finished, so a chunk is never rewritten while the GPU may still copy from it.
//! A cancelled transaction never submits, so its chunks are still mapped and are reused at once:
//! the copies that would have read them were recorded only into the discarded encoder.
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

/// The smallest chunk. Larger uploads get a chunk of the next power of two. Either is cut to the
/// largest buffer the device may create.
pub(super) const MINIMUM_CHUNK_BYTES: u64 = 256 * 1024;
/// Free chunks unused for this many transactions are released.
pub(super) const IDLE_TRANSACTIONS: u64 = 1024;
/// Staged offsets satisfy both the mapped-range and the buffer-copy alignment.
const ALIGNMENT: u64 = 8;

const WAITING: u8 = 0;
const MAPPED: u8 = 1;
const FAILED: u8 = 2;

/// Cumulative staging counters for one submission authority.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StagingStatistics {
    /// Staging chunks created, and their bytes.
    pub chunks_created: u64,
    pub bytes_created: u64,
    /// Chunks held now, free, recording or in flight, and their bytes.
    pub chunks: u64,
    pub bytes: u64,
    /// Uploads staged, and their bytes.
    pub uploads: u64,
    pub uploaded_bytes: u64,
}

/// Completion of the mapping requested after a chunk's submission.
#[derive(Clone, Debug)]
pub(super) struct MapState(Arc<AtomicU8>);
impl Default for MapState {
    fn default() -> Self {
        Self(Arc::new(AtomicU8::new(WAITING)))
    }
}
impl MapState {
    pub(super) fn complete(&self, mapped: bool) {
        self.0
            .store(if mapped { MAPPED } else { FAILED }, Ordering::Release);
    }
    fn get(&self) -> u8 {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug)]
enum State {
    /// Mapped and unused.
    Free,
    /// Mapped and filled up to `used` by the recording transaction `owner`.
    Recording { owner: u64, used: u64 },
    /// Unmapped and submitted; usable again once its mapping completes.
    Submitted(MapState),
}

#[derive(Debug)]
struct Chunk<B> {
    buffer: B,
    size: u64,
    state: State,
    last_used: u64,
}

/// Chunk bookkeeping, independent of the buffer type so its rules are tested without a device.
#[derive(Debug)]
pub(super) struct Pool<B> {
    chunks: Vec<Chunk<B>>,
    statistics: StagingStatistics,
    latest: u64,
}
impl<B> Default for Pool<B> {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            statistics: StagingStatistics::default(),
            latest: 0,
        }
    }
}

/// Where one upload was staged.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Reservation<B> {
    pub(super) buffer: B,
    pub(super) offset: u64,
}

impl<B: Clone> Pool<B> {
    /// Reserves `len` bytes for `transaction`, creating a chunk of `create(size)` bytes only when
    /// no recording or free chunk has room. `len` must be a nonzero multiple of four.
    /// `limit` returns the device's `max_buffer_size`; it is read only when a chunk is created,
    /// and every chunk is at most that large, so `len` must not exceed it.
    pub(super) fn reserve(
        &mut self,
        transaction: u64,
        len: u64,
        limit: impl FnOnce() -> u64,
        create: impl FnOnce(u64) -> B,
    ) -> Reservation<B> {
        assert!(
            len != 0 && len.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
            "staged uploads must be a nonzero multiple of {} bytes, got {len}",
            wgpu::COPY_BUFFER_ALIGNMENT
        );
        assert_ne!(transaction, 0, "staging requires a recording transaction");
        self.latest = self.latest.max(transaction);
        self.recall();
        let index = self
            .recording_with_room(transaction, len)
            .or_else(|| self.smallest_free(len))
            .unwrap_or_else(|| self.create(len, limit(), create));
        let chunk = &mut self.chunks[index];
        let offset = match &mut chunk.state {
            State::Recording { used, .. } => used.next_multiple_of(ALIGNMENT),
            State::Free => 0,
            State::Submitted(_) => unreachable!("selected chunks are mapped"),
        };
        chunk.state = State::Recording {
            owner: transaction,
            used: offset + len,
        };
        chunk.last_used = transaction;
        self.statistics.uploads += 1;
        self.statistics.uploaded_bytes += len;
        Reservation {
            buffer: chunk.buffer.clone(),
            offset,
        }
    }

    /// The chunks `transaction` filled, now submitted. The caller unmaps each before submitting
    /// and requests its mapping afterwards, completing the returned state.
    pub(super) fn submit(&mut self, transaction: u64) -> Vec<(B, MapState)> {
        self.chunks
            .iter_mut()
            .filter(|chunk| matches!(chunk.state, State::Recording { owner, .. } if owner == transaction))
            .map(|chunk| {
                let state = MapState::default();
                chunk.state = State::Submitted(state.clone());
                (chunk.buffer.clone(), state)
            })
            .collect()
    }

    /// Returns the still-mapped chunks of a transaction that ended without submitting.
    pub(super) fn cancel(&mut self, transaction: u64) {
        for chunk in &mut self.chunks {
            if matches!(chunk.state, State::Recording { owner, .. } if owner == transaction) {
                chunk.state = State::Free;
            }
        }
    }

    pub(super) fn statistics(&self) -> StagingStatistics {
        StagingStatistics {
            chunks: self.chunks.len() as u64,
            bytes: self.chunks.iter().map(|chunk| chunk.size).sum(),
            ..self.statistics
        }
    }

    /// Frees submitted chunks whose mapping completed, drops chunks whose mapping failed and
    /// releases free chunks idle for [`IDLE_TRANSACTIONS`].
    fn recall(&mut self) {
        let latest = self.latest;
        self.chunks.retain_mut(|chunk| match &chunk.state {
            State::Submitted(map) => match map.get() {
                MAPPED => {
                    chunk.state = State::Free;
                    true
                }
                FAILED => false,
                _ => true,
            },
            State::Free => latest - chunk.last_used <= IDLE_TRANSACTIONS,
            State::Recording { .. } => true,
        });
    }

    fn recording_with_room(&self, transaction: u64, len: u64) -> Option<usize> {
        self.chunks.iter().position(|chunk| {
            matches!(chunk.state, State::Recording { owner, used }
                if owner == transaction && used.next_multiple_of(ALIGNMENT) + len <= chunk.size)
        })
    }

    fn smallest_free(&self, len: u64) -> Option<usize> {
        self.chunks
            .iter()
            .enumerate()
            .filter(|(_, chunk)| matches!(chunk.state, State::Free) && chunk.size >= len)
            .min_by_key(|(_, chunk)| chunk.size)
            .map(|(index, _)| index)
    }

    fn create(&mut self, len: u64, limit: u64, create: impl FnOnce(u64) -> B) -> usize {
        // A buffer mapped at creation must be a multiple of the copy alignment long.
        let largest = limit - limit % wgpu::COPY_BUFFER_ALIGNMENT;
        assert!(
            len <= largest,
            "a staged upload of {len} bytes exceeds the device's max_buffer_size of {limit} bytes"
        );
        let size = len
            .max(MINIMUM_CHUNK_BYTES)
            .next_power_of_two()
            .min(largest);
        self.chunks.push(Chunk {
            buffer: create(size),
            size,
            state: State::Free,
            last_used: self.latest,
        });
        self.statistics.chunks_created += 1;
        self.statistics.bytes_created += size;
        self.chunks.len() - 1
    }
}

/// A chunk buffer, counted as staging in the authority's census while any clone lives.
type ChunkBuffer = Arc<crate::census::Tracked<wgpu::Buffer>>;

/// The authority's staging: pool bookkeeping over mapped, census-counted wgpu buffers.
#[derive(Debug, Default)]
pub(super) struct Staging(std::sync::Mutex<Pool<ChunkBuffer>>);
impl Staging {
    /// Writes `bytes` into staging reserved for `transaction` and returns the buffer and offset
    /// a copy reads them from.
    pub(super) fn stage(
        &self,
        device: &wgpu::Device,
        census: &crate::census::MemoryCensus,
        transaction: u64,
        bytes: &[u8],
    ) -> (wgpu::Buffer, u64) {
        let len = bytes.len() as u64;
        let reservation = self.0.lock().unwrap().reserve(
            transaction,
            len,
            || device.limits().max_buffer_size,
            |size| {
                Arc::new(census.create_buffer(
                    device,
                    crate::census::ResourceClass::Staging,
                    &wgpu::BufferDescriptor {
                        label: Some("frame upload staging"),
                        size,
                        usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: true,
                    },
                ))
            },
        );
        let buffer: &wgpu::Buffer = &reservation.buffer;
        buffer
            .get_mapped_range_mut(reservation.offset..reservation.offset + len)
            .expect("recording staging chunks stay mapped")
            .copy_from_slice(bytes);
        (buffer.clone(), reservation.offset)
    }

    /// Unmaps `transaction`'s chunks before its commands are submitted. Pass the result to
    /// [`Self::remap`] after the submission.
    pub(super) fn unmap(&self, transaction: u64) -> Vec<(ChunkBuffer, MapState)> {
        let submitted = self.0.lock().unwrap().submit(transaction);
        for (chunk, _) in &submitted {
            chunk.unmap();
        }
        submitted
    }

    /// Requests the mapping that returns submitted chunks to the pool once the GPU is done.
    pub(super) fn remap(submitted: Vec<(ChunkBuffer, MapState)>) {
        for (chunk, state) in submitted {
            chunk.map_async(wgpu::MapMode::Write, .., move |result| {
                state.complete(result.is_ok());
            });
        }
    }

    pub(super) fn cancel(&self, transaction: u64) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cancel(transaction);
    }

    pub(super) fn statistics(&self) -> StagingStatistics {
        self.0.lock().unwrap().statistics()
    }
}

#[cfg(test)]
#[path = "staging_tests.rs"]
mod tests;
