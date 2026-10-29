//! Textures, sprites and sprite atlases of a player's `Resources` as plan assets.
//!
//! A texture names the byte range of its pixels. Pixels stored in the texture's own object
//! are written to `resources/<n>.bin` beside the plan; pixels in a `.resS` file next to the
//! serialized file are named in place. Sprites and atlases reference their textures and
//! sprites as `{"$asset": i}`. A sprite's mesh, physics shape, bones and secondary textures
//! are not planned: the engine draws a sprite as its rectangle.

use std::path::{Path, PathBuf};

use serde::Serialize;
use worldrunner_unity_serialized::{
    graphics::Texture2D,
    object::PPtr,
    sprite::{Sprite, SpriteAtlas},
};

use crate::{
    PlayerError,
    player_files::PlayerFiles,
    player_resources::{ObjectKey, Registry, ResourceAsset},
};

/// A reference to another planned asset, written as `{"$asset": i}`.
#[derive(Debug, Serialize)]
pub struct AssetRef {
    #[serde(rename = "$asset")]
    pub asset: usize,
}

/// The pixels of a texture: `size` bytes at `offset` in `file`, which is relative to the plan's
/// directory for inline pixels and absolute for streamed ones.
#[derive(Debug, Serialize)]
pub struct TextureData {
    pub file: PathBuf,
    pub offset: u64,
    pub size: u64,
}

const SETTINGS_PACKED: u32 = 1;
const SETTINGS_ROTATION_SHIFT: u32 = 2;
const SETTINGS_ROTATION_MASK: u32 = 0xf;
const SETTINGS_MESH_SHIFT: u32 = 6;

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> PlayerError {
    let path = path.to_owned();
    move |source| PlayerError::Io { path, source }
}

fn invalid(file: &Path, reason: String) -> PlayerError {
    PlayerError::InvalidHierarchy {
        path: file.to_owned(),
        reason,
    }
}

/// Writes `bytes` to `relative` under `output`, creating the directory.
pub(crate) fn write_resource(
    output: &Path,
    relative: &str,
    bytes: &[u8],
) -> Result<(), PlayerError> {
    let target = output.join(relative);
    let parent = target.parent().expect("resource files have a directory");
    std::fs::create_dir_all(parent).map_err(io_error(parent))?;
    std::fs::write(&target, bytes).map_err(io_error(&target))
}

/// Why the engine cannot build the texture, as a difference line, or `None` when it can.
/// Only 2D textures with one image are built.
pub(crate) fn texture_flaw(
    files: &mut PlayerFiles,
    key: &ObjectKey,
) -> Result<Option<String>, PlayerError> {
    let (file, path_id) = key;
    let texture: Texture2D = files.open_path(file)?.decode(*path_id)?;
    if texture.image_count == 1 && texture.dimension == 2 {
        return Ok(None);
    }
    Ok(Some(format!(
        "resource texture {}: {} images of dimension {} are not built",
        texture.name, texture.image_count, texture.dimension
    )))
}

/// Plans the texture, asset number `index`.
pub(crate) fn plan_texture(
    registry: &mut Registry,
    key: &ObjectKey,
    output: &Path,
    index: usize,
) -> Result<ResourceAsset, PlayerError> {
    let (file, path_id) = key;
    let open = registry.files.open_path(file)?;
    let texture: Texture2D = open.decode(*path_id)?;
    let data = if texture.stream.path.is_empty() {
        let object = *open.file.object(*path_id).expect("decoded objects exist");
        let bytes = open.bytes(&object)?;
        let pixels = bytes.get(texture.image_data.clone()).ok_or_else(|| {
            invalid(
                file,
                format!("texture {}: image data lies outside it", texture.name),
            )
        })?;
        let relative = format!("resources/{index}.bin");
        write_resource(output, &relative, pixels)?;
        TextureData {
            file: PathBuf::from(relative),
            offset: 0,
            size: pixels.len() as u64,
        }
    } else {
        streamed_data(file, &texture)?
    };
    let settings = texture.settings;
    Ok(ResourceAsset::Texture2D {
        name: texture.name,
        width: texture.width,
        height: texture.height,
        format: texture.format,
        mip_count: texture.mipmaps.count,
        readable: texture.is_readable,
        color_space: texture.color_space,
        filter_mode: settings.filter_mode,
        wrap_u: settings.wrap[0],
        wrap_v: settings.wrap[1],
        wrap_w: settings.wrap[2],
        aniso_level: settings.aniso,
        mip_bias: settings.mip_bias,
        data,
    })
}

/// The pixels a texture keeps in a `.resS` file beside its serialized `file`. The path the
/// texture names ends in the file's name, after any `archive:/` prefix or directories.
fn streamed_data(file: &Path, texture: &Texture2D) -> Result<TextureData, PlayerError> {
    let stream = &texture.stream;
    let name = stream.path.rsplit(['/', '\\']).next().unwrap_or_default();
    let missing = |why: &str| {
        invalid(
            file,
            format!(
                "texture {}: its data in {} ({why}) cannot be found",
                texture.name, stream.path
            ),
        )
    };
    let path = file.parent().unwrap_or(Path::new("")).join(name);
    let length = match std::fs::metadata(&path) {
        Ok(meta) if !name.is_empty() && meta.is_file() => meta.len(),
        _ => return Err(missing("no such file")),
    };
    if stream
        .offset
        .checked_add(u64::from(stream.size))
        .is_none_or(|end| end > length)
    {
        return Err(missing("the range lies outside the file"));
    }
    Ok(TextureData {
        file: std::path::absolute(&path).map_err(io_error(&path))?,
        offset: stream.offset,
        size: u64::from(stream.size),
    })
}

/// Plans the sprite.
pub(crate) fn plan_sprite(
    registry: &mut Registry,
    key: &ObjectKey,
) -> Result<ResourceAsset, PlayerError> {
    let (file, path_id) = key;
    let sprite: Sprite = registry.files.open_path(file)?.decode(*path_id)?;
    let place = format!("resource sprite {}", sprite.name);
    if !sprite.bones.is_empty() {
        registry.notes.push(format!("{place}: bones are not built"));
    }
    if !sprite.render_data.secondary_textures.is_empty() {
        registry
            .notes
            .push(format!("{place}: secondary textures are not built"));
    }
    let placed = placement(registry, file, &sprite)?;
    let texture = asset_ref(
        registry,
        &placed.owner,
        placed.texture,
        &format!("{place}.texture"),
    );
    Ok(ResourceAsset::Sprite {
        rect: sprite.rect,
        pivot: sprite.pivot,
        pixels_per_unit: sprite.pixels_to_units,
        border: sprite.border,
        extrude: sprite.extrude,
        texture,
        texture_rect: placed.texture_rect,
        texture_rect_offset: placed.texture_rect_offset,
        packed: placed.settings & SETTINGS_PACKED != 0,
        packing_rotation: (placed.settings >> SETTINGS_ROTATION_SHIFT) & SETTINGS_ROTATION_MASK,
        mesh_type: (placed.settings >> SETTINGS_MESH_SHIFT) & 1,
        name: sprite.name,
    })
}

/// The texture a sprite's pixels are in, where in it, and the packing settings.
struct Placement {
    /// The file whose references `texture` is relative to.
    owner: PathBuf,
    texture: PPtr,
    texture_rect: [f32; 4],
    texture_rect_offset: [f32; 2],
    settings: u32,
}

/// The sprite's own render data, or, for a sprite packed into an atlas whose own render data
/// names no texture, the atlas's entry for the sprite's render data key, which holds the packed
/// texture and the sprite's place in it.
fn placement(
    registry: &mut Registry,
    file: &Path,
    sprite: &Sprite,
) -> Result<Placement, PlayerError> {
    let data = &sprite.render_data;
    let own = Placement {
        owner: file.to_owned(),
        texture: data.texture,
        texture_rect: data.texture_rect,
        texture_rect_offset: data.texture_rect_offset,
        settings: data.settings,
    };
    if !data.texture.is_null() || sprite.sprite_atlas.is_null() {
        return Ok(own);
    }
    let owner = registry.files.referenced_file(file, sprite.sprite_atlas)?;
    let atlas: SpriteAtlas = registry
        .files
        .open_path(&owner)?
        .decode(sprite.sprite_atlas.path_id)?;
    let Some((_, entry)) = atlas
        .render_data
        .iter()
        .find(|(key, _)| *key == sprite.render_data_key)
    else {
        return Ok(own);
    };
    Ok(Placement {
        owner,
        texture: entry.texture,
        texture_rect: entry.texture_rect,
        texture_rect_offset: entry.texture_rect_offset,
        settings: entry.settings,
    })
}

/// Plans the atlas. A packed sprite the plan cannot hold is left out and noted.
pub(crate) fn plan_atlas(
    registry: &mut Registry,
    key: &ObjectKey,
) -> Result<ResourceAsset, PlayerError> {
    let (file, path_id) = key;
    let atlas: SpriteAtlas = registry.files.open_path(file)?.decode(*path_id)?;
    let sprites = atlas
        .packed_sprites
        .iter()
        .enumerate()
        .filter_map(|(i, &pptr)| {
            asset_ref(
                registry,
                file,
                pptr,
                &format!("resource atlas {}.packed[{i}]", atlas.name),
            )
        })
        .collect();
    Ok(ResourceAsset::SpriteAtlas {
        name: atlas.name,
        tag: atlas.tag,
        is_variant: atlas.is_variant,
        sprites,
    })
}

/// The reference from `owner` to a planned asset, or `None` for a null reference or one the
/// plan does not hold, which is noted at `place`.
fn asset_ref(registry: &mut Registry, owner: &Path, pptr: PPtr, place: &str) -> Option<AssetRef> {
    if pptr.is_null() {
        return None;
    }
    match registry.asset_index(owner, pptr) {
        Ok(asset) => Some(AssetRef { asset }),
        Err(reason) => {
            registry.notes.push(format!("{place}: {reason}"));
            None
        }
    }
}
