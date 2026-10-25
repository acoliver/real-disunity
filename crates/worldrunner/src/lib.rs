//! The Real Disunity engine facade.
//!
//! Games and tools import `worldrunner` and nothing else. The facade
//! re-exports the engine crates behind modules that mirror the engine layers:
//! [`assets`] for identity, packaging, and resource formats; [`gfx`] for the
//! GPU renderer and camera; [`world`] for schema, simulation, generation, and
//! save; and [`script`] for Lua.
//!
//! The facade is deliberately thin: every item here is a re-export, so the
//! stable surface is exactly the union of the engine crates' public APIs and
//! the engine can restructure internally without touching game code.
//!
//! # Examples
//!
//! ```
//! use worldrunner::LogicalAssetId;
//!
//! let id = LogicalAssetId::new("demo.material.checker").unwrap();
//! assert_eq!(id.as_str(), "demo.material.checker");
//! ```

/// Re-exported so games do not need a direct glam dependency for the common
/// math types the engine APIs take.
pub use glam;

pub use worldrunner_asset_id::{AssetIdentityError, ContentHash, LogicalAssetId};
pub use worldrunner_core_types::{
    AssetCatalogVersion, AssetId, GenerationVersion, SchemaVersion, WorldDimensions, WorldId,
    WorldPosition, WorldSeed, WorldStateHash,
};

/// Asset identity, packaging, and resource formats (engine layer L0).
#[cfg(feature = "assets")]
pub mod assets {
    pub use worldrunner_catalog;
    pub use worldrunner_import_schema;
    pub use worldrunner_material;
    pub use worldrunner_mesh;
    pub use worldrunner_package;
    pub use worldrunner_texture;
}

/// GPU rendering and camera (engine layer L1).
#[cfg(feature = "gfx")]
pub mod gfx {
    pub use worldrunner_camera;
    pub use worldrunner_render_scene;
}

/// Sandboxed Lua host (engine foundation layer).
#[cfg(feature = "script")]
pub use worldrunner_script as script;

/// World schema, simulation, generation, planning, and save (engine layer L2).
#[cfg(feature = "world")]
pub mod world {
    pub use worldrunner_render_plan;
    pub use worldrunner_save;
    pub use worldrunner_world_generation;
    pub use worldrunner_world_model;
    pub use worldrunner_world_runtime;
    pub use worldrunner_world_schema;
}

/// CPU-only live world storage, independent of document preparation and GPU services.
#[cfg(feature = "simulation")]
pub use worldrunner_world_simulation as simulation;

/// CPU-only native UI components, fonts and pathless render planning.
#[cfg(feature = "ui")]
pub mod ui {
    pub use worldrunner_render_plan as plan;
    pub use worldrunner_world_simulation::ui::*;
}

/// Pathless scene planning and GPU capture, without asset lookup or an application host.
///
/// ```no_run
/// use worldrunner::scene::{gpu, plan};
///
/// async fn capture(scene: &plan::RenderScene, view: &gpu::SceneView)
///     -> Result<gpu::offscreen::RgbaImage, Box<dyn std::error::Error>>
/// {
///     let context = gpu::device::GpuContext::request_with_features(gpu::wgpu::Features::empty()).await?;
///     Ok(gpu::offscreen::capture_scene(&context, scene, view)?)
/// }
/// ```
#[cfg(feature = "scene")]
pub mod scene {
    pub use worldrunner_camera as camera;
    pub use worldrunner_render_gpu as resources;
    pub use worldrunner_render_plan as plan;
    pub use worldrunner_render_post as post;
    pub use worldrunner_render_scene as gpu;
    /// Optional desktop host (engine layer L3).
    #[cfg(feature = "scene-viewer")]
    pub use worldrunner_scene_viewer as viewer;
}
