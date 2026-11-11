//! The encodings of the work a scriptable render context records.
use super::WireError;
use super::codec::{Reader, Wire, Writer};
use super::readback_parts::{read_readback, write_readback};
use crate::graphics::{DrawTopology, MAX_CULL_PLANES, OffscreenWork};

pub(super) const WORK_CULL: u8 = 10;
const WORK_SETUP_CAMERA: u8 = 11;
const WORK_SET_VIEW_PROJECTION: u8 = 15;
const WORK_DRAW_MESH: u8 = 16;
const WORK_DRAW_PROCEDURAL: u8 = 17;

const TOPOLOGY_TRIANGLE_LIST: u8 = 0;
const TOPOLOGY_LINE_LIST: u8 = 1;

impl Wire for DrawTopology {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        match self {
            Self::TriangleList => TOPOLOGY_TRIANGLE_LIST,
            Self::LineList => TOPOLOGY_LINE_LIST,
        }
        .write(out)
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        match u8::read(input)? {
            TOPOLOGY_TRIANGLE_LIST => Ok(Self::TriangleList),
            TOPOLOGY_LINE_LIST => Ok(Self::LineList),
            other => Err(WireError::UnknownTag(u32::from(other))),
        }
    }
}

/// The most planes a decoder accepts for one culling result.
fn plane_limit() -> u32 {
    u32::try_from(MAX_CULL_PLANES).unwrap_or(u32::MAX)
}

/// Writes the context variants of [`OffscreenWork`]; the other variants have their own writers.
pub(super) fn write_context(work: &OffscreenWork, out: &mut Writer) -> Result<(), WireError> {
    match work {
        OffscreenWork::Cull {
            id,
            camera,
            culling_mask,
            planes,
            options,
        } => {
            WORK_CULL.write(out)?;
            id.write(out)?;
            camera.write(out)?;
            culling_mask.write(out)?;
            out.count(planes.len(), plane_limit())?;
            for plane in planes {
                plane.write(out)?;
            }
            options.write(out)
        }
        OffscreenWork::SetupCamera {
            camera,
            view,
            projection,
            target,
        } => {
            WORK_SETUP_CAMERA.write(out)?;
            camera.write(out)?;
            view.write(out)?;
            projection.write(out)?;
            target.write(out)
        }
        OffscreenWork::SetViewProjection { view, projection } => {
            WORK_SET_VIEW_PROJECTION.write(out)?;
            view.write(out)?;
            projection.write(out)
        }
        OffscreenWork::DrawMesh {
            mesh,
            matrix,
            material,
            submesh,
            pass,
        } => {
            WORK_DRAW_MESH.write(out)?;
            mesh.write(out)?;
            matrix.write(out)?;
            material.write(out)?;
            submesh.write(out)?;
            pass.write(out)
        }
        OffscreenWork::DrawProcedural {
            matrix,
            material,
            pass,
            topology,
            vertex_count,
            instance_count,
        } => {
            WORK_DRAW_PROCEDURAL.write(out)?;
            matrix.write(out)?;
            material.write(out)?;
            pass.write(out)?;
            topology.write(out)?;
            vertex_count.write(out)?;
            instance_count.write(out)
        }
        other => write_readback(other, out),
    }
}

/// Reads the context variant that starts with `kind`, or refuses a kind no encoder writes.
pub(super) fn read_context(kind: u8, input: &mut Reader<'_>) -> Result<OffscreenWork, WireError> {
    match kind {
        WORK_CULL => {
            let id = Wire::read(input)?;
            let camera = Wire::read(input)?;
            let culling_mask = Wire::read(input)?;
            let count = input.count(plane_limit())?;
            let mut planes = Vec::with_capacity(count);
            for _ in 0..count {
                planes.push(<[f32; 4]>::read(input)?);
            }
            Ok(OffscreenWork::Cull {
                id,
                camera,
                culling_mask,
                planes,
                options: Wire::read(input)?,
            })
        }
        WORK_SETUP_CAMERA => Ok(OffscreenWork::SetupCamera {
            camera: Wire::read(input)?,
            view: Wire::read(input)?,
            projection: Wire::read(input)?,
            target: Wire::read(input)?,
        }),
        WORK_SET_VIEW_PROJECTION => Ok(OffscreenWork::SetViewProjection {
            view: Wire::read(input)?,
            projection: Wire::read(input)?,
        }),
        WORK_DRAW_MESH => Ok(OffscreenWork::DrawMesh {
            mesh: Wire::read(input)?,
            matrix: Wire::read(input)?,
            material: Wire::read(input)?,
            submesh: Wire::read(input)?,
            pass: Wire::read(input)?,
        }),
        WORK_DRAW_PROCEDURAL => Ok(OffscreenWork::DrawProcedural {
            matrix: Wire::read(input)?,
            material: Wire::read(input)?,
            pass: Wire::read(input)?,
            topology: Wire::read(input)?,
            vertex_count: Wire::read(input)?,
            instance_count: Wire::read(input)?,
        }),
        other => read_readback(other, input),
    }
}
