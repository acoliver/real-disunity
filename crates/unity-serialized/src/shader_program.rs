//! A shader pass's programs for one stage: compiled variants and their parameter bindings.

use crate::{SerializedError, object::ObjectReader};

/// A vector or matrix uniform. `columns` is a vector's dimension or a matrix's row count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueParam {
    /// Index into the pass's name indices.
    pub name_index: i32,
    /// Byte offset in the constant buffer, or register index.
    pub index: i32,
    pub array_size: i32,
    /// Unity's `ShaderParamType`: float, int, bool, half, short or uint.
    pub kind: i8,
    pub columns: i8,
}

/// Fewest bytes of a value parameter: three `i32`s, two `i8`s and padding.
const VALUE_PARAM_LEN: usize = 16;

impl ValueParam {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let param = Self {
            name_index: r.i32()?,
            index: r.i32()?,
            array_size: r.i32()?,
            kind: r.i8()?,
            columns: r.i8()?,
        };
        r.align()?;
        Ok(param)
    }

    fn list(r: &mut ObjectReader<'_>, what: &'static str) -> Result<Vec<Self>, SerializedError> {
        r.array(what, VALUE_PARAM_LEN, Self::read)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureParam {
    pub name_index: i32,
    pub index: i32,
    pub sampler_index: i32,
    pub multisampled: bool,
    /// Unity's `TextureDimension`.
    pub dimension: i8,
}

impl TextureParam {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let param = Self {
            name_index: r.i32()?,
            index: r.i32()?,
            sampler_index: r.i32()?,
            multisampled: r.bool()?,
            dimension: r.i8()?,
        };
        r.align()?;
        Ok(param)
    }
}

/// A buffer or constant buffer binding: name, binding index and array size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub name_index: i32,
    pub index: i32,
    pub array_size: i32,
}

impl Binding {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            name_index: r.i32()?,
            index: r.i32()?,
            array_size: r.i32()?,
        })
    }

    fn list(r: &mut ObjectReader<'_>, what: &'static str) -> Result<Vec<Self>, SerializedError> {
        r.array(what, 12, Self::read)
    }
}

/// A struct inside a constant buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructParam {
    pub name_index: i32,
    pub index: i32,
    pub array_size: i32,
    pub struct_size: i32,
    pub vectors: Vec<ValueParam>,
    pub matrices: Vec<ValueParam>,
}

impl StructParam {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            name_index: r.i32()?,
            index: r.i32()?,
            array_size: r.i32()?,
            struct_size: r.i32()?,
            vectors: ValueParam::list(r, "struct vector")?,
            matrices: ValueParam::list(r, "struct matrix")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstantBuffer {
    pub name_index: i32,
    pub matrices: Vec<ValueParam>,
    pub vectors: Vec<ValueParam>,
    pub structs: Vec<StructParam>,
    pub size: i32,
    /// Whether only some of the buffer's members are used.
    pub partial: bool,
}

impl ConstantBuffer {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let buffer = Self {
            name_index: r.i32()?,
            matrices: ValueParam::list(r, "constant buffer matrix")?,
            vectors: ValueParam::list(r, "constant buffer vector")?,
            structs: r.array("constant buffer struct", 24, StructParam::read)?,
            size: r.i32()?,
            partial: r.bool()?,
        };
        r.align()?;
        Ok(buffer)
    }
}

/// A read-write resource: name, binding index and its index before remapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UavParam {
    pub name_index: i32,
    pub index: i32,
    pub original_index: i32,
}

/// The parameters a program reads and where each is bound.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProgramParameters {
    pub vectors: Vec<ValueParam>,
    pub matrices: Vec<ValueParam>,
    pub textures: Vec<TextureParam>,
    pub buffers: Vec<Binding>,
    pub constant_buffers: Vec<ConstantBuffer>,
    pub constant_buffer_bindings: Vec<Binding>,
    pub uavs: Vec<UavParam>,
    /// Sampler state and the binding it applies to.
    pub samplers: Vec<[u32; 2]>,
}

impl ProgramParameters {
    pub(crate) fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            vectors: ValueParam::list(r, "vector parameter")?,
            matrices: ValueParam::list(r, "matrix parameter")?,
            textures: r.array("texture parameter", 16, TextureParam::read)?,
            buffers: Binding::list(r, "buffer parameter")?,
            constant_buffers: r.array("constant buffer", 24, ConstantBuffer::read)?,
            constant_buffer_bindings: Binding::list(r, "constant buffer binding")?,
            uavs: r.array("UAV parameter", 12, |r| {
                Ok(UavParam {
                    name_index: r.i32()?,
                    index: r.i32()?,
                    original_index: r.i32()?,
                })
            })?,
            samplers: r.array("sampler", 8, |r| Ok([r.u32()?, r.u32()?]))?,
        })
    }
}

/// Fewest bytes of empty program parameters: eight array counts.
const PARAMETERS_LEN: usize = 32;

/// How vertex data channels map to shader inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindChannels {
    /// Source vertex channel and target shader input, as Unity's `ShaderChannel` and
    /// `VertexComponent` values.
    pub channels: Vec<[i8; 2]>,
    pub source_map: i32,
}

/// One compiled variant of a program, with its own parameters. Used by shaders compiled
/// before player sub-programs existed; current players use [`PlayerSubProgram`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubProgram {
    /// Index into the shader's blob entries for this platform.
    pub blob_index: u32,
    pub channels: BindChannels,
    /// Indices into the shader's keyword names that select this variant.
    pub keyword_indices: Vec<u16>,
    pub hardware_tier: i8,
    /// Unity's `ShaderGpuProgramType`, for example SPIR-V.
    pub gpu_program_type: i8,
    pub parameters: ProgramParameters,
    pub requirements: i64,
}

impl SubProgram {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let blob_index = r.u32()?;
        let channels = BindChannels {
            channels: r.array("bind channel", 2, |r| Ok([r.i8()?, r.i8()?]))?,
            source_map: r.i32()?,
        };
        let keyword_indices = r.array("keyword index", 2, ObjectReader::u16)?;
        let [hardware_tier, gpu_program_type] = [r.i8()?, r.i8()?];
        r.align()?;
        Ok(Self {
            blob_index,
            channels,
            keyword_indices,
            hardware_tier,
            gpu_program_type,
            parameters: ProgramParameters::read(r)?,
            requirements: r.i64()?,
        })
    }
}

/// One compiled variant of a program in a player. Its parameters are stored with the
/// compiled code, at [`Program::parameter_blob_indices`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerSubProgram {
    pub blob_index: u32,
    pub keyword_indices: Vec<u16>,
    pub requirements: i64,
    pub gpu_program_type: i8,
}

impl PlayerSubProgram {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let sub_program = Self {
            blob_index: r.u32()?,
            keyword_indices: r.array("keyword index", 2, ObjectReader::u16)?,
            requirements: r.i64()?,
            gpu_program_type: r.i8()?,
        };
        r.align()?;
        Ok(sub_program)
    }
}

/// A pass's program for one stage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    pub sub_programs: Vec<SubProgram>,
    /// Variants per platform, in the order of the shader's platforms.
    pub player_sub_programs: Vec<Vec<PlayerSubProgram>>,
    /// Per platform, the blob entry that holds each variant's parameters.
    pub parameter_blob_indices: Vec<Vec<u32>>,
    pub common_parameters: ProgramParameters,
    pub keyword_state_mask: Vec<u16>,
}

impl Program {
    pub(crate) fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            sub_programs: r.array("sub-program", 24 + PARAMETERS_LEN, SubProgram::read)?,
            player_sub_programs: r.array("platform sub-programs", 4, |r| {
                r.array("player sub-program", 20, PlayerSubProgram::read)
            })?,
            parameter_blob_indices: r.array("platform parameter blobs", 4, |r| {
                r.array("parameter blob index", 4, ObjectReader::u32)
            })?,
            common_parameters: ProgramParameters::read(r)?,
            keyword_state_mask: r.array("keyword state", 2, ObjectReader::u16)?,
        })
    }
}
