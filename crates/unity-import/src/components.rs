//! Typed views of the engine components the games use beyond `GameObject` and `Transform`.
//!
//! Each view reads one object of an imported [`Scene`]. Settings the engine does not implement
//! fail with the object's location instead of importing with different behavior.

mod body;
mod camera;
mod group;
mod light;
mod material;
mod physics;
mod renderer;
mod tilemap;
mod ui;
mod urp;

use crate::{
    package::{PackageAsset, PackageScript},
    scene::{ObjectId, Origin, Reference, Scene, Value},
};

pub use body::{BodyType, Collider, Collider2D, ColliderShape, Interpolation, Rigidbody2D};
pub use camera::{Camera, CameraOutput, CameraParticipant, ClearFlags};
pub use group::CanvasGroupInput;
pub use light::Light3D;
pub use material::RendererMaterial;
pub use physics::{Combine, PhysicsMaterial};
pub use renderer::{RendererSettings, SpriteRenderer, TilemapRenderer};
pub use tilemap::{Grid, Tile, Tilemap};
pub use ui::{
    Canvas, CanvasScale, CanvasScaler, FontData, FontStyle, HorizontalOverflow, RectTransform,
    RenderMode, ScreenMatch, Text, TextAnchor, VerticalOverflow,
};
pub use urp::{
    Antialiasing, CameraOverride, CameraRenderType, Light2D, UrpCamera, UrpPostProcessing,
};

pub(crate) const RIGIDBODY_2D: i32 = 50;
pub(crate) const MONO_BEHAVIOUR: i32 = 114;
pub(crate) const CANVAS: i32 = 223;
pub(crate) const PHYSICS_MATERIAL_2D: i32 = 62;
pub(crate) const GRID: i32 = 156_049_354;
pub(crate) const TILEMAP: i32 = 1_839_735_485;

#[derive(Debug, thiserror::Error)]
#[error("{location}: {message}")]
pub struct ComponentError {
    pub object: ObjectId,
    /// The object's type, fileID and the file it came from.
    pub location: String,
    pub message: String,
}

impl Scene {
    pub(crate) fn component_error(
        &self,
        id: ObjectId,
        message: impl Into<String>,
    ) -> ComponentError {
        let location = self.object(id).map_or_else(
            || format!("object {}", id.index()),
            |object| {
                let file = match object.origin {
                    Origin::Scene => self.path().display().to_string(),
                    Origin::Template(guid) | Origin::Asset(guid) => format!("asset {guid}"),
                };
                format!("{} {} in {file}", object.type_name, object.source.file_id)
            },
        );
        ComponentError {
            object: id,
            location,
            message: message.into(),
        }
    }

    /// The object `id` when it has `class_id`.
    pub(crate) fn expect_class(
        &self,
        id: ObjectId,
        class_id: i32,
        name: &str,
    ) -> Result<&Value, ComponentError> {
        match self.object(id) {
            Some(object) if object.class_id == class_id => Ok(&object.fields),
            _ => Err(self.component_error(id, format!("is not a {name}"))),
        }
    }

    /// The fields of a `MonoBehaviour` whose script is the package type `script`.
    pub(crate) fn expect_script(
        &self,
        id: ObjectId,
        script: PackageScript,
    ) -> Result<&Value, ComponentError> {
        let fields = self.expect_class(id, MONO_BEHAVIOUR, "MonoBehaviour")?;
        let expected = Reference::Package(PackageAsset::Script(script));
        if self.script(id) == Some(expected) {
            Ok(fields)
        } else {
            Err(self.component_error(id, format!("is not a {}", script.type_name().1)))
        }
    }

    /// `value`, or an error naming the malformed field `key` of `id`.
    pub(crate) fn field<T>(
        &self,
        id: ObjectId,
        key: &str,
        value: Option<T>,
    ) -> Result<T, ComponentError> {
        value.ok_or_else(|| self.component_error(id, format!("invalid {key}")))
    }
}
