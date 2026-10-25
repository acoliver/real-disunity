//! Offline import of Unity projects into engine content.
//!
//! The importer reads a Unity project's text-serialized scenes, prefabs, assets and `.meta`
//! files without modifying them. This crate is a content tool: it has no runtime, simulation or
//! renderer dependency.

pub mod components;
pub mod guid;
pub mod project;
pub mod scene;
pub mod texture;
pub mod yaml;

pub use components::{
    Combine, ComponentError, Grid, PhysicsMaterial, RendererMaterial, Tile, Tilemap,
};
pub use guid::{Guid, ObjectRef, ReferenceError};
pub use project::{Asset, ImportError, Project};
pub use scene::{
    GameObject, Object, ObjectId, Origin, Reference, Scene, SceneError, Source, Transform, Value,
    import_scene,
};
pub use texture::{
    Filter, Image, Sprite, SpriteMode, Texture, TextureError, TextureSettings, Textures, Wrap,
    import_texture,
};
pub use yaml::{Document, Limits, Mapping, Node, Scalar, ScalarStyle, YamlError};
