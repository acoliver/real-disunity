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
    graphics::{StreamingInfo, Texture2D},
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

/// A 2D texture as a plan holds it, in resource plans and scene plans alike. `format` is the
/// `TextureFormat` value and `color_space` the decoded value; `filter_mode` and the wrap modes
/// are the sampler's values.
#[derive(Debug, Serialize)]
pub struct Texture2DAsset {
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub format: i32,
    pub mip_count: i32,
    pub readable: bool,
    pub color_space: i32,
    pub filter_mode: i32,
    pub wrap_u: i32,
    pub wrap_v: i32,
    pub wrap_w: i32,
    pub aniso_level: i32,
    pub mip_bias: f32,
    pub data: TextureData,
}

/// A rectangle of a texture, in resource plans and scene plans alike. Rectangles and the border
/// are in pixels (x, y, width, height and left, bottom, right, top); the pivot is a fraction of
/// the rectangle. `packing_rotation` and `mesh_type` are the sprite's packed settings bits.
#[derive(Debug, Serialize)]
pub struct SpriteAsset {
    pub name: String,
    pub rect: [f32; 4],
    pub pivot: [f32; 2],
    pub pixels_per_unit: f32,
    pub border: [f32; 4],
    pub extrude: u32,
    pub texture: Option<AssetRef>,
    pub texture_rect: [f32; 4],
    pub texture_rect_offset: [f32; 2],
    pub packed: bool,
    pub packing_rotation: u32,
    pub mesh_type: u32,
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
    Ok(ResourceAsset::Texture2D {
        texture: texture_asset(texture, data),
    })
}

/// The texture's planned fields over the pixels at `data`.
fn texture_asset(texture: Texture2D, data: TextureData) -> Texture2DAsset {
    let settings = texture.settings;
    Texture2DAsset {
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
    }
}

/// Plans a texture for a scene plan, which has no directory of its own: pixels inside the
/// texture's object are named in place in the serialized file (an absolute path), and pixels in
/// a `.resS` file as for [`plan_texture`].
pub(crate) fn plan_scene_texture(
    files: &mut PlayerFiles,
    key: &ObjectKey,
) -> Result<Texture2DAsset, PlayerError> {
    let (file, path_id) = key;
    let open = files.open_path(file)?;
    let texture: Texture2D = open.decode(*path_id)?;
    let data = if texture.stream.path.is_empty() {
        let object = *open.file.object(*path_id).expect("decoded objects exist");
        let range = texture.image_data.clone();
        if range.end > object.size as usize {
            return Err(invalid(
                file,
                format!("texture {}: image data lies outside it", texture.name),
            ));
        }
        TextureData {
            file: std::path::absolute(file).map_err(io_error(file))?,
            offset: object.offset + range.start as u64,
            size: range.len() as u64,
        }
    } else {
        streamed_data(file, &texture)?
    };
    Ok(texture_asset(texture, data))
}

/// The pixels a texture keeps in a `.resS` file beside its serialized `file`.
fn streamed_data(file: &Path, texture: &Texture2D) -> Result<TextureData, PlayerError> {
    locate_stream(file, &texture.stream, &format!("texture {}", texture.name))
}

/// The bytes the object `what` keeps in a `.resS` file beside its serialized `file`. The path
/// the object names ends in the file's name, after any `archive:/` prefix or directories.
pub(crate) fn locate_stream(
    file: &Path,
    stream: &StreamingInfo,
    what: &str,
) -> Result<TextureData, PlayerError> {
    let name = stream.path.rsplit(['/', '\\']).next().unwrap_or_default();
    let missing = |why: &str| {
        invalid(
            file,
            format!(
                "{what}: its data in {} ({why}) cannot be found",
                stream.path
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
    let parts = read_sprite(&mut registry.files, key)?;
    registry.notes.extend(parts.notes.iter().cloned());
    let texture = asset_ref(
        registry,
        &parts.placed.owner,
        parts.placed.texture,
        &format!("{}.texture", parts.place),
    );
    Ok(ResourceAsset::Sprite {
        sprite: parts.into_asset(texture),
    })
}

/// A decoded sprite, before its texture is planned.
pub(crate) struct SpriteParts {
    sprite: Sprite,
    /// `resource sprite <name>`, which names the sprite in differences.
    place: String,
    /// What the plan leaves out of the sprite.
    pub(crate) notes: Vec<String>,
    placed: Placement,
}

impl SpriteParts {
    /// The file the texture reference is relative to, and the reference.
    pub(crate) fn texture(&self) -> (&Path, PPtr) {
        (&self.placed.owner, self.placed.texture)
    }

    /// The sprite over its planned texture, `None` when it has none.
    pub(crate) fn into_asset(self, texture: Option<AssetRef>) -> SpriteAsset {
        let (sprite, placed) = (self.sprite, self.placed);
        SpriteAsset {
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
        }
    }
}

/// Decodes the sprite and finds the texture it draws.
pub(crate) fn read_sprite(
    files: &mut PlayerFiles,
    key: &ObjectKey,
) -> Result<SpriteParts, PlayerError> {
    let (file, path_id) = key;
    let sprite: Sprite = files.open_path(file)?.decode(*path_id)?;
    let place = format!("resource sprite {}", sprite.name);
    let mut notes = Vec::new();
    if !sprite.bones.is_empty() {
        notes.push(format!("{place}: bones are not built"));
    }
    if !sprite.render_data.secondary_textures.is_empty() {
        notes.push(format!("{place}: secondary textures are not built"));
    }
    let placed = placement(files, file, &sprite)?;
    Ok(SpriteParts {
        sprite,
        place,
        notes,
        placed,
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
    files: &mut PlayerFiles,
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
    let owner = files.referenced_file(file, sprite.sprite_atlas)?;
    let atlas: SpriteAtlas = files
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
