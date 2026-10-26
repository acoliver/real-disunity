use super::{ComponentError, RendererMaterial};
use crate::scene::{
    ObjectId, Reference, Scene, Value,
    read::{RGBA, flag, integer, vector},
};

pub(crate) const SPRITE_RENDERER: i32 = 212;
pub(crate) const TILEMAP_RENDERER: i32 = 483_693_784;

/// Sorting and material shared by sprite and tilemap renderers. Only the Default sorting layer
/// exists, so its order is the whole sort key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RendererSettings {
    pub enabled: bool,
    pub sorting_order: i64,
    pub material: RendererMaterial,
}

/// A `SpriteRenderer` in Simple draw mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteRenderer {
    pub renderer: RendererSettings,
    /// The sprite, usually a sub-asset of a texture; `Reference::Null` for none.
    pub sprite: Reference,
    /// Linear RGBA tint.
    pub color: [f64; 4],
    pub flip_x: bool,
    pub flip_y: bool,
}

/// A `TilemapRenderer` in Chunk mode, drawing cells from the bottom left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TilemapRenderer {
    pub renderer: RendererSettings,
}

impl Scene {
    pub fn sprite_renderer(&self, id: ObjectId) -> Result<SpriteRenderer, ComponentError> {
        let fields = self.expect_class(id, SPRITE_RENDERER, "SpriteRenderer")?;
        let error = |message: &str| self.component_error(id, message);
        if integer(fields, "m_DrawMode", 0) != Some(0) {
            return Err(error("only the Simple draw mode is supported"));
        }
        Ok(SpriteRenderer {
            renderer: self.renderer_settings(id, fields)?,
            sprite: fields
                .get("m_Sprite")
                .map_or(Some(Reference::Null), Value::as_reference)
                .ok_or_else(|| error("invalid m_Sprite"))?,
            color: self.field(id, "m_Color", vector(fields, "m_Color", RGBA, [1.0; 4]))?,
            flip_x: self.field(id, "m_FlipX", flag(fields, "m_FlipX", false))?,
            flip_y: self.field(id, "m_FlipY", flag(fields, "m_FlipY", false))?,
        })
    }

    pub fn tilemap_renderer(&self, id: ObjectId) -> Result<TilemapRenderer, ComponentError> {
        let fields = self.expect_class(id, TILEMAP_RENDERER, "TilemapRenderer")?;
        let error = |message: &str| self.component_error(id, message);
        if integer(fields, "m_Mode", 0) != Some(0) {
            return Err(error("only the Chunk render mode is supported"));
        }
        if integer(fields, "m_SortOrder", 0) != Some(0) {
            return Err(error("only the bottom-left sort order is supported"));
        }
        Ok(TilemapRenderer {
            renderer: self.renderer_settings(id, fields)?,
        })
    }

    fn renderer_settings(
        &self,
        id: ObjectId,
        fields: &Value,
    ) -> Result<RendererSettings, ComponentError> {
        let error = |message: &str| self.component_error(id, message);
        if integer(fields, "m_SortingLayerID", 0) != Some(0) {
            return Err(error("only the Default sorting layer is supported"));
        }
        if integer(fields, "m_MaskInteraction", 0) != Some(0) {
            return Err(error("sprite mask interaction is not supported"));
        }
        Ok(RendererSettings {
            enabled: self.field(id, "m_Enabled", flag(fields, "m_Enabled", true))?,
            sorting_order: self.field(
                id,
                "m_SortingOrder",
                integer(fields, "m_SortingOrder", 0),
            )?,
            material: self.renderer_material(id)?,
        })
    }
}
