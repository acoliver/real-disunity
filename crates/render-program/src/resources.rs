//! Buffers, textures and render targets the runtime owns, named by handles.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use worldrunner_render_gpu::submission::{FrameOwner, Publications, SubmissionAuthority};

use crate::budget::Reservation;
use crate::error::ProgramError;

macro_rules! handle {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name(u64);
    };
}

handle!(
    /// A buffer the runtime owns.
    BufferHandle
);
handle!(
    /// A texture the runtime owns.
    TextureHandle
);
handle!(
    /// A render target: a color texture and an optional depth texture.
    TargetHandle
);

/// A resource a binding takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Buffer(BufferHandle),
    Texture(TextureHandle),
}

/// A resource for one set and binding of a program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundResource {
    pub set: u32,
    pub binding: u32,
    pub resource: Resource,
}

impl BoundResource {
    #[must_use]
    pub fn buffer(set: u32, binding: u32, buffer: BufferHandle) -> Self {
        Self {
            set,
            binding,
            resource: Resource::Buffer(buffer),
        }
    }

    #[must_use]
    pub fn texture(set: u32, binding: u32, texture: TextureHandle) -> Self {
        Self {
            set,
            binding,
            resource: Resource::Texture(texture),
        }
    }
}

pub(crate) struct BufferEntry {
    pub(crate) buffer: wgpu::Buffer,
    /// The bytes the caller gave; the GPU buffer may be padded to a multiple of four.
    pub(crate) len: u64,
}

pub(crate) struct TextureEntry {
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
}

pub(crate) struct TargetEntry {
    pub(crate) color: TextureEntry,
    pub(crate) depth: Option<TextureEntry>,
}

/// A resource with the budget reservation that accounts for it. Prepared draws share the
/// reservation, so its bytes stay counted while a draw can still use the resource.
struct Held<T> {
    entry: T,
    reservation: Arc<Reservation>,
}

/// Anything released but possibly still in use by submitted work, and the submission that last
/// could have used it.
type Retired = (u64, Box<dyn Any + Send + Sync>);

/// What a frame holds for the draws it records: shares of the runtime's reservations and the
/// owners of the resources a draw borrowed instead of having the runtime create.
struct FrameHold {
    _reservations: Vec<Arc<Reservation>>,
    _owners: Vec<Keepalive>,
}

/// Something the caller keeps alive for as long as a frame that records a borrowing draw can
/// still be executed by the GPU: the scene object that owns the borrowed buffers, for example.
pub type Keepalive = Arc<dyn Any + Send + Sync>;

/// Shares of the reservations of the resources prepared draws use, and the owners of the
/// resources they borrow, held by a [`FrameOwner`].
///
/// Each frame that records the draws retains the owner through [`retain`](Self::retain), and the
/// shares stay held until that frame's outcome: a cancelled or dropped frame releases them at
/// once and a submitted one when the GPU has completed it. They do not depend on the runtime
/// that made the draws. Dropping the pins gives up only the draws' own hold, by detaching the
/// owner: frames that already retained it can still be submitted and keep it until their outcome.
pub(crate) struct Pins {
    owner: Option<FrameOwner<FrameHold>>,
}

impl Drop for Pins {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.take() {
            owner.detach();
        }
    }
}

impl std::fmt::Debug for Pins {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pins").finish_non_exhaustive()
    }
}

impl Pins {
    pub(crate) fn new(
        authority: &SubmissionAuthority,
        reservations: Vec<Arc<Reservation>>,
    ) -> Self {
        Self::borrowing(authority, reservations, Vec::new())
    }

    /// Pins that also hold `owners`, the borrowed resources' owners.
    pub(crate) fn borrowing(
        authority: &SubmissionAuthority,
        reservations: Vec<Arc<Reservation>>,
        owners: Vec<Keepalive>,
    ) -> Self {
        let held = FrameHold {
            _reservations: reservations,
            _owners: owners,
        };
        Self {
            owner: Some(FrameOwner::new(held, authority)),
        }
    }

    /// Makes the frame `encoder` records hold the shares until the frame is cancelled, dropped
    /// or completed on the GPU.
    ///
    /// # Errors
    /// Fails with `ForeignAuthority` when the frame belongs to another submission authority
    /// than the draws' and when `encoder` is not a frame being recorded.
    pub(crate) fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), ProgramError> {
        if let Some(owner) = &self.owner {
            Publications::retain_lease(encoder, &owner.lease())?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct Store {
    next: u64,
    buffers: HashMap<u64, Held<BufferEntry>>,
    textures: HashMap<u64, Held<TextureEntry>>,
    targets: HashMap<u64, Held<TargetEntry>>,
    retired: Mutex<Vec<Retired>>,
}

fn held<T>(entry: T, reservation: Reservation) -> Held<T> {
    Held {
        entry,
        reservation: Arc::new(reservation),
    }
}

impl Store {
    fn id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    pub(crate) fn add_buffer(&mut self, entry: BufferEntry, held_by: Reservation) -> BufferHandle {
        let id = self.id();
        self.buffers.insert(id, held(entry, held_by));
        BufferHandle(id)
    }

    pub(crate) fn add_texture(
        &mut self,
        entry: TextureEntry,
        held_by: Reservation,
    ) -> TextureHandle {
        let id = self.id();
        self.textures.insert(id, held(entry, held_by));
        TextureHandle(id)
    }

    pub(crate) fn add_target(&mut self, entry: TargetEntry, held_by: Reservation) -> TargetHandle {
        let id = self.id();
        self.targets.insert(id, held(entry, held_by));
        TargetHandle(id)
    }

    pub(crate) fn buffer(&self, handle: BufferHandle) -> Result<&BufferEntry, ProgramError> {
        self.buffers
            .get(&handle.0)
            .map(|held| &held.entry)
            .ok_or(ProgramError::UnknownHandle { kind: "buffer" })
    }

    pub(crate) fn texture(&self, handle: TextureHandle) -> Result<&TextureEntry, ProgramError> {
        self.textures
            .get(&handle.0)
            .map(|held| &held.entry)
            .ok_or(ProgramError::UnknownHandle { kind: "texture" })
    }

    pub(crate) fn target(&self, handle: TargetHandle) -> Result<&TargetEntry, ProgramError> {
        self.targets
            .get(&handle.0)
            .map(|held| &held.entry)
            .ok_or(ProgramError::UnknownHandle { kind: "target" })
    }

    /// The reservation of the buffer, to keep its bytes counted.
    pub(crate) fn buffer_reservation(
        &self,
        handle: BufferHandle,
    ) -> Result<Arc<Reservation>, ProgramError> {
        self.buffers
            .get(&handle.0)
            .map(|held| held.reservation.clone())
            .ok_or(ProgramError::UnknownHandle { kind: "buffer" })
    }

    /// The reservation of the texture, to keep its bytes counted.
    pub(crate) fn texture_reservation(
        &self,
        handle: TextureHandle,
    ) -> Result<Arc<Reservation>, ProgramError> {
        self.textures
            .get(&handle.0)
            .map(|held| held.reservation.clone())
            .ok_or(ProgramError::UnknownHandle { kind: "texture" })
    }

    /// The reservation of the target, to keep its bytes counted.
    pub(crate) fn target_reservation(
        &self,
        handle: TargetHandle,
    ) -> Result<Arc<Reservation>, ProgramError> {
        self.targets
            .get(&handle.0)
            .map(|held| held.reservation.clone())
            .ok_or(ProgramError::UnknownHandle { kind: "target" })
    }

    /// Takes the resource out of use for new work. Its reservation stays until the work
    /// submitted through `last_submission` has completed.
    pub(crate) fn remove_buffer(&mut self, handle: BufferHandle, last_submission: u64) -> bool {
        let removed = self.buffers.remove(&handle.0);
        self.retire(removed, last_submission)
    }

    pub(crate) fn remove_texture(&mut self, handle: TextureHandle, last_submission: u64) -> bool {
        let removed = self.textures.remove(&handle.0);
        self.retire(removed, last_submission)
    }

    pub(crate) fn remove_target(&mut self, handle: TargetHandle, last_submission: u64) -> bool {
        let removed = self.targets.remove(&handle.0);
        self.retire(removed, last_submission)
    }

    fn retire<T: Send + Sync + 'static>(
        &self,
        removed: Option<Held<T>>,
        last_submission: u64,
    ) -> bool {
        let Some(removed) = removed else { return false };
        self.retired
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((last_submission, Box::new(removed)));
        true
    }

    /// Frees released resources whose last possible use has completed.
    pub(crate) fn collect(&self, completed: u64) {
        self.retired
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(last_submission, _)| *last_submission > completed);
    }
}
