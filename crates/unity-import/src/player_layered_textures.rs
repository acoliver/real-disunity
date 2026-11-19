//! `Cubemap` and `Texture3D` objects a script field, a prefab or a material references, as plan
//! assets.
//!
//! A cube plans like a 2D texture and a volume like a texture array: pixels stored in the object
//! are written to `resources/<n>.bin` beside the plan and pixels in a `.resS` file are named in
//! place. A cube keeps the fields of a 2D texture, with a `TextureFormat`; a volume keeps its
//! size, depth and `GraphicsFormat` for every format, since the managed side only needs the object
//! and its bytes. The bytes of a cube are its six faces in the order +X, -X, +Y, -Y, +Z, -Z, each
//! with its whole mip chain, and the bytes of a volume are its mip levels from the base level
//! down, each with its slices one after the other.

use std::{
    ops::Range,
    path::{Path, PathBuf},
};

use worldrunner_unity_serialized::graphics::{Cubemap, StreamingInfo, Texture3D};

use crate::{
    PlayerError,
    player_resources::{ObjectKey, Registry, ResourceAsset},
    player_sprites::{TextureData, locate_stream, texture_asset, write_resource},
};

/// The faces of a cube.
const CUBE_FACES: i32 = 6;

/// Where the pixels of the object `key` are: written beside the plan when the object holds them,
/// or named in place in the `.resS` file `stream` names.
pub(crate) fn pixel_data(
    registry: &mut Registry,
    key: &ObjectKey,
    (image_data, stream): (&Range<usize>, &StreamingInfo),
    (what, output, index): (&str, &Path, usize),
) -> Result<TextureData, PlayerError> {
    let (file, path_id) = key;
    let open = registry.files.open_path(file)?;
    if !stream.path.is_empty() {
        return locate_stream(file, stream, what);
    }
    let object = *open.file.object(*path_id).expect("decoded objects exist");
    let bytes = open.bytes(&object)?;
    let pixels = bytes
        .get(image_data.clone())
        .ok_or_else(|| PlayerError::InvalidHierarchy {
            path: file.clone(),
            reason: format!("{what}: image data lies outside it"),
        })?;
    let relative = format!("resources/{index}.bin");
    write_resource(output, &relative, pixels)?;
    Ok(TextureData {
        file: PathBuf::from(relative),
        offset: 0,
        size: pixels.len() as u64,
    })
}

/// Why the engine cannot build the cubemap, as a difference line, or `None` when it can: it has
/// six images, which are six faces of a cube, and its faces are square.
pub(crate) fn cubemap_flaw(cube: &Cubemap) -> Option<String> {
    let texture = &cube.texture;
    (texture.image_count != CUBE_FACES || texture.width != texture.height).then(|| {
        format!(
            "resource cubemap {}: {} images of {}x{} are not six square faces",
            texture.name, texture.image_count, texture.width, texture.height
        )
    })
}

/// Plans the cubemap, asset number `index`.
pub(crate) fn plan_cubemap(
    registry: &mut Registry,
    key: &ObjectKey,
    output: &Path,
    index: usize,
) -> Result<ResourceAsset, PlayerError> {
    let cube: Cubemap = registry.files.open_path(&key.0)?.decode(key.1)?;
    let texture = cube.texture;
    let what = format!("cubemap {}", texture.name);
    let data = pixel_data(
        registry,
        key,
        (&texture.image_data, &texture.stream),
        (&what, output, index),
    )?;
    Ok(ResourceAsset::Cubemap {
        texture: texture_asset(texture, data),
    })
}

/// Plans the 3D texture, asset number `index`.
pub(crate) fn plan_texture_3d(
    registry: &mut Registry,
    key: &ObjectKey,
    output: &Path,
    index: usize,
) -> Result<ResourceAsset, PlayerError> {
    let volume: Texture3D = registry.files.open_path(&key.0)?.decode(key.1)?;
    let what = format!("texture {}", volume.name);
    let data = pixel_data(
        registry,
        key,
        (&volume.image_data, &volume.stream),
        (&what, output, index),
    )?;
    let settings = volume.settings;
    Ok(ResourceAsset::Texture3D {
        name: volume.name,
        width: volume.width,
        height: volume.height,
        depth: volume.depth,
        graphics_format: volume.format,
        mip_count: volume.mip_count,
        readable: volume.is_readable,
        color_space: volume.color_space,
        filter_mode: settings.filter_mode,
        wrap_u: settings.wrap[0],
        wrap_v: settings.wrap[1],
        wrap_w: settings.wrap[2],
        aniso_level: settings.aniso,
        mip_bias: settings.mip_bias,
        data,
    })
}
