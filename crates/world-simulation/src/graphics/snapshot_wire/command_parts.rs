//! The encodings of the work command buffers record.
use super::WireError;
use super::codec::{Reader, Wire, Writer};
use super::context_parts::{read_context, write_context};
use super::scene_parts::{
    WORK_ALLOCATE_TEMPORARY, WORK_CLEAR_RENDER_TARGET, WORK_RELEASE_TEMPORARY, WORK_SET_GLOBAL,
    WORK_SET_GLOBAL_KEYWORD, WORK_SET_RENDER_TARGET,
};
use crate::graphics::{OffscreenWork, PropertyValue, RenderTextureId, TargetRef, TextureSlot};

const TARGET_RENDER: u8 = 0;
const TARGET_TEMPORARY: u8 = 1;
const TARGET_CAMERA: u8 = 2;
const TARGET_NONE: u8 = 3;

impl Wire for TargetRef {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        match self {
            Self::Render(id) => {
                TARGET_RENDER.write(out)?;
                id.write(out)
            }
            Self::Temporary(name) => {
                TARGET_TEMPORARY.write(out)?;
                name.write(out)
            }
            Self::CameraTarget => TARGET_CAMERA.write(out),
            Self::None => TARGET_NONE.write(out),
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        match u8::read(input)? {
            TARGET_RENDER => Ok(Self::Render(RenderTextureId::read(input)?)),
            TARGET_TEMPORARY => Ok(Self::Temporary(i32::read(input)?)),
            TARGET_CAMERA => Ok(Self::CameraTarget),
            TARGET_NONE => Ok(Self::None),
            other => Err(WireError::UnknownTag(u32::from(other))),
        }
    }
}

const PROPERTY_FLOAT: u8 = 0;
const PROPERTY_INT: u8 = 1;
const PROPERTY_VECTOR: u8 = 2;
const PROPERTY_MATRIX: u8 = 3;
const PROPERTY_TEXTURE: u8 = 4;

impl Wire for PropertyValue {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        match self {
            Self::Float(name, value) => write_named(out, PROPERTY_FLOAT, name, value),
            Self::Int(name, value) => write_named(out, PROPERTY_INT, name, value),
            Self::Vector(name, value) => write_named(out, PROPERTY_VECTOR, name, value),
            Self::Matrix(name, value) => write_named(out, PROPERTY_MATRIX, name, value),
            Self::Texture(name, value) => write_named(out, PROPERTY_TEXTURE, name, value),
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        let tag = u8::read(input)?;
        if tag > PROPERTY_TEXTURE {
            return Err(WireError::UnknownTag(u32::from(tag)));
        }
        let name = String::read(input)?;
        Ok(match tag {
            PROPERTY_FLOAT => Self::Float(name, Wire::read(input)?),
            PROPERTY_INT => Self::Int(name, Wire::read(input)?),
            PROPERTY_VECTOR => Self::Vector(name, Wire::read(input)?),
            PROPERTY_MATRIX => Self::Matrix(name, Wire::read(input)?),
            _ => Self::Texture(name, TextureSlot::read(input)?),
        })
    }
}

fn write_named(out: &mut Writer, tag: u8, name: &str, value: &impl Wire) -> Result<(), WireError> {
    tag.write(out)?;
    name.to_owned().write(out)?;
    value.write(out)
}

/// Writes the command variants of [`OffscreenWork`]; the other variants have their own writer.
pub(super) fn write_command(work: &OffscreenWork, out: &mut Writer) -> Result<(), WireError> {
    match work {
        OffscreenWork::SetRenderTarget {
            color,
            depth,
            mip,
            face,
            slice,
        } => {
            WORK_SET_RENDER_TARGET.write(out)?;
            color.write(out)?;
            depth.write(out)?;
            mip.write(out)?;
            face.write(out)?;
            slice.write(out)
        }
        OffscreenWork::ClearRenderTarget {
            color,
            depth,
            stencil,
            value,
            depth_value,
            stencil_value,
        } => {
            WORK_CLEAR_RENDER_TARGET.write(out)?;
            [*color, *depth, *stencil].write(out)?;
            value.write(out)?;
            depth_value.write(out)?;
            stencil_value.write(out)
        }
        OffscreenWork::SetGlobal(value) => {
            WORK_SET_GLOBAL.write(out)?;
            value.write(out)
        }
        OffscreenWork::SetGlobalKeyword { name, enabled } => {
            WORK_SET_GLOBAL_KEYWORD.write(out)?;
            name.write(out)?;
            enabled.write(out)
        }
        OffscreenWork::AllocateTemporary { name_id, desc } => {
            WORK_ALLOCATE_TEMPORARY.write(out)?;
            name_id.write(out)?;
            desc.write(out)
        }
        OffscreenWork::ReleaseTemporary { name_id } => {
            WORK_RELEASE_TEMPORARY.write(out)?;
            name_id.write(out)
        }
        other => write_context(other, out),
    }
}

/// Reads the command variant that starts with `kind`, or refuses a kind no encoder writes.
pub(super) fn read_command(kind: u8, input: &mut Reader<'_>) -> Result<OffscreenWork, WireError> {
    match kind {
        WORK_SET_RENDER_TARGET => Ok(OffscreenWork::SetRenderTarget {
            color: Wire::read(input)?,
            depth: Wire::read(input)?,
            mip: Wire::read(input)?,
            face: Wire::read(input)?,
            slice: Wire::read(input)?,
        }),
        WORK_CLEAR_RENDER_TARGET => {
            let [color, depth, stencil] = <[bool; 3]>::read(input)?;
            Ok(OffscreenWork::ClearRenderTarget {
                color,
                depth,
                stencil,
                value: Wire::read(input)?,
                depth_value: Wire::read(input)?,
                stencil_value: Wire::read(input)?,
            })
        }
        WORK_SET_GLOBAL => Ok(OffscreenWork::SetGlobal(PropertyValue::read(input)?)),
        WORK_SET_GLOBAL_KEYWORD => Ok(OffscreenWork::SetGlobalKeyword {
            name: Wire::read(input)?,
            enabled: Wire::read(input)?,
        }),
        WORK_ALLOCATE_TEMPORARY => Ok(OffscreenWork::AllocateTemporary {
            name_id: Wire::read(input)?,
            desc: Wire::read(input)?,
        }),
        WORK_RELEASE_TEMPORARY => Ok(OffscreenWork::ReleaseTemporary {
            name_id: Wire::read(input)?,
        }),
        other => read_context(other, input),
    }
}
