//! The raster state a command buffer sets for the draws after it: a scissor rectangle and the
//! global depth bias.
//!
//! The scissor is a rectangle in pixels of the bound targets from the bottom left, the convention
//! of a viewport. It holds until a disable, a binding of targets or a camera setup, as a viewport
//! does. The depth bias holds until the next bias record, and a bias of 0 with a slope bias of 0
//! ends it; binding other targets does not change it. Both travel with each list or procedural draw
//! as its [`DrawRaster`], so a record does not depend on the state at the time the frame is built.
//! A clear ignores the scissor. The units of the bias are the script's: the render program takes
//! them as the constant and slope-scale terms of the pipeline bias.

use worldrunner_render_program::unity::offscreen::frame::DrawRaster;
use worldrunner_world_simulation::graphics::OffscreenWork;

use super::frame::Mapper;

/// The scissor and the depth bias the commands so far left.
#[derive(Default)]
pub(super) struct RasterScript {
    scissor: Option<[f32; 4]>,
    depth_bias: [f32; 2],
}

impl RasterScript {
    /// Applies a command that sets the raster state, and says whether `work` is one.
    pub(super) fn apply(&mut self, work: &OffscreenWork) -> bool {
        match work {
            OffscreenWork::EnableScissor { rect } => self.scissor = Some(*rect),
            OffscreenWork::DisableScissor => self.scissor = None,
            OffscreenWork::SetDepthBias { bias, slope_bias } => {
                self.depth_bias = [*bias, *slope_bias];
            }
            _ => return false,
        }
        true
    }

    /// Ends the scissor: the targets the draws go to changed.
    pub(super) fn end_scissor(&mut self) {
        self.scissor = None;
    }

    fn draw(&self) -> DrawRaster {
        DrawRaster {
            scissor: self.scissor,
            depth_bias: self.depth_bias,
        }
    }
}

impl Mapper<'_> {
    /// The raster state the draws at this place in the frame carry.
    pub(super) fn raster(&self) -> DrawRaster {
        self.script.raster.draw()
    }
}
