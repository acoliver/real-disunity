//! The encoding of the pixel read request.
use super::WireError;
use super::codec::{Reader, Wire, Writer};
use crate::graphics::OffscreenWork;

const WORK_READ_PIXELS: u8 = 12;

/// Writes the pixel read variant of [`OffscreenWork`]; the other variants have their own writers.
pub(super) fn write_readback(work: &OffscreenWork, out: &mut Writer) -> Result<(), WireError> {
    match work {
        OffscreenWork::ReadPixels {
            request,
            texture,
            x,
            y,
            width,
            height,
        } => {
            WORK_READ_PIXELS.write(out)?;
            request.write(out)?;
            texture.write(out)?;
            [*x, *y, *width, *height].write(out)
        }
        _ => Err(WireError::UnknownTag(u32::MAX)),
    }
}

/// Reads the pixel read variant that starts with `kind`, or refuses a kind no encoder writes.
pub(super) fn read_readback(kind: u8, input: &mut Reader<'_>) -> Result<OffscreenWork, WireError> {
    match kind {
        WORK_READ_PIXELS => {
            let request = Wire::read(input)?;
            let texture = Wire::read(input)?;
            let [x, y, width, height] = <[u32; 4]>::read(input)?;
            Ok(OffscreenWork::ReadPixels {
                request,
                texture,
                x,
                y,
                width,
                height,
            })
        }
        other => Err(WireError::UnknownTag(u32::from(other))),
    }
}
