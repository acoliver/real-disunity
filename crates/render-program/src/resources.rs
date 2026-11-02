//! Buffers, textures and render targets the runtime owns, named by handles.

use std::collections::HashMap;

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

#[derive(Default)]
pub(crate) struct Store {
    next: u64,
    buffers: HashMap<u64, BufferEntry>,
    textures: HashMap<u64, TextureEntry>,
    targets: HashMap<u64, TargetEntry>,
}

impl Store {
    fn id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    pub(crate) fn add_buffer(&mut self, entry: BufferEntry) -> BufferHandle {
        let id = self.id();
        self.buffers.insert(id, entry);
        BufferHandle(id)
    }

    pub(crate) fn add_texture(&mut self, entry: TextureEntry) -> TextureHandle {
        let id = self.id();
        self.textures.insert(id, entry);
        TextureHandle(id)
    }

    pub(crate) fn add_target(&mut self, entry: TargetEntry) -> TargetHandle {
        let id = self.id();
        self.targets.insert(id, entry);
        TargetHandle(id)
    }

    pub(crate) fn buffer(&self, handle: BufferHandle) -> Result<&BufferEntry, ProgramError> {
        self.buffers
            .get(&handle.0)
            .ok_or(ProgramError::UnknownHandle { kind: "buffer" })
    }

    pub(crate) fn texture(&self, handle: TextureHandle) -> Result<&TextureEntry, ProgramError> {
        self.textures
            .get(&handle.0)
            .ok_or(ProgramError::UnknownHandle { kind: "texture" })
    }

    pub(crate) fn target(&self, handle: TargetHandle) -> Result<&TargetEntry, ProgramError> {
        self.targets
            .get(&handle.0)
            .ok_or(ProgramError::UnknownHandle { kind: "target" })
    }

    pub(crate) fn remove_buffer(&mut self, handle: BufferHandle) -> bool {
        self.buffers.remove(&handle.0).is_some()
    }

    pub(crate) fn remove_texture(&mut self, handle: TextureHandle) -> bool {
        self.textures.remove(&handle.0).is_some()
    }

    pub(crate) fn remove_target(&mut self, handle: TargetHandle) -> bool {
        self.targets.remove(&handle.0).is_some()
    }
}
