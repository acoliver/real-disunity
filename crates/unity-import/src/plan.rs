//! A scene as the managed loader builds it.
//!
//! A [`ScenePlan`] lists every object of the scene and of the prefabs it references, parents
//! first, with its engine components and scripts in authored order. Assets, sprites, textures,
//! package fonts and input axes the objects use are listed once and referenced by index. Script
//! fields stay untyped, because only the compiled script says how to read them; references in
//! them name plan entries:
//!
//! | JSON | Target |
//! | --- | --- |
//! | `null` | no object |
//! | `{"$object": i}` | `objects[i]`, a `GameObject` |
//! | `{"$transform": i}` | the transform of `objects[i]` |
//! | `{"$component": [i, j]}` | `objects[i].components[j]` |
//! | `{"$asset": i}` | `assets[i]` |
//! | `{"$sprite": i}`, `{"$texture": i}` | `sprites[i]`, `textures[i]` |
//! | `{"$font": "Arial"}` | a package font |
//!
//! Planning fails with the object named for anything the engine cannot build as authored: an
//! unsupported component or setting, a missing reference, or a reference to an asset type the
//! engine has no script API for.

mod components;
mod objects;
mod tilemap;
mod values;

use std::path::Path;

use serde::Serialize;

use crate::{
    components::{
        Camera, CanvasScaler, ColliderShape, ComponentError, Light2D, PhysicsMaterial,
        RectTransform, RendererMaterial, Text, UrpCamera,
    },
    input::{InputAxis, input_axes},
    package::PackageFont,
    project::{ImportError, Project},
    scene::{SceneError, import_scene},
    texture::{Filter, TextureError, Wrap},
};

#[derive(Debug, Serialize)]
pub struct ScenePlan {
    pub scene: String,
    pub objects: Vec<PlanObject>,
    pub assets: Vec<PlanAsset>,
    pub sprites: Vec<PlanSprite>,
    pub textures: Vec<PlanTexture>,
    pub fonts: Vec<PackageFont>,
    pub axes: Vec<InputAxis>,
    /// Layer names by layer index, 32 entries with an empty string for an unnamed layer. Only
    /// player plans carry them; a project plan leaves the list empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<String>,
    /// What the plan leaves out of the scene as authored, one line each. Only player plans
    /// leave anything out; a project plan fails instead.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub differences: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PlanObject {
    pub name: String,
    pub tag: String,
    pub layer: i64,
    pub active: bool,
    /// The parent's index; parents precede their children.
    pub parent: Option<usize>,
    /// Part of a prefab template, kept for `Instantiate` and never activated.
    pub template: bool,
    pub position: [f64; 3],
    pub rotation: [f64; 4],
    pub scale: [f64; 3],
    /// Present when the object's transform is a `RectTransform`.
    pub rect: Option<RectTransform>,
    pub components: Vec<PlanComponent>,
}

/// Settings every 2D collider shares. `material` indexes [`ScenePlan::assets`].
#[derive(Debug, Serialize)]
pub struct PlanCollider {
    pub enabled: bool,
    pub trigger: bool,
    pub offset: [f64; 2],
    pub material: Option<usize>,
    pub shape: ColliderShape,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum PlanComponent {
    Camera(Camera),
    AudioListener {
        enabled: bool,
    },
    #[serde(rename = "GUILayer")]
    GuiLayer {
        enabled: bool,
    },
    FlareLayer {
        enabled: bool,
    },
    SpriteRenderer {
        enabled: bool,
        sorting_order: i64,
        material: RendererMaterial,
        sprite: Option<usize>,
        color: [f64; 4],
        flip_x: bool,
        flip_y: bool,
    },
    Rigidbody2D {
        body_type: crate::components::BodyType,
        simulated: bool,
        mass: f64,
        gravity_scale: f64,
        constraints: u8,
        interpolation: crate::components::Interpolation,
        material: Option<usize>,
    },
    Collider2D(PlanCollider),
    Tilemap(tilemap::PlanTilemap),
    Canvas {
        render_mode: crate::components::RenderMode,
        /// The camera component a screen-space-camera canvas renders through.
        camera: Option<[usize; 2]>,
        plane_distance: f64,
        pixel_perfect: bool,
        sorting_layer_id: i64,
        sorting_order: i64,
        target_display: i64,
    },
    Text {
        enabled: bool,
        #[serde(flatten)]
        text: Text,
    },
    CanvasScaler {
        enabled: bool,
        #[serde(flatten)]
        scaler: CanvasScaler,
    },
    GraphicRaycaster {
        enabled: bool,
    },
    EventSystem {
        enabled: bool,
    },
    StandaloneInputModule {
        enabled: bool,
    },
    Light2D {
        enabled: bool,
        #[serde(flatten)]
        light: Light2D,
    },
    UniversalAdditionalCameraData {
        enabled: bool,
        #[serde(flatten)]
        camera: UrpCamera,
    },
    /// A project script: `class` in the compiled game assembly, from `script`'s file. A
    /// player's script has no file; `class` is its full name in `assembly`.
    Script {
        script: String,
        class: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        assembly: Option<String>,
        enabled: bool,
        fields: serde_json::Value,
    },
    /// An engine component of a player scene that the engine cannot build yet, by class.
    Unsupported {
        class: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum PlanAsset {
    /// A `ScriptableObject` asset of project script `class`.
    Script {
        script: String,
        class: String,
        name: String,
        fields: serde_json::Value,
    },
    PhysicsMaterial2D {
        name: String,
        #[serde(flatten)]
        material: PhysicsMaterial,
    },
}

/// A sprite of [`ScenePlan::textures`]`[texture]`. `physics_shape` is present for sprites that
/// tiles collide with: convex paths in world units around the pivot.
#[derive(Debug, Serialize)]
pub struct PlanSprite {
    pub texture: usize,
    pub name: String,
    pub rect: [f64; 4],
    pub pivot: [f64; 2],
    pub pixels_per_unit: f64,
    pub border: [f64; 4],
    pub physics_shape: Option<Vec<Vec<[f64; 2]>>>,
}

#[derive(Debug, Serialize)]
pub struct PlanTexture {
    pub guid: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub filter: Filter,
    pub wrap: [Wrap; 2],
    pub srgb: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error(transparent)]
    Scene(#[from] SceneError),
    #[error(transparent)]
    Import(#[from] ImportError),
    #[error(transparent)]
    Component(#[from] ComponentError),
    #[error(transparent)]
    Texture(#[from] TextureError),
}

/// Plans the scene at `path`, relative to the project root.
pub fn plan_scene(project: &Project, path: &Path) -> Result<ScenePlan, PlanError> {
    let scene = import_scene(project, path)?;
    let mut planner = objects::Planner::new(project, &scene)?;
    let objects = planner.objects()?;
    let assets = planner.assets()?;
    let (sprites, textures, fonts) = planner.finish();
    Ok(ScenePlan {
        scene: path.display().to_string(),
        objects,
        assets,
        sprites,
        textures,
        fonts,
        axes: input_axes(project)?,
        layers: Vec::new(),
        differences: Vec::new(),
    })
}
