use super::ComponentError;
use crate::{
    guid::Guid,
    scene::{ObjectId, Reference, Scene, Value},
};

/// fileID of Unity's built-in `Sprites-Default` material in the default resources.
const SPRITES_DEFAULT: i64 = 10_754;
const DEFAULT_RESOURCES: Guid = Guid::from_u128(0x0000_0000_0000_0000_f000_0000_0000_0000);

/// The material a sprite or tilemap renderer draws with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererMaterial {
    /// Unity's built-in unlit, alpha-blended sprite material.
    SpritesDefault,
}

impl Scene {
    /// Resolves a renderer's single `m_Materials` entry. Custom and absent materials fail, so a
    /// game is never drawn with a material it did not author.
    pub fn renderer_material(
        &self,
        renderer: ObjectId,
    ) -> Result<RendererMaterial, ComponentError> {
        let fields = &self
            .object(renderer)
            .ok_or_else(|| self.component_error(renderer, "is not in the scene"))?
            .fields;
        let error = |message: String| self.component_error(renderer, message);
        let materials = fields
            .get("m_Materials")
            .and_then(Value::as_sequence)
            .ok_or_else(|| error("has no m_Materials".into()))?;
        let [material] = materials else {
            return Err(error(format!(
                "has {} materials; exactly one is supported",
                materials.len()
            )));
        };
        match material.as_reference() {
            Some(Reference::External { guid, file_id })
                if guid == DEFAULT_RESOURCES && file_id == SPRITES_DEFAULT =>
            {
                Ok(RendererMaterial::SpritesDefault)
            }
            Some(Reference::Missing { guid, file_id }) => Err(error(format!(
                "material {file_id} in {} is not in the project",
                guid.map_or_else(|| "this file".to_owned(), |guid| guid.to_string())
            ))),
            Some(Reference::Null) | None => Err(error("has no material".into())),
            Some(other) => Err(error(format!("custom material {other:?} is not supported"))),
        }
    }
}
