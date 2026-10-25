//! Pathless GPU rendering for validated, wholly owned scenes.
//!
//! Static content arrives through [`RenderScene`]; runtime sprite and UI quads
//! arrive through [`dynamic`]. Both use the same resident scene and pipelines.
//! Geometry, textures, materials, terrain, and effects are pathless inputs; the renderer has no
//! asset resolver, package reader, filesystem input, or content adapter.
//!
//! [`RenderScene`]: worldrunner_render_plan::RenderScene

pub mod compositor;
pub use post::settings as post_settings;
pub mod device;
pub mod draw;
pub mod dynamic;
pub mod frame;
pub mod geometry;
mod instances;
pub mod offscreen;
pub mod pipeline;
pub mod renderer;
pub mod shader;
pub mod shadows;
mod ssao;
mod ssr;
pub mod terrain;
pub mod terrain_uniform;
pub mod texture;
pub mod upload;
pub mod view;
pub mod water;

/// GPU API types used by scene construction and capture.
pub use wgpu;
/// Shared executor contracts for external frame features.
pub use worldrunner_render_post as executor;

pub use view::{CameraPose, OrthographicLens, SceneView, ViewError, Viewport};

mod post;
