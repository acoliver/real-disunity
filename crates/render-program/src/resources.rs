//! Buffers, textures and render targets the runtime owns, named by handles.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use worldrunner_render_gpu::submission::SubmissionAuthority;

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
    /// A 2D texture the runtime owns.
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

type RetiredQueue = Arc<Mutex<Vec<Retired>>>;

/// Shares of the reservations of the resources a prepared draw uses. When the draw is dropped
/// the shares wait out the frames that may have recorded it, as released resources do.
pub(crate) struct Pins {
    reservations: Vec<Arc<Reservation>>,
    retired: RetiredQueue,
    authority: SubmissionAuthority,
}

impl Drop for Pins {
    fn drop(&mut self) {
        if self.reservations.is_empty() {
            return;
        }
        // Frames submitted so far are covered by the submitted serial. A frame still being
        // recorded may have recorded the draw and will be the next submission. Replacement
        // is refused exactly while a frame is being recorded or configured.
        let recording = self.authority.try_replacement().is_err();
        let last = self.authority.submitted_serial() + u64::from(recording);
        let reservations = std::mem::take(&mut self.reservations);
        self.retired
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((last, Box::new(reservations)));
    }
}

#[derive(Default)]
pub(crate) struct Store {
    next: u64,
    buffers: HashMap<u64, Held<BufferEntry>>,
    textures: HashMap<u64, Held<TextureEntry>>,
    targets: HashMap<u64, Held<TargetEntry>>,
    retired: RetiredQueue,
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

    /// Holds `reservations` until the frames that may use them have completed after the
    /// returned pins are dropped.
    pub(crate) fn pin(
        &self,
        authority: &SubmissionAuthority,
        reservations: Vec<Arc<Reservation>>,
    ) -> Pins {
        Pins {
            reservations,
            retired: self.retired.clone(),
            authority: authority.clone(),
        }
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
