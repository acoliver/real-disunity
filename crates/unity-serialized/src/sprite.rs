//! The `Sprite` layout: a rectangle of a texture, its mesh, physics outline and bones.
//!
//! A sprite's index and vertex bytes are returned as ranges within the object.

use std::ops::Range;

use crate::{
    ClassId, SerializedError,
    mesh::{SUB_MESH_LEN, SubMesh, VertexData},
    object::{Layout, ObjectReader, PPtr},
};

/// An extra texture a sprite's material samples, such as a normal map, by shader property name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecondaryTexture {
    pub texture: PPtr,
    pub name: String,
}

/// The mesh and texture placement a sprite draws with.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteRenderData {
    pub texture: PPtr,
    /// The separate alpha texture of a split-alpha compressed atlas, or null.
    pub alpha_texture: PPtr,
    pub secondary_textures: Vec<SecondaryTexture>,
    pub sub_meshes: Vec<SubMesh>,
    /// Byte range of the index buffer within the object.
    pub index_buffer: Range<usize>,
    pub vertex_data: VertexData,
    /// Inverse bind poses for skinned sprites, as row-major 4x4 matrices.
    pub bindposes: Vec<[f32; 16]>,
    /// The sprite's rectangle in the texture, in pixels: x, y, width, height.
    pub texture_rect: [f32; 4],
    /// Offset of the texture rectangle within the sprite rectangle, in pixels.
    pub texture_rect_offset: [f32; 2],
    /// Unity's `atlasRectOffset`; (-1, -1) in every sprite of the player checked, atlased or not.
    pub atlas_rect_offset: [f32; 2],
    /// Unity's packed settings bits: bit 0 packed, bit 1 packing mode, bits 2 to 5 packing
    /// rotation, bit 6 mesh type.
    pub settings: u32,
    /// Pixels per unit and the pivot's position in the texture, in pixels: x scale, x pivot,
    /// y scale, y pivot.
    pub uv_transform: [f32; 4],
    /// Unity's `downscaleMultiplier` for the packed texture.
    pub downscale_multiplier: f32,
}

const MATRIX_LEN: usize = 64;
/// A secondary texture's reference and the length of its name.
const SECONDARY_TEXTURE_MIN: usize = 16;

impl SecondaryTexture {
    /// Reads a length-prefixed list of secondary textures.
    pub(crate) fn read_all(r: &mut ObjectReader<'_>) -> Result<Vec<Self>, SerializedError> {
        r.array("secondary texture", SECONDARY_TEXTURE_MIN, |r| {
            Ok(Self {
                texture: r.pptr()?,
                name: r.string()?,
            })
        })
    }
}

impl SpriteRenderData {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            texture: r.pptr()?,
            alpha_texture: r.pptr()?,
            secondary_textures: SecondaryTexture::read_all(r)?,
            sub_meshes: r.array("sub-mesh", SUB_MESH_LEN, SubMesh::read)?,
            index_buffer: r.byte_range("index buffer byte")?,
            vertex_data: VertexData::read(r)?,
            bindposes: r.array("bind pose", MATRIX_LEN, ObjectReader::floats::<16>)?,
            texture_rect: r.floats()?,
            texture_rect_offset: r.floats()?,
            atlas_rect_offset: r.floats()?,
            settings: r.u32()?,
            uv_transform: r.floats()?,
            downscale_multiplier: r.f32()?,
        })
    }
}

/// One bone of a skinned sprite.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteBone {
    pub name: String,
    pub guid: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub length: f32,
    /// Index of the parent bone, or -1 for a root.
    pub parent: i32,
    /// The bone's editor color as packed RGBA bytes.
    pub color: u32,
}

/// Two string lengths and the fixed fields of a bone.
const BONE_MIN: usize = 4 + 4 + 12 + 16 + 4 + 4 + 4;

impl SpriteBone {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            name: r.string()?,
            guid: r.string()?,
            position: r.floats()?,
            rotation: r.floats()?,
            length: r.f32()?,
            parent: r.i32()?,
            color: r.u32()?,
        })
    }
}

/// The key a sprite's render data is stored under: an asset GUID and a local identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderDataKey {
    pub guid: [u32; 4],
    pub local_id: i64,
}

impl RenderDataKey {
    pub(crate) fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            guid: [r.u32()?, r.u32()?, r.u32()?, r.u32()?],
            local_id: r.i64()?,
        })
    }
}

/// A 2D image drawn from a rectangle of a texture, with its own mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    pub name: String,
    /// The sprite's rectangle in its source texture, in pixels: x, y, width, height.
    pub rect: [f32; 4],
    /// Offset of the pivot from the rectangle's center, in pixels.
    pub offset: [f32; 2],
    /// Nine-slice borders in pixels: left, bottom, right, top.
    pub border: [f32; 4],
    pub pixels_to_units: f32,
    /// The pivot as a fraction of the rectangle.
    pub pivot: [f32; 2],
    /// How far the generated mesh extends beyond the sprite's edge, in pixels (the importer's
    /// Extrude Edges setting).
    pub extrude: u32,
    pub is_polygon: bool,
    pub render_data_key: RenderDataKey,
    pub atlas_tags: Vec<String>,
    pub sprite_atlas: PPtr,
    pub render_data: SpriteRenderData,
    /// Collider outlines, each a closed path of points.
    pub physics_shape: Vec<Vec<[f32; 2]>>,
    pub bones: Vec<SpriteBone>,
    pub scriptable_objects: Vec<PPtr>,
}

/// The length prefix of an empty array.
const ARRAY_MIN: usize = 4;
const POINT_LEN: usize = 8;

impl Layout for Sprite {
    const CLASS: ClassId = ClassId::SPRITE;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let name = r.string()?;
        let rect = r.floats()?;
        let offset = r.floats()?;
        let border = r.floats()?;
        let pixels_to_units = r.f32()?;
        let pivot = r.floats()?;
        let extrude = r.u32()?;
        let is_polygon = r.bool()?;
        r.align()?;
        let render_data_key = RenderDataKey::read(r)?;
        let atlas_tags = r.array("atlas tag", ARRAY_MIN, ObjectReader::string)?;
        let sprite_atlas = r.pptr()?;
        let render_data = SpriteRenderData::read(r)?;
        let physics_shape = r.array("physics shape path", ARRAY_MIN, |r| {
            r.array("physics shape point", POINT_LEN, ObjectReader::floats::<2>)
        })?;
        Ok(Self {
            name,
            rect,
            offset,
            border,
            pixels_to_units,
            pivot,
            extrude,
            is_polygon,
            render_data_key,
            atlas_tags,
            sprite_atlas,
            render_data,
            physics_shape,
            bones: r.array("sprite bone", BONE_MIN, SpriteBone::read)?,
            scriptable_objects: r.pptrs("scriptable object")?,
        })
    }
}

/// Where an atlas placed one packed sprite, and the textures it drew it into.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteAtlasData {
    pub texture: PPtr,
    /// The separate alpha texture of a split-alpha compressed atlas, or null.
    pub alpha_texture: PPtr,
    /// The sprite's rectangle in the atlas texture, in pixels: x, y, width, height.
    pub texture_rect: [f32; 4],
    /// Offset of the texture rectangle within the sprite rectangle, in pixels.
    pub texture_rect_offset: [f32; 2],
    /// Unity's `atlasRectOffset`.
    pub atlas_rect_offset: [f32; 2],
    /// Pixels per unit and the pivot's position in the texture, as for a sprite.
    pub uv_transform: [f32; 4],
    /// Unity's `downscaleMultiplier` for the packed texture.
    pub downscale_multiplier: f32,
    /// Unity's packed settings bits, as for a sprite.
    pub settings: u32,
    pub secondary_textures: Vec<SecondaryTexture>,
}

impl SpriteAtlasData {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            texture: r.pptr()?,
            alpha_texture: r.pptr()?,
            texture_rect: r.floats()?,
            texture_rect_offset: r.floats()?,
            atlas_rect_offset: r.floats()?,
            uv_transform: r.floats()?,
            // Unlike a sprite's render data, the multiplier comes before the settings.
            downscale_multiplier: r.f32()?,
            settings: r.u32()?,
            secondary_textures: SecondaryTexture::read_all(r)?,
        })
    }
}

/// A key, then the fixed fields of the atlas data and the count of its secondary textures.
const ATLAS_ENTRY_MIN: usize = 24 + 12 + 12 + 16 + 8 + 8 + 16 + 4 + 4 + 4;

/// Sprites packed together into shared textures.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteAtlas {
    pub name: String,
    pub packed_sprites: Vec<PPtr>,
    /// The names of the packed sprites, in the order of `packed_sprites`.
    pub packed_sprite_names: Vec<String>,
    /// Each packed sprite's placement, by the sprite's render data key.
    pub render_data: Vec<(RenderDataKey, SpriteAtlasData)>,
    pub tag: String,
    /// Whether this atlas is a variant that scales another atlas's textures.
    pub is_variant: bool,
}

impl Layout for SpriteAtlas {
    const CLASS: ClassId = ClassId::SPRITE_ATLAS;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let atlas = Self {
            name: r.string()?,
            packed_sprites: r.pptrs("packed sprite")?,
            packed_sprite_names: r.array("packed sprite name", ARRAY_MIN, ObjectReader::string)?,
            render_data: r.array("atlas render data", ATLAS_ENTRY_MIN, |r| {
                Ok((RenderDataKey::read(r)?, SpriteAtlasData::read(r)?))
            })?,
            tag: r.string()?,
            is_variant: r.bool()?,
        };
        r.align()?;
        Ok(atlas)
    }
}
