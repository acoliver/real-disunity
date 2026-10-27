//! The `Shader` layout: properties, sub-shaders and passes, and the compressed blob that
//! holds each platform's compiled programs.
//!
//! A pass names its programs per stage; each variant names a blob entry by index. The blob
//! itself is returned as a byte range; decompressing it and reading entries is a later step.

use std::ops::Range;

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader, PPtr},
    shader_program::Program,
    shader_state::{ShaderState, string_pairs},
};

/// Fewest bytes of a serialized string: its 32-bit length.
const STRING_LEN: usize = 4;

/// A material property the shader declares.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderProperty {
    pub name: String,
    pub description: String,
    pub attributes: Vec<String>,
    /// Unity's `ShaderPropertyType`: color, vector, float, range, texture or int.
    pub kind: i32,
    pub flags: u32,
    /// Default value; a range property stores its default, minimum and maximum.
    pub default_value: [f32; 4],
    /// Name of the default texture, such as `white`, for texture properties.
    pub default_texture: String,
    /// Unity's `TextureDimension` of a texture property.
    pub texture_dimension: i32,
}

impl ShaderProperty {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            name: r.string()?,
            description: r.string()?,
            attributes: r.array("property attribute", STRING_LEN, ObjectReader::string)?,
            kind: r.i32()?,
            flags: r.u32()?,
            default_value: r.floats()?,
            default_texture: r.string()?,
            texture_dimension: r.i32()?,
        })
    }
}

/// A pass's programs, one per stage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StagePrograms {
    pub vertex: Program,
    pub fragment: Program,
    pub geometry: Program,
    pub hull: Program,
    pub domain: Program,
    pub ray_tracing: Program,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pass {
    /// Names the pass's parameters use, with their indices.
    pub name_indices: Vec<(String, i32)>,
    /// Unity's pass type: 0 normal, 1 `UsePass`, 2 `GrabPass`.
    pub kind: i32,
    pub state: ShaderState,
    /// Which stages have programs, one bit per stage.
    pub program_mask: u32,
    pub programs: StagePrograms,
    pub has_instancing_variant: bool,
    pub has_procedural_instancing_variant: bool,
    /// The pass a `UsePass` pass refers to.
    pub use_name: String,
    pub name: String,
    /// The texture a `GrabPass` writes.
    pub texture_name: String,
    /// Pass tags, such as `LightMode`.
    pub tags: Vec<(String, String)>,
}

impl Pass {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let name_indices = r.array("name index", STRING_LEN + 4, |r| {
            Ok((r.string()?, r.i32()?))
        })?;
        let kind = r.i32()?;
        let state = ShaderState::read(r)?;
        let program_mask = r.u32()?;
        let programs = StagePrograms {
            vertex: Program::read(r)?,
            fragment: Program::read(r)?,
            geometry: Program::read(r)?,
            hull: Program::read(r)?,
            domain: Program::read(r)?,
            ray_tracing: Program::read(r)?,
        };
        let [has_instancing_variant, has_procedural_instancing_variant] = [r.bool()?, r.bool()?];
        r.align()?;
        Ok(Self {
            name_indices,
            kind,
            state,
            program_mask,
            programs,
            has_instancing_variant,
            has_procedural_instancing_variant,
            use_name: r.string()?,
            name: r.string()?,
            texture_name: r.string()?,
            tags: string_pairs(r, "pass tag")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubShader {
    pub passes: Vec<Pass>,
    /// Sub-shader tags, such as `RenderPipeline` and `Queue`.
    pub tags: Vec<(String, String)>,
    pub lod: i32,
}

/// The shader as parsed from source: everything except compiled code.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedShader {
    pub properties: Vec<ShaderProperty>,
    pub sub_shaders: Vec<SubShader>,
    /// Keywords the variants select by index.
    pub keyword_names: Vec<String>,
    pub keyword_flags: Vec<u8>,
    pub name: String,
    pub custom_editor: String,
    pub fallback: String,
    /// Shaders this one uses for named roles, as role and shader name.
    pub dependencies: Vec<(String, String)>,
    /// Custom editors per render pipeline, as editor and pipeline type names.
    pub pipeline_editors: Vec<(String, String)>,
    pub disable_no_sub_shaders_message: bool,
}

impl ParsedShader {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let properties = r.array("shader property", 7 * 4 + 4 * 4, ShaderProperty::read)?;
        let sub_shaders = r.array("sub-shader", 12, |r| {
            Ok(SubShader {
                passes: r.array("pass", 64, Pass::read)?,
                tags: string_pairs(r, "sub-shader tag")?,
                lod: r.i32()?,
            })
        })?;
        let parsed = Self {
            properties,
            sub_shaders,
            keyword_names: r.array("keyword name", STRING_LEN, ObjectReader::string)?,
            keyword_flags: r.array("keyword flag", 1, ObjectReader::u8)?,
            name: r.string()?,
            custom_editor: r.string()?,
            fallback: r.string()?,
            dependencies: string_pairs(r, "shader dependency")?,
            pipeline_editors: string_pairs(r, "pipeline editor")?,
            disable_no_sub_shaders_message: r.bool()?,
        };
        r.align()?;
        Ok(parsed)
    }
}

/// The compressed blob of compiled programs and where each platform's entries are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledBlobs {
    /// Unity's `ShaderCompilerPlatform` of each platform.
    pub platforms: Vec<u32>,
    /// Per platform, offset of each compressed chunk in the blob.
    pub offsets: Vec<Vec<u32>>,
    pub compressed_lengths: Vec<Vec<u32>>,
    pub decompressed_lengths: Vec<Vec<u32>>,
    /// Byte range of the compressed blob within the object.
    pub blob: Range<usize>,
    pub stage_counts: Vec<u32>,
}

fn per_platform(
    r: &mut ObjectReader<'_>,
    what: &'static str,
) -> Result<Vec<Vec<u32>>, SerializedError> {
    r.array(what, 4, |r| r.array(what, 4, ObjectReader::u32))
}

impl CompiledBlobs {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            platforms: r.array("platform", 4, ObjectReader::u32)?,
            offsets: per_platform(r, "blob offset")?,
            compressed_lengths: per_platform(r, "compressed length")?,
            decompressed_lengths: per_platform(r, "decompressed length")?,
            blob: r.byte_range("compressed blob byte")?,
            stage_counts: r.array("stage count", 4, ObjectReader::u32)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shader {
    /// The object's name; [`ParsedShader::name`] is the name the shader source declares.
    pub name: String,
    pub parsed: ParsedShader,
    pub compiled: CompiledBlobs,
    /// Shaders this one depends on, such as its fallback.
    pub dependencies: Vec<PPtr>,
    /// Textures the shader sets that materials cannot change.
    pub non_modifiable_textures: Vec<(String, PPtr)>,
    pub is_baked: bool,
    pub asset_guid: [u8; 16],
    pub asset_local_identifier: i64,
}

impl Layout for Shader {
    const CLASS: ClassId = ClassId::SHADER;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let name = r.string()?;
        let parsed = ParsedShader::read(r)?;
        let compiled = CompiledBlobs::read(r)?;
        let dependencies = r.pptrs("shader dependency")?;
        let non_modifiable_textures = r.array("non-modifiable texture", STRING_LEN + 12, |r| {
            Ok((r.string()?, r.pptr()?))
        })?;
        let is_baked = r.bool()?;
        r.align()?;
        Ok(Self {
            name,
            parsed,
            compiled,
            dependencies,
            non_modifiable_textures,
            is_baked,
            asset_guid: r.bytes()?,
            asset_local_identifier: r.i64()?,
        })
    }
}
