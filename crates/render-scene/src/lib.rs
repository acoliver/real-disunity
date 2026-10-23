//! Pathless GPU rendering for validated, wholly owned scenes.
//!
//! The only content input is a [`RenderScene`]. Geometry, textures, materials,
//! terrain, and effects arrive through that CPU boundary; the renderer has no
//! asset resolver, package reader, filesystem input, or content adapter.
//!
//! [`RenderScene`]: worldrunner_render_plan::RenderScene

pub mod compositor;
pub mod device;
pub mod draw;
pub mod frame;
pub mod geometry;
mod instances;
pub mod offscreen;
pub mod pipeline;
pub mod renderer;
pub mod shader;
pub mod shadows;
mod ssao;
pub mod terrain;
pub mod terrain_uniform;
pub mod texture;
pub mod upload;
pub mod view;

/// GPU API types used by scene construction and capture.
pub use wgpu;

pub use view::{CameraPose, SceneView, ViewError, Viewport};
