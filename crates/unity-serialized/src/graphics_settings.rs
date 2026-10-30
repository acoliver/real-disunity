//! The `GraphicsSettings` layout: the render pipeline asset a player starts with.
//!
//! This is the layout of Unity 6000.3 players, whose type trees are stripped. The decoder
//! reads the fields before the pipeline reference, skips the quality and tier settings after
//! it and reads the global settings reference that follows them, so the values those
//! settings stand for are not decoded.

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader, PPtr},
};

/// Built-in shader settings before the shader lists: a mode and a shader reference each.
const BUILTIN_SHADER_SETTINGS: usize = 7;
const BUILTIN_SHADER_SETTING_LEN: usize = 4 + 12;

/// Bytes between the sort axis and the global settings reference: stripping, tier and
/// lighting settings the decoder does not read.
const SETTINGS_BETWEEN_LEN: usize = 132;

/// Bytes after the global settings reference, all zero in the Old World player.
const TRAILER_LEN: usize = 12;

/// The render pipeline assets `GraphicsSettings` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphicsSettings {
    /// The render pipeline asset used when no quality level names one, or null.
    pub render_pipeline: PPtr,
    /// The settings asset of the pipeline package, or null.
    pub global_settings: PPtr,
}

impl Layout for GraphicsSettings {
    const CLASS: ClassId = ClassId::GRAPHICS_SETTINGS;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        for _ in 0..BUILTIN_SHADER_SETTINGS {
            r.bytes::<BUILTIN_SHADER_SETTING_LEN>()?;
        }
        // The video shaders include mode.
        r.i32()?;
        r.pptrs("always included shader")?;
        r.pptrs("preloaded shader")?;
        r.i32()?;
        let _sprites_default_material = r.pptr()?;
        let render_pipeline = r.pptr()?;
        // The transparency sort mode and axis, then the two default rendering paths.
        r.i32()?;
        r.floats::<3>()?;
        r.i32()?;
        r.i32()?;
        r.bytes::<SETTINGS_BETWEEN_LEN>()?;
        let global_settings = r.pptr()?;
        r.bytes::<TRAILER_LEN>()?;
        Ok(Self {
            render_pipeline,
            global_settings,
        })
    }
}
