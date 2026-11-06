//! The encodings of the work a scriptable render context records.
use super::WireError;
use super::codec::{Reader, Wire, Writer};
use crate::graphics::{MAX_CULL_PLANES, OffscreenWork};

pub(super) const WORK_CULL: u8 = 10;

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
        _ => Err(WireError::UnknownTag(u32::MAX)),
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
        other => Err(WireError::UnknownTag(u32::from(other))),
    }
}
