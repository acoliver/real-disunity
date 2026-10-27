//! Layouts of the classes a renderer reads: mesh filters and renderers, materials and textures.

use std::ops::Range;

use crate::{
    ClassId, SerializedError,
    mesh::Aabb,
    object::{Layout, ObjectReader, PPTR_LEN, PPtr},
};

/// Fewest bytes of a serialized string: its 32-bit length.
const STRING_LEN: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshFilter {
    pub game_object: PPtr,
    pub mesh: PPtr,
}

impl Layout for MeshFilter {
    const CLASS: ClassId = ClassId::MESH_FILTER;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            game_object: r.pptr()?,
            mesh: r.pptr()?,
        })
    }
}

/// The byte-sized rendering options of every renderer, in serialized order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RendererSettings {
    pub cast_shadows: u8,
    pub receive_shadows: u8,
    pub dynamic_occludee: u8,
    pub static_shadow_caster: u8,
    pub motion_vectors: u8,
    pub light_probe_usage: u8,
    pub reflection_probe_usage: u8,
    pub ray_tracing_mode: u8,
    pub ray_trace_procedural: u8,
    pub ray_tracing_build_flags_override: u8,
    pub ray_tracing_build_flags: u8,
    pub small_mesh_culling: u8,
}

impl RendererSettings {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            cast_shadows: r.u8()?,
            receive_shadows: r.u8()?,
            dynamic_occludee: r.u8()?,
            static_shadow_caster: r.u8()?,
            motion_vectors: r.u8()?,
            light_probe_usage: r.u8()?,
            reflection_probe_usage: r.u8()?,
            ray_tracing_mode: r.u8()?,
            ray_trace_procedural: r.u8()?,
            ray_tracing_build_flags_override: r.u8()?,
            ray_tracing_build_flags: r.u8()?,
            small_mesh_culling: r.u8()?,
        })
    }
}

/// The fields every renderer class starts with.
#[derive(Clone, Debug, PartialEq)]
pub struct Renderer {
    pub game_object: PPtr,
    pub enabled: bool,
    pub settings: RendererSettings,
    pub force_mesh_lod: i16,
    pub mesh_lod_selection_bias: f32,
    pub rendering_layer_mask: u32,
    pub renderer_priority: i32,
    pub lightmap_index: u16,
    pub lightmap_index_dynamic: u16,
    pub lightmap_tiling_offset: [f32; 4],
    pub lightmap_tiling_offset_dynamic: [f32; 4],
    pub materials: Vec<PPtr>,
    /// First sub-mesh and sub-mesh count of the static batch this renderer belongs to.
    pub static_batch: [u16; 2],
    pub static_batch_root: PPtr,
    pub probe_anchor: PPtr,
    pub light_probe_volume_override: PPtr,
    pub sorting_layer_id: i32,
    pub sorting_layer: i16,
    pub sorting_order: i16,
    pub mask_interaction: i32,
}

impl Renderer {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let game_object = r.pptr()?;
        let enabled = r.bool()?;
        let settings = RendererSettings::read(r)?;
        r.align()?;
        let force_mesh_lod = r.i16()?;
        r.align()?;
        Ok(Self {
            game_object,
            enabled,
            settings,
            force_mesh_lod,
            mesh_lod_selection_bias: r.f32()?,
            rendering_layer_mask: r.u32()?,
            renderer_priority: r.i32()?,
            lightmap_index: r.u16()?,
            lightmap_index_dynamic: r.u16()?,
            lightmap_tiling_offset: r.floats()?,
            lightmap_tiling_offset_dynamic: r.floats()?,
            materials: r.pptrs("material")?,
            static_batch: [r.u16()?, r.u16()?],
            static_batch_root: r.pptr()?,
            probe_anchor: r.pptr()?,
            light_probe_volume_override: r.pptr()?,
            sorting_layer_id: r.i32()?,
            sorting_layer: r.i16()?,
            sorting_order: r.i16()?,
            mask_interaction: r.i32()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshRenderer {
    pub renderer: Renderer,
    pub additional_vertex_streams: PPtr,
    pub enlighten_vertex_stream: PPtr,
}

impl Layout for MeshRenderer {
    const CLASS: ClassId = ClassId::MESH_RENDERER;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            renderer: Renderer::read(r)?,
            additional_vertex_streams: r.pptr()?,
            enlighten_vertex_stream: r.pptr()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkinnedMeshRenderer {
    pub renderer: Renderer,
    /// Most bones that may affect one vertex; 0 uses the quality setting.
    pub quality: i32,
    pub update_when_offscreen: bool,
    pub skinned_motion_vectors: bool,
    pub mesh: PPtr,
    pub bones: Vec<PPtr>,
    pub blend_shape_weights: Vec<f32>,
    pub root_bone: PPtr,
    pub aabb: Aabb,
    pub dirty_aabb: bool,
}

impl Layout for SkinnedMeshRenderer {
    const CLASS: ClassId = ClassId::SKINNED_MESH_RENDERER;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let renderer = Renderer::read(r)?;
        let quality = r.i32()?;
        let [update_when_offscreen, skinned_motion_vectors] = [r.bool()?, r.bool()?];
        r.align()?;
        let value = Self {
            renderer,
            quality,
            update_when_offscreen,
            skinned_motion_vectors,
            mesh: r.pptr()?,
            bones: r.pptrs("bone")?,
            blend_shape_weights: r.array("blend shape weight", 4, ObjectReader::f32)?,
            root_bone: r.pptr()?,
            aabb: Aabb::read(r)?,
            dirty_aabb: r.bool()?,
        };
        r.align()?;
        Ok(value)
    }
}

/// A texture slot of a material: the texture and its UV scale and offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexEnv {
    pub texture: PPtr,
    pub scale: [f32; 2],
    pub offset: [f32; 2],
}

/// A material's property values by property name, in serialized order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedProperties {
    pub textures: Vec<(String, TexEnv)>,
    pub ints: Vec<(String, i32)>,
    pub floats: Vec<(String, f32)>,
    /// Colors as r, g, b, a.
    pub colors: Vec<(String, [f32; 4])>,
}

impl SavedProperties {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            textures: r.array("texture property", STRING_LEN + PPTR_LEN + 16, |r| {
                let name = r.string()?;
                let texture = r.pptr()?;
                Ok((
                    name,
                    TexEnv {
                        texture,
                        scale: r.floats()?,
                        offset: r.floats()?,
                    },
                ))
            })?,
            ints: r.array("int property", STRING_LEN + 4, |r| {
                Ok((r.string()?, r.i32()?))
            })?,
            floats: r.array("float property", STRING_LEN + 4, |r| {
                Ok((r.string()?, r.f32()?))
            })?,
            colors: r.array("color property", STRING_LEN + 16, |r| {
                Ok((r.string()?, r.floats()?))
            })?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    pub shader: PPtr,
    pub valid_keywords: Vec<String>,
    pub invalid_keywords: Vec<String>,
    pub lightmap_flags: u32,
    pub enable_instancing_variants: bool,
    pub double_sided_gi: bool,
    /// -1 uses the shader's queue.
    pub custom_render_queue: i32,
    pub tags: Vec<(String, String)>,
    pub disabled_shader_passes: Vec<String>,
    pub properties: SavedProperties,
    /// Texture stack group and item names.
    pub build_texture_stacks: Vec<(String, String)>,
}

fn strings(r: &mut ObjectReader<'_>, what: &'static str) -> Result<Vec<String>, SerializedError> {
    r.array(what, STRING_LEN, ObjectReader::string)
}

fn string_pairs(
    r: &mut ObjectReader<'_>,
    what: &'static str,
) -> Result<Vec<(String, String)>, SerializedError> {
    r.array(what, 2 * STRING_LEN, |r| Ok((r.string()?, r.string()?)))
}

impl Layout for Material {
    const CLASS: ClassId = ClassId::MATERIAL;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let name = r.string()?;
        let shader = r.pptr()?;
        let valid_keywords = strings(r, "valid keyword")?;
        let invalid_keywords = strings(r, "invalid keyword")?;
        let lightmap_flags = r.u32()?;
        let enable_instancing_variants = r.bool()?;
        let double_sided_gi = r.bool()?;
        r.align()?;
        Ok(Self {
            name,
            shader,
            valid_keywords,
            invalid_keywords,
            lightmap_flags,
            enable_instancing_variants,
            double_sided_gi,
            custom_render_queue: r.i32()?,
            tags: string_pairs(r, "tag")?,
            disabled_shader_passes: strings(r, "disabled shader pass")?,
            properties: SavedProperties::read(r)?,
            build_texture_stacks: string_pairs(r, "texture stack")?,
        })
    }
}

/// Mipmap fields of a texture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mipmaps {
    pub stripped: i32,
    pub count: i32,
    pub ignore_limit: bool,
    pub limit_group: String,
    pub streaming: bool,
    pub streaming_priority: i32,
}

/// Sampler settings of a texture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureSettings {
    pub filter_mode: i32,
    pub aniso: i32,
    pub mip_bias: f32,
    /// Wrap modes for U, V and W.
    pub wrap: [i32; 3],
}

impl TextureSettings {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            filter_mode: r.i32()?,
            aniso: r.i32()?,
            mip_bias: r.f32()?,
            wrap: [r.i32()?, r.i32()?, r.i32()?],
        })
    }
}

/// Where a texture's pixels live when they are not inside the object: a byte range of a
/// `.resS` file next to the serialized file. An empty `path` means the data is inline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamingInfo {
    pub offset: u64,
    pub size: u32,
    pub path: String,
}

impl StreamingInfo {
    pub(crate) fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            offset: r.u64()?,
            size: r.u32()?,
            path: r.string()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Texture2D {
    pub name: String,
    pub is_alpha_channel_optional: bool,
    pub width: i32,
    pub height: i32,
    pub complete_image_size: u32,
    /// Unity's `TextureFormat` value, for example 12 for DXT5 (BC3).
    pub format: i32,
    pub is_readable: bool,
    pub is_pre_processed: bool,
    pub mipmaps: Mipmaps,
    pub image_count: i32,
    pub dimension: i32,
    pub settings: TextureSettings,
    pub lightmap_format: i32,
    pub color_space: i32,
    /// Byte range of the platform blob within the object.
    pub platform_blob: Range<usize>,
    /// Byte range of inline image data within the object; empty when the data is streamed.
    pub image_data: Range<usize>,
    pub stream: StreamingInfo,
}

impl Layout for Texture2D {
    const CLASS: ClassId = ClassId::TEXTURE_2D;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let name = r.string()?;
        let is_alpha_channel_optional = r.bool()?;
        r.align()?;
        let [width, height] = [r.i32()?, r.i32()?];
        let complete_image_size = r.u32()?;
        let [stripped, format, count] = [r.i32()?, r.i32()?, r.i32()?];
        let [is_readable, is_pre_processed, ignore_limit] = [r.bool()?, r.bool()?, r.bool()?];
        r.align()?;
        let limit_group = r.string()?;
        let streaming = r.bool()?;
        r.align()?;
        let mipmaps = Mipmaps {
            stripped,
            count,
            ignore_limit,
            limit_group,
            streaming,
            streaming_priority: r.i32()?,
        };
        r.align()?;
        Ok(Self {
            name,
            is_alpha_channel_optional,
            width,
            height,
            complete_image_size,
            format,
            is_readable,
            is_pre_processed,
            mipmaps,
            image_count: r.i32()?,
            dimension: r.i32()?,
            settings: TextureSettings::read(r)?,
            lightmap_format: r.i32()?,
            color_space: r.i32()?,
            platform_blob: r.byte_range("platform blob byte")?,
            image_data: r.byte_range("image data byte")?,
            stream: StreamingInfo::read(r)?,
        })
    }
}
