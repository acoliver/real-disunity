//! `Texture2DArray` objects a script field or prefab references, as plan assets.
//!
//! An array plans like a texture: pixels stored in the object are written to
//! `resources/<n>.bin` beside the plan and pixels in a `.resS` file are named in place. The plan
//! keeps the array's size, slice count and `GraphicsFormat` for every format, since the managed
//! side only needs the object and its bytes.

use std::path::Path;

use worldrunner_unity_serialized::graphics::Texture2DArray;

use crate::{
    PlayerError,
    player_layered_textures::pixel_data,
    player_resources::{ObjectKey, Registry, ResourceAsset},
};

/// Plans the texture array, asset number `index`.
pub(crate) fn plan_texture_array(
    registry: &mut Registry,
    key: &ObjectKey,
    output: &Path,
    index: usize,
) -> Result<ResourceAsset, PlayerError> {
    let array: Texture2DArray = registry.files.open_path(&key.0)?.decode(key.1)?;
    let what = format!("texture array {}", array.name);
    let data = pixel_data(
        registry,
        key,
        (&array.image_data, &array.stream),
        (&what, output, index),
    )?;
    let settings = array.settings;
    Ok(ResourceAsset::Texture2DArray {
        name: array.name,
        width: array.width,
        height: array.height,
        depth: array.depth,
        graphics_format: array.format,
        mip_count: array.mip_count,
        readable: array.is_readable,
        color_space: array.color_space,
        filter_mode: settings.filter_mode,
        wrap_u: settings.wrap[0],
        wrap_v: settings.wrap[1],
        wrap_w: settings.wrap[2],
        aniso_level: settings.aniso,
        mip_bias: settings.mip_bias,
        data,
    })
}
