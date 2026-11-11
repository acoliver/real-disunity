//! The encodings of the graphics resources a snapshot holds.
use super::WireError;
use super::array_parts::{read_array_list, write_array_list};
use super::codec::{Reader, Wire, Writer, read_bytes, wire_id, wire_struct, write_bytes};
use crate::graphics::{
    ComputeBindings, ComputeBufferId, ComputeKernel, ComputeShaderId, GlobalShaderState,
    MAX_COMPUTE_BUFFER_BYTES, MaterialId, RenderTextureId, ShaderId, ShaderProperties,
    ShaderSource, TextureRef, TextureSlot, WorldComputeBuffer, WorldComputeShader, WorldMaterial,
    WorldRenderTexture, WorldSampler, WorldShader,
};

wire_id!(
    ShaderId,
    MaterialId,
    RenderTextureId,
    ComputeShaderId,
    ComputeBufferId
);

const TEXTURE_ASSET: u8 = 0;
const TEXTURE_RENDER: u8 = 1;
const TEXTURE_GLOBAL: u8 = 2;
const TEXTURE_NONE: u8 = 3;

impl Wire for TextureRef {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        match self {
            Self::Asset(identity) => {
                TEXTURE_ASSET.write(out)?;
                identity.write(out)
            }
            Self::Render(id) => {
                TEXTURE_RENDER.write(out)?;
                id.write(out)
            }
            Self::Global(name) => {
                TEXTURE_GLOBAL.write(out)?;
                name.write(out)
            }
            Self::None => TEXTURE_NONE.write(out),
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        match u8::read(input)? {
            TEXTURE_ASSET => Ok(Self::Asset(u64::read(input)?)),
            TEXTURE_RENDER => Ok(Self::Render(RenderTextureId::read(input)?)),
            TEXTURE_GLOBAL => Ok(Self::Global(String::read(input)?)),
            TEXTURE_NONE => Ok(Self::None),
            other => Err(WireError::UnknownTag(u32::from(other))),
        }
    }
}

wire_struct!(TextureSlot {
    texture,
    scale,
    offset
});
impl Wire for ShaderProperties {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        self.floats.write(out)?;
        self.ints.write(out)?;
        self.vectors.write(out)?;
        self.matrices.write(out)?;
        self.textures.write(out)?;
        write_array_list(out, &self.vector_arrays)?;
        write_array_list(out, &self.matrix_arrays)
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(Self {
            floats: Wire::read(input)?,
            ints: Wire::read(input)?,
            vectors: Wire::read(input)?,
            matrices: Wire::read(input)?,
            textures: Wire::read(input)?,
            vector_arrays: read_array_list(input)?,
            matrix_arrays: read_array_list(input)?,
        })
    }
}
wire_struct!(ShaderSource { file, path_id });
wire_struct!(WorldShader { name, source });
wire_struct!(WorldMaterial {
    shader,
    render_queue,
    keywords,
    disabled_passes,
    properties,
    generation
});
wire_struct!(GlobalShaderState {
    keywords,
    properties,
    generation
});
wire_struct!(WorldSampler {
    wrap_u,
    wrap_v,
    wrap_w,
    filter,
    aniso_level,
    mip_bias
});
wire_struct!(WorldRenderTexture {
    width,
    height,
    format,
    depth_bits,
    mip_count,
    srgb,
    dimension,
    volume_depth,
    sampler,
    generation
});
wire_struct!(ComputeKernel {
    name,
    thread_group_size
});
wire_struct!(WorldComputeShader {
    name,
    source,
    kernels,
    generation
});
wire_struct!(ComputeBindings {
    properties,
    buffers
});

impl Wire for WorldComputeBuffer {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        self.count.write(out)?;
        self.stride.write(out)?;
        self.kind.write(out)?;
        self.generation.write(out)?;
        let limit = u32::try_from(MAX_COMPUTE_BUFFER_BYTES).map_err(|_| WireError::TooLong)?;
        write_bytes(out, &self.contents, limit)
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        let count = u32::read(input)?;
        let stride = u32::read(input)?;
        let kind = u32::read(input)?;
        let generation = u64::read(input)?;
        let limit = u32::try_from(MAX_COMPUTE_BUFFER_BYTES).map_err(|_| WireError::TooLong)?;
        Ok(Self {
            count,
            stride,
            kind,
            contents: read_bytes(input, limit)?,
            generation,
        })
    }
}
