//! A command buffer's scissor rectangle and global depth bias: each list or procedural draw
//! carries the state in force at its place in the frame. The scissor ends at a disable, a new
//! target or a camera setup; the bias holds until the next bias record. The worlds are synthetic.

use worldrunner_render_program::unity::offscreen::frame::{DrawRaster, Record};
use worldrunner_world_simulation::graphics::{OffscreenWork, TargetRef};

use super::viewport::{Rig, kinds, rig};

const RECT: [f32; 4] = [2.0, 1.0, 4.0, 3.0];

impl Rig {
    fn scissor(&mut self, rect: [f32; 4]) {
        self.record(OffscreenWork::EnableScissor { rect });
    }

    fn bias(&mut self, bias: f32, slope_bias: f32) {
        self.record(OffscreenWork::SetDepthBias { bias, slope_bias });
    }

    fn rebind(&mut self) {
        let target = TargetRef::Render(self.target);
        self.record(OffscreenWork::SetRenderTarget {
            color: vec![target],
            depth: None,
            mip: 0,
            face: -1,
            slice: -1,
        });
    }

    /// The raster state of each draw record of the frame, in order.
    fn rasters(&mut self) -> Vec<DrawRaster> {
        let adapted = self.run();
        assert!(
            adapted.report.skipped.is_empty(),
            "{:?}",
            adapted.report.skipped
        );
        adapted
            .frame
            .records
            .iter()
            .filter_map(|record| match record {
                Record::Draws(list) => Some(list.raster),
                Record::Procedural(draw) => Some(draw.raster),
                _ => None,
            })
            .collect()
    }
}

fn scissored(rect: [f32; 4]) -> DrawRaster {
    DrawRaster {
        scissor: Some(rect),
        depth_bias: [0.0; 2],
    }
}

#[test]
fn a_draw_with_no_raster_command_carries_the_default_state() {
    let mut rig = rig();
    rig.bind();
    rig.draw_mesh();
    rig.draw_procedural();
    assert_eq!(rig.rasters(), [DrawRaster::default(); 2]);
}

#[test]
fn a_scissor_applies_to_the_mesh_and_procedural_draws_after_it_until_it_is_disabled() {
    let mut rig = rig();
    rig.bind();
    rig.draw_mesh();
    rig.scissor(RECT);
    rig.draw_mesh();
    rig.draw_procedural();
    rig.record(OffscreenWork::DisableScissor);
    rig.draw_procedural();
    assert_eq!(
        rig.rasters(),
        [
            DrawRaster::default(),
            scissored(RECT),
            scissored(RECT),
            DrawRaster::default()
        ]
    );
}

#[test]
fn the_latest_scissor_replaces_the_one_before_it() {
    let mut rig = rig();
    rig.bind();
    rig.scissor(RECT);
    rig.scissor([0.0, 0.0, 1.0, 1.0]);
    rig.draw_mesh();
    assert_eq!(rig.rasters(), [scissored([0.0, 0.0, 1.0, 1.0])]);
}

#[test]
fn binding_targets_or_setting_up_a_camera_ends_the_scissor() {
    let mut rig = rig();
    rig.bind();
    rig.scissor(RECT);
    rig.rebind();
    rig.draw_mesh();
    rig.scissor(RECT);
    rig.setup_camera();
    rig.draw_procedural();
    assert_eq!(rig.rasters(), [DrawRaster::default(); 2]);
}

#[test]
fn a_depth_bias_holds_across_target_changes_until_the_next_bias_record() {
    let mut rig = rig();
    rig.bind();
    rig.bias(1.0, 2.5);
    rig.draw_mesh();
    rig.rebind();
    rig.draw_procedural();
    rig.bias(0.0, 0.0);
    rig.draw_mesh();
    let with = |depth_bias| DrawRaster {
        scissor: None,
        depth_bias,
    };
    assert_eq!(
        rig.rasters(),
        [with([1.0, 2.5]), with([1.0, 2.5]), with([0.0, 0.0])]
    );
}

#[test]
fn a_scissor_does_not_leave_out_a_draw_or_a_clear_that_a_viewport_would() {
    let mut rig = rig();
    rig.bind();
    rig.scissor([0.0, 0.0, 2.0, 2.0]);
    rig.clear();
    rig.draw_mesh();
    let adapted = rig.run();
    assert_eq!(kinds(&adapted), ["clear", "draws"]);
    assert!(adapted.report.skipped.is_empty());
    assert_eq!(adapted.report.built_by_kind["EnableScissor"], 1);
}
