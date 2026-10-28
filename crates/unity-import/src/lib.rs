//! Offline import of Unity projects into engine content.
//!
//! The importer reads a Unity project's text-serialized scenes, prefabs, assets and `.meta`
//! files without modifying them. This crate is a content tool: it has no runtime, simulation or
//! renderer dependency.

pub mod components;
pub mod guid;
pub mod input;
pub mod package;
pub mod plan;
pub mod player;
pub mod player_files;
mod player_hierarchy;
pub mod player_objects;
pub mod player_scene;
pub mod player_scripts;
pub mod player_shaders;
pub mod project;
pub mod scene;
pub mod script_layouts;
pub mod shader_prep;
pub mod texture;
pub mod yaml;

pub use components::{
    Antialiasing, BodyType, Camera, CameraOverride, CameraParticipant, CameraRenderType, Canvas,
    CanvasScale, CanvasScaler, ClearFlags, Collider, Collider2D, ColliderShape, Combine,
    ComponentError, FontData, FontStyle, Grid, HorizontalOverflow, Interpolation, Light2D,
    PhysicsMaterial, RectTransform, RenderMode, RendererMaterial, RendererSettings, Rigidbody2D,
    ScreenMatch, SpriteRenderer, Text, TextAnchor, Tile, Tilemap, TilemapRenderer, UrpCamera,
    UrpPostProcessing, VerticalOverflow,
};
pub use guid::{Guid, ObjectRef, ReferenceError};
pub use input::{AxisKind, InputAxis, input_axes};
pub use package::{PackageAsset, PackageFont, PackageMaterial, PackageScript, PackageSprite};
pub use plan::{
    PlanAsset, PlanCollider, PlanComponent, PlanError, PlanObject, PlanSprite, PlanTexture,
    ScenePlan, plan_scene,
};
pub use player::{PlayerError, PlayerFile, is_serialized_name, read_metadata, read_player};
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
