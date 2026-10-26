//! Typed views of the engine components the games use beyond `GameObject` and `Transform`.
//!
//! Each view reads one object of an imported [`Scene`]. Settings the engine does not implement
//! fail with the object's location instead of importing with different behavior.

mod material;
mod physics;
mod tilemap;

use crate::scene::{ObjectId, Origin, Scene};

pub use material::RendererMaterial;
pub use physics::{Combine, PhysicsMaterial};
pub use tilemap::{Grid, Tile, Tilemap};

pub(crate) const RIGIDBODY_2D: i32 = 50;
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
    ) -> Result<&crate::scene::Value, ComponentError> {
        match self.object(id) {
            Some(object) if object.class_id == class_id => Ok(&object.fields),
            _ => Err(self.component_error(id, format!("is not a {name}"))),
        }
    }
}
