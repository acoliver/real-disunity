//! The graphics resources a world carries for a host that draws: shaders, materials, render
//! textures, compute shaders and buffers, the global shader state and the offscreen work scripts
//! record each frame.
//!
//! The world stores and validates these; it draws nothing. A host reads them through the
//! accessors on [`World`] when it extracts a frame.
//!
//! Destroying a resource that something still names is allowed, which is the engine's choice:
//! the name goes stale instead of keeping the resource alive. A stale id reads as an error
//! ([`GraphicsError::StaleShader`] and its siblings). A texture property that names a destroyed
//! render texture reads as [`TextureRef::None`] through [`World::live_texture`]. Offscreen work
//! that names a destroyed render texture stays in the list, and the host skips it when the id no
//! longer resolves.
mod active_pass;
mod commands;
mod compute;
mod context;
mod material;
mod offscreen;
mod properties;
mod render_texture;
mod slab;
pub mod snapshot;
pub mod snapshot_wire;

pub use active_pass::ActivePass;
pub use commands::{MAX_COLOR_TARGETS, TargetRef};
pub use context::MAX_CULL_PLANES;
pub use compute::{
    ComputeBindings, ComputeKernel, MAX_COMPUTE_BUFFER_BYTES, MAX_COMPUTE_TOTAL_BYTES,
    WorldComputeBuffer, WorldComputeShader,
};
pub use material::{GlobalShaderState, MaterialEdit, WorldMaterial};
pub use offscreen::{MAX_OFFSCREEN_WORK, OffscreenWork};
pub use properties::{
    MAX_NAME_BYTES, MAX_PROPERTIES_PER_KIND, PropertyValue, ShaderProperties, TextureRef,
    TextureSlot,
};
pub use render_texture::{WorldRenderTexture, WorldSampler};
pub use snapshot::{CameraView, GraphicsSnapshot, RendererView};

use slab::Slab;

/// Most shaders, materials and render textures a world holds, each.
const MAX_SHADERS: usize = 1 << 16;
const MAX_MATERIALS: usize = 1 << 20;
const MAX_RENDER_TEXTURES: usize = 1 << 16;
const MAX_COMPUTE_SHADERS: usize = 1 << 16;
const MAX_COMPUTE_BUFFERS: usize = 1 << 16;

/// Why a graphics call was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GraphicsError {
    #[error("the shader id is stale")]
    StaleShader,
    #[error("the material id is stale")]
    StaleMaterial,
    #[error("the render texture id is stale")]
    StaleRenderTexture,
    #[error("names hold 1 to 256 bytes")]
    InvalidName,
    #[error("values must be finite")]
    NotFinite,
    #[error("a texture property names a nonzero asset or a live render texture")]
    InvalidTexture,
    #[error("a set of properties holds at most 1024 values of one kind")]
    TooManyProperties,
    #[error("a material or the global state holds at most 256 keywords or disabled passes")]
    TooManyKeywords,
    #[error("a shader source needs a file name of 1 to 1024 bytes")]
    InvalidShaderSource,
    #[error(
        "a render texture needs a size of 1 to 16384, depth bits 0, 16, 24 or 32 and 1 to 15 mips"
    )]
    InvalidRenderTexture,
    #[error("the offscreen work names a pass below -1 or a mip level the texture lacks")]
    InvalidOffscreen,
    #[error("the camera of offscreen work must be a live camera component")]
    NotCamera,
    #[error("the compute shader id is stale")]
    StaleComputeShader,
    #[error("the compute buffer id is stale")]
    StaleComputeBuffer,
    #[error(
        "a compute shader holds at most 256 kernels, each with a thread group size of at most 1024 by 1024 by 64 and 1024 threads"
    )]
    InvalidComputeShader,
    #[error("the compute shader has no kernel with this index")]
    InvalidKernel,
    #[error(
        "a compute buffer needs a count of at least 1, a stride that is a multiple of 4 from 4 to 2048, at most 256 MiB, and a world holds at most 1 GiB of them"
    )]
    InvalidComputeBuffer,
    #[error("the data range lies outside the compute buffer")]
    ComputeRange,
}

impl GraphicsError {
    /// Whether the error is an id that names nothing, as opposed to a rejected value.
    pub fn is_stale(self) -> bool {
        matches!(
            self,
            Self::StaleShader
                | Self::StaleMaterial
                | Self::StaleRenderTexture
                | Self::StaleComputeShader
                | Self::StaleComputeBuffer
        )
    }
}

macro_rules! graphics_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);
        impl $name {
            /// The id as 64 bits for a foreign caller: the slot in the low half and the
            /// generation, from 1, in the high half. Zero never names a resource.
            pub fn to_bits(self) -> u64 {
                self.0
            }
            /// An id from bits [`Self::to_bits`] gave. Whether it names a live resource is the
            /// world's to answer.
            pub fn from_bits(bits: u64) -> Self {
                Self(bits)
            }
        }
    };
}

graphics_id!(
    /// A shader registered in a world.
    ShaderId
);
graphics_id!(
    /// A material of a world.
    MaterialId
);
graphics_id!(
    /// A render texture of a world.
    RenderTextureId
);
graphics_id!(
    /// A compute shader registered in a world.
    ComputeShaderId
);
graphics_id!(
    /// A compute buffer of a world.
    ComputeBufferId
);

/// Where a shader's programs are: the serialized file the shader object lives in, as the
/// reader names it, and the object's path id in that file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderSource {
    pub file: String,
    pub path_id: i64,
}

/// A shader registered by name. A shader without a source is one the engine supplies itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldShader {
    pub name: String,
    pub source: Option<ShaderSource>,
}

impl ShaderSource {
    pub(crate) fn validate(&self) -> Result<(), GraphicsError> {
        if self.file.is_empty() || self.file.len() > 1024 {
            return Err(GraphicsError::InvalidShaderSource);
        }
        Ok(())
    }
}

impl WorldShader {
    pub(crate) fn validate(&self) -> Result<(), GraphicsError> {
        properties::check_name(&self.name)?;
        self.source.as_ref().map_or(Ok(()), ShaderSource::validate)
    }
}

/// The graphics state of one world.
#[cfg_attr(test, derive(Clone))]
pub(crate) struct GraphicsState {
    shaders: Slab<WorldShader>,
    materials: Slab<WorldMaterial>,
    render_textures: Slab<WorldRenderTexture>,
    compute_shaders: Slab<compute::ComputeShaderEntry>,
    compute_buffers: Slab<WorldComputeBuffer>,
    /// Bytes the live compute buffers hold.
    compute_bytes: u64,
    global: GlobalShaderState,
    offscreen: Vec<OffscreenWork>,
    /// The handles the frame's records issued, which later records of the frame may name.
    handles: std::collections::BTreeMap<u64, context::HandleKind>,
    /// The pass immediate-mode draws use; not part of the snapshot.
    active_pass: Option<ActivePass>,
}

impl GraphicsState {
    /// Drops the offscreen work of the previous frame that nobody took.
    pub(crate) fn begin_frame(&mut self) {
        self.offscreen.clear();
        self.handles.clear();
    }

    pub(crate) const fn new() -> Self {
        Self {
            shaders: Slab::new(MAX_SHADERS),
            materials: Slab::new(MAX_MATERIALS),
            render_textures: Slab::new(MAX_RENDER_TEXTURES),
            compute_shaders: Slab::new(MAX_COMPUTE_SHADERS),
            compute_buffers: Slab::new(MAX_COMPUTE_BUFFERS),
            compute_bytes: 0,
            global: GlobalShaderState::new(),
            offscreen: Vec::new(),
            handles: std::collections::BTreeMap::new(),
            active_pass: None,
        }
    }
}

use crate::{World, WorldError};

impl World {
    /// Registers a shader.
    pub fn create_shader(&mut self, shader: WorldShader) -> Result<ShaderId, WorldError> {
        shader.validate()?;
        Ok(ShaderId(self.graphics.shaders.insert(shader)?))
    }

    /// Replaces a shader's name and source. Materials that use it keep using it.
    pub fn update_shader(&mut self, id: ShaderId, shader: WorldShader) -> Result<(), WorldError> {
        shader.validate()?;
        *self
            .graphics
            .shaders
            .get_mut(id.0)
            .ok_or(GraphicsError::StaleShader)? = shader;
        Ok(())
    }

    /// Removes a shader. Materials that name it keep the stale id.
    pub fn destroy_shader(&mut self, id: ShaderId) -> Result<(), WorldError> {
        self.graphics
            .shaders
            .remove(id.0)
            .map(drop)
            .ok_or(WorldError::Graphics(GraphicsError::StaleShader))
    }

    pub fn shader(&self, id: ShaderId) -> Result<&WorldShader, WorldError> {
        Ok(self
            .graphics
            .shaders
            .get(id.0)
            .ok_or(GraphicsError::StaleShader)?)
    }

    /// Every live shader, in slot order.
    pub fn shaders(&self) -> impl Iterator<Item = (ShaderId, &WorldShader)> {
        self.graphics
            .shaders
            .iter()
            .map(|(bits, shader)| (ShaderId(bits), shader))
    }
}
