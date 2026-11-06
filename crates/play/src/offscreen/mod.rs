//! The host adapter that turns the offscreen work a Unity player recorded in the world into
//! render-program's [`OffscreenFrame`](worldrunner_render_program::offscreen::frame::OffscreenFrame),
//! so a camera render, a blit or a texture copy of the player runs with the game's own shaders.
//!
//! # Placement
//!
//! The adapter is in `play`, the crate that runs a built player. It needs the world's types
//! (`world-simulation`, Foundation), the importer's readers for the player's serialized files
//! (`unity-import`, which only a Tool crate may depend on) and render-program (Graphics).
//! `scene-viewer`, the one crate in the Host layer, is a window and an event loop with none of
//! those dependencies, and a Host crate may not depend on the importer. `play` already owns the
//! managed host and the conversion of the player's meshes and textures, which is what the adapter
//! consumes.
//!
//! # A frame
//!
//! 1. The host's `graphics_snapshot` drains the world's records and copies what they reach.
//! 2. [`OffscreenAdapter::adapt`] loads the shaders ([`ShaderLoader`]), uploads the meshes and
//!    stored textures the managed engine holds ([`Assets`]) and maps the snapshot
//!    ([`frame::adapt`]) onto an `OffscreenFrame`, with a [`Report`] of what it left out.
//! 3. [`build`] turns the frame into `OffscreenOperations`; `admit` and
//!    `SceneGpu::admit_offscreen` then take them as render-program documents.
//!
//! # Conventions
//!
//! * Matrices are column-major, as the world stores them.
//! * The view, the projection and the camera position follow the engine layer's camera
//!   ([`camera`]), mapped to the clip space render-program's conventions tests measured.
//! * The built-in values and the evidence for each name are in [`builtins`].
//! * Texture rows are bottom first, as the engine stores them, which is the orientation of a
//!   render texture's rows.
//!
//! # Not mapped
//!
//! * A texture slot's scale and offset (render-program's property convention names no member
//!   that carries them).
//! * The wrap along w, anisotropy and mip bias of a sampler (render-program's sampler state
//!   carries a wrap along u and v and a filter), and a render texture's sampler when a blit or a
//!   copy reads it as its source, which the frame does not describe; those read with the fixed
//!   sampler.
//! * A material's disabled passes.
//! * Built-in values whose meaning the repository's evidence does not give; [`builtins`] lists
//!   them with the number of the player's variants that hold each.

mod adapter;
mod assets;
mod builtins;
mod camera;
mod census;
mod compute;
mod flush;
mod frame;
mod ids;
mod kernels;
mod properties;
mod shaders;

#[cfg(test)]
mod tests;

pub use adapter::{OffscreenAdapter, build};
pub use assets::{
    AssetError, Assets, MeshError, mesh_of, sampler_of_stored, sampler_of_world, texture_of,
};
pub use builtins::{
    CAMERA_POSITION, OBJECT_TO_WORLD, VIEW, VIEW_PROJECTION, WORLD_TO_OBJECT, camera_layer,
    draw_layer,
};
pub use camera::{clip_from_gl, projection, world_to_camera};
pub use census::{ShaderCensus, shader_census};
pub use compute::{
    BufferTrace, ComputeAdapter, ComputeLoadError, ComputeSupply, DispatchTrace, NoKernels,
    TextureTrace,
};
pub use flush::{BufferSink, BufferWrite, FlushError, FlushOutcome, OffscreenHost};
pub use frame::{
    Adapted, Available, BLIT_MESH, Placed, RendererSkip, Report, Settings, ShaderInfo, Skip, adapt,
};
pub use kernels::PlayerKernels;
pub use shaders::{LoadedShader, ShaderLoadError, ShaderLoader, ShaderSupply};
