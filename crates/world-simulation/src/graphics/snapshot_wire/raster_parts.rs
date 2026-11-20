//! The encodings of the raster state a command buffer sets: the scissor rectangle and the global
//! depth bias.
use super::WireError;
use super::codec::{Reader, Wire, Writer};
use super::readback_parts::{read_readback, write_readback};
use crate::graphics::OffscreenWork;

const WORK_ENABLE_SCISSOR: u8 = 20;
const WORK_DISABLE_SCISSOR: u8 = 21;
const WORK_SET_DEPTH_BIAS: u8 = 22;

/// Writes the raster state variants of [`OffscreenWork`]; the other variants have their own
/// writers.
pub(super) fn write_raster(work: &OffscreenWork, out: &mut Writer) -> Result<(), WireError> {
    match work {
        OffscreenWork::EnableScissor { rect } => {
            WORK_ENABLE_SCISSOR.write(out)?;
            rect.write(out)
        }
        OffscreenWork::DisableScissor => WORK_DISABLE_SCISSOR.write(out),
        OffscreenWork::SetDepthBias { bias, slope_bias } => {
            WORK_SET_DEPTH_BIAS.write(out)?;
            bias.write(out)?;
            slope_bias.write(out)
        }
        other => write_readback(other, out),
    }
}

/// Reads the raster state variant that starts with `kind`, or refuses a kind no encoder writes.
pub(super) fn read_raster(kind: u8, input: &mut Reader<'_>) -> Result<OffscreenWork, WireError> {
    match kind {
        WORK_ENABLE_SCISSOR => Ok(OffscreenWork::EnableScissor {
            rect: Wire::read(input)?,
        }),
        WORK_DISABLE_SCISSOR => Ok(OffscreenWork::DisableScissor),
        WORK_SET_DEPTH_BIAS => Ok(OffscreenWork::SetDepthBias {
            bias: Wire::read(input)?,
            slope_bias: Wire::read(input)?,
        }),
        other => read_readback(other, input),
    }
}
