//! The host adapter that turns the offscreen work a Unity player recorded in the world into
//! render-program's [`OffscreenFrame`](worldrunner_render_program::unity::offscreen::frame::OffscreenFrame),
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
//! A flush also answers the world's pixel reads in place among the work (see the `pixels` module).
//!
//! # Conventions
//!
//! * Matrices are column-major, as the world stores them.
//! * The view, the projection and the camera position follow the engine layer's camera
//!   ([`camera`]), mapped to the clip space render-program's conventions tests measured.
//! * Depth is reversed: near is 1 and far is 0. The render-program state clears depth to the far
//!   value and reverses depth tests and the shadow comparison, every camera's clip space maps near
//!   to w ([`clip_from_gl`]), and `prepare_player` tells the managed side's `PlayerDevice` that
//!   the buffer is reversed. An Old World `RomeTurn42` run, frame 16 (board 133, posts 3214, 3220
//!   and 3238), measured the difference: with forward depth every camera texture held one value
//!   across the screen (normals exactly (0, 0, -1, 0), depth 0.0076), and with reversed depth the
//!   normals varied across the screen and real geometry won the depth test.
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
mod bounds;
mod builtins;
mod camera;
mod census;
mod compute;
mod defaults;
mod environment;
mod flush;
mod frame;
mod holdings;
mod ids;
mod kernels;
mod layer_assets;
mod light_data;
mod lists;
mod mesh_draw;
mod mips;
mod pass_notes;
mod pixels;
mod probe;
mod procedural_draw;
mod properties;
mod raster;
mod record_names;
mod record_sets;
mod records;
mod report;
mod screen;
mod script;
mod shaders;
mod sink;
mod stand_ins;
mod supplied;
mod taps;
mod trace;
mod ui_frame;
mod volume_assets;

#[cfg(test)]
mod tests;

pub use adapter::OffscreenAdapter;
pub(crate) use adapter::Settling;
pub use adapter::build;
pub use assets::AssetError;
pub use assets::Assets;
pub use assets::Emptiness;
pub use assets::MeshError;
pub use assets::mesh_of;
pub use assets::sampler_of_stored;
pub use assets::sampler_of_world;
pub use assets::texture_of;
pub use bounds::Bounds;
pub use builtins::CAMERA_POSITION;
pub use builtins::MAIN_LIGHT_COLOR;
pub use builtins::MAIN_LIGHT_POSITION;
pub use builtins::OBJECT_TO_WORLD;
pub use builtins::ORTHO_PARAMS;
pub use builtins::PROJECTION_PARAMS;
pub use builtins::SCREEN_PARAMS;
pub use builtins::SH_AB;
pub use builtins::SH_AG;
pub use builtins::SH_AR;
pub use builtins::SH_BB;
pub use builtins::SH_BG;
pub use builtins::SH_BR;
pub use builtins::SH_C;
pub use builtins::SPEC_CUBE0_HDR;
pub use builtins::VIEW;
pub use builtins::VIEW_PROJECTION;
pub use builtins::WORLD_TO_OBJECT;
pub use builtins::WORLD_TRANSFORM_PARAMS;
pub use builtins::add_ambient;
pub use builtins::add_main_light;
pub use builtins::camera_layer;
pub use builtins::draw_layer;
pub use camera::clip_from_gl;
pub use camera::projection;
pub use camera::world_to_camera;
pub use census::ComputeCensus;
pub use census::ShaderCensus;
pub use census::compute_census;
pub use census::shader_census;
pub use compute::BufferTrace;
pub use compute::ComputeAdapter;
pub use compute::ComputeLoadError;
pub use compute::ComputeSupply;
pub use compute::DispatchTrace;
pub use compute::NoKernels;
pub use compute::SampledSource;
pub use compute::TextureTrace;
pub use defaults::DefaultTexture;
pub use defaults::StandInLook;
pub use environment::SPEC_CUBE0;
pub use environment::add_environment;
pub use flush::BufferWrite;
pub use flush::FlushError;
pub use flush::FlushOutcome;
pub use flush::OffscreenHost;
pub use flush::UiRead;
pub use frame::Adapted;
pub use frame::Available;
pub use frame::BLIT_MESH;
pub use frame::Placed;
pub use frame::Reclaimed;
pub use frame::RendererSkip;
pub use frame::Report;
pub use frame::Settings;
pub use frame::ShaderInfo;
pub use frame::Skip;
pub use frame::TEST_CLEAR;
pub use frame::adapt;
pub use frame::adapt_probing;
pub use kernels::PlayerKernels;
pub use pixels::PixelAnswer;
pub use pixels::REFUSED_BUDGET;
pub use pixels::REFUSED_FORMAT;
pub use pixels::REFUSED_INTEGER;
pub use pixels::REFUSED_NO_COPY;
pub use pixels::REFUSED_NO_TEXTURE;
pub use pixels::REFUSED_NOT_DECODED;
pub use pixels::REFUSED_NOT_READ;
pub use pixels::is_integer_format;
#[cfg(test)]
pub use screen::REFUSED_SCREEN_FORMAT;
#[cfg(test)]
pub use screen::ScreenImage;
pub use script::temporary_ids;
pub use shaders::LoadedShader;
pub use shaders::ShaderLoadError;
pub use shaders::ShaderLoader;
pub use shaders::ShaderSupply;
pub use sink::BufferSink;
pub use stand_ins::GlobalReads;
pub use stand_ins::StandInCounts;
pub use stand_ins::StandInReason;
pub use supplied::SuppliedGlobal;
pub use supplied::supplied_line;
pub use trace::Phases;
pub use trace::WorkTrace;
pub use ui_frame::UiAssets;
pub use ui_frame::UiFrame;
pub use ui_frame::atlas_id;
