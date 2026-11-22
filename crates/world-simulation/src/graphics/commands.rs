//! The work command buffers record: render targets, clears, global values and temporary render
//! textures. The world validates and orders these records like the rest of the offscreen list; a
//! host applies them in order. A recorded global value or keyword is not folded into the global
//! shader state, so it lasts only for the take that carries it. The ABI records a global only for a
//! temporary render texture, which has no identity beyond its take; every other global an executed
//! command buffer sets is a direct write, which the state keeps and the world orders in front of
//! the next record. Those ordered records are the other source of `SetGlobal` and
//! `SetGlobalKeyword` work.
use super::{GraphicsError, OffscreenWork, PropertyValue, RenderTextureId, properties};
use crate::{World, WorldError};

/// Most color targets one `SetRenderTarget` names.
pub const MAX_COLOR_TARGETS: usize = 8;

/// What a command buffer draws into or reads from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetRef {
    /// A render texture of the world.
    Render(RenderTextureId),
    /// A temporary render texture by the property name id it was allocated under. The world does
    /// not track temporaries: an allocation may come from a buffer recorded earlier or later, so
    /// the host resolves the name when it applies the record.
    Temporary(i32),
    /// The target of the camera being rendered, or the screen when no camera renders.
    CameraTarget,
    /// No target.
    None,
}

impl World {
    /// Checks a command record; other work is the offscreen list's own to check.
    pub(super) fn check_command(&self, work: &OffscreenWork) -> Result<(), WorldError> {
        match work {
            OffscreenWork::SetRenderTarget {
                color,
                depth,
                face,
                slice,
                ..
            } => self.check_set_target(color, depth.as_ref(), *face, *slice),
            OffscreenWork::ClearRenderTarget {
                value, depth_value, ..
            } => {
                if value.iter().chain([depth_value]).all(|v| v.is_finite()) {
                    Ok(())
                } else {
                    Err(GraphicsError::NotFinite.into())
                }
            }
            OffscreenWork::SetGlobal(value) => self.check_work_global(value),
            OffscreenWork::SetGlobalKeyword { name, .. } => {
                properties::check_name(name).map_err(Into::into)
            }
            OffscreenWork::AllocateTemporary { desc, .. } => desc.validate().map_err(Into::into),
            _ => Ok(()),
        }
    }

    /// Checks a global value recorded work sets, which may name a temporary render texture.
    fn check_work_global(&self, value: &PropertyValue) -> Result<(), WorldError> {
        value.validate()?;
        match value {
            PropertyValue::Texture(_, slot) => self.check_work_texture(&slot.texture),
            _ => Ok(()),
        }
    }

    fn check_set_target(
        &self,
        color: &[TargetRef],
        depth: Option<&TargetRef>,
        face: i32,
        slice: i32,
    ) -> Result<(), WorldError> {
        let none = color.is_empty() && depth.is_none();
        if none || color.len() > MAX_COLOR_TARGETS || !(-1..=5).contains(&face) || slice < -1 {
            return Err(GraphicsError::InvalidOffscreen.into());
        }
        for target in color.iter().chain(depth) {
            if let TargetRef::Render(id) = target {
                self.render_texture(*id)?;
            }
        }
        Ok(())
    }
}
