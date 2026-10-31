//! `Texture2DArray` objects a script field or prefab references, as plan assets.
//!
//! An array plans like a texture: pixels stored in the object are written to
//! `resources/<n>.bin` beside the plan and pixels in a `.resS` file are named in place. The plan
//! keeps the array's size, slice count and `GraphicsFormat` for every format, since the managed
//! side only needs the object and its bytes.

use std::path::{Path, PathBuf};

use worldrunner_unity_serialized::graphics::Texture2DArray;

use crate::{
    PlayerError,
    player_resources::{ObjectKey, Registry, ResourceAsset},
    player_sprites::{TextureData, locate_stream, write_resource},
};

/// Plans the texture array, asset number `index`.
pub(crate) fn plan_texture_array(
    registry: &mut Registry,
    key: &ObjectKey,
    output: &Path,
    index: usize,
) -> Result<ResourceAsset, PlayerError> {
    let (file, path_id) = key;
    let open = registry.files.open_path(file)?;
    let array: Texture2DArray = open.decode(*path_id)?;
    let data = if array.stream.path.is_empty() {
        let object = *open.file.object(*path_id).expect("decoded objects exist");
        let bytes = open.bytes(&object)?;
        let pixels =
            bytes
                .get(array.image_data.clone())
                .ok_or_else(|| PlayerError::InvalidHierarchy {
                    path: file.clone(),
                    reason: format!("texture array {}: image data lies outside it", array.name),
                })?;
        let relative = format!("resources/{index}.bin");
        write_resource(output, &relative, pixels)?;
        TextureData {
            file: PathBuf::from(relative),
            offset: 0,
            size: pixels.len() as u64,
        }
    } else {
        locate_stream(
            file,
            &array.stream,
            &format!("texture array {}", array.name),
        )?
    };
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
