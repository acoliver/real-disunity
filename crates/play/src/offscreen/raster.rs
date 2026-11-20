//! The raster state a command buffer sets for the draws after it: a viewport, a scissor rectangle
//! and the global depth bias.
//!
//! The scissor is a rectangle in pixels of the bound targets from the bottom left, the convention
//! of a viewport. It holds until a disable, a binding of targets or a camera setup, as a viewport
//! does. The depth bias holds until the next bias record, and a bias of 0 with a slope bias of 0
//! ends it; binding other targets does not change it. Both travel with each list or procedural draw
//! as its [`DrawRaster`], so a record does not depend on the state at the time the frame is built.
//! A viewport inside the target maps the draws' clip space to its rectangle (see
//! [`Mapper::draw_target`]); a clear ignores the scissor. The units of the bias are the script's: the render program takes
//! them as the constant and slope-scale terms of the pipeline bias.

use worldrunner_render_program::unity::offscreen::frame::{Attachments, DrawRaster};
use worldrunner_world_simulation::graphics::OffscreenWork;

use super::frame::{Mapper, Skip};
use super::script::covers;

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

    pub(super) fn draw(&self) -> DrawRaster {
        DrawRaster {
            scissor: self.scissor,
            depth_bias: self.depth_bias,
            ..DrawRaster::default()
        }
    }
}

/// Whether a viewport of `rect` (x, y, width, height in pixels) has an area and lies inside a
/// target of `size`.
fn inside(rect: [f32; 4], size: [u32; 2]) -> bool {
    let [x, y, width, height] = rect.map(f64::from);
    let [target_width, target_height] = size.map(f64::from);
    x >= 0.0
        && y >= 0.0
        && width > 0.0
        && height > 0.0
        && x + width <= target_width
        && y + height <= target_height
}

impl Mapper<'_> {
    /// The targets a draw at this place in the frame goes to, and the raster state it carries. A
    /// recorded viewport that covers the target draws as if none were set, one inside the target
    /// maps the draw's clip space to its rectangle, and one that leaves the target is refused as
    /// [`Skip::PartialViewport`], because the host cannot map clip space outside its target.
    pub(super) fn draw_target(&self) -> Result<(Attachments, DrawRaster), Skip> {
        let target = self.script.target.clone().ok_or(Skip::UnboundTarget)?;
        let mut raster = self.script.raster.draw();
        if let Some(rect) = self.script.viewport {
            let size = self.target_size().ok_or(Skip::PartialViewport)?;
            if !covers(rect, size) {
                if !inside(rect, size) {
                    return Err(Skip::PartialViewport);
                }
                raster.viewport = Some(rect);
            }
        }
        Ok((target, raster))
    }
}
