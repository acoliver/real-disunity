//! The material mip bias travels in the last lane of the frame block, which was always zero.
use crate::frame::{FRAME_UNIFORM_BYTES, FrameUniform};
use crate::view::{CameraPose, SceneView, Viewport};
use worldrunner_render_plan::SceneCamera;

/// Byte range of `lighting_options.w`: the block ends with the sixteen-byte `lighting_options`.
const BIAS_LANE: std::ops::Range<usize> = FRAME_UNIFORM_BYTES - 4..FRAME_UNIFORM_BYTES;

fn frame() -> FrameUniform {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [1_000, 2_000, 3_000],
            target_millimeters: [1_000, 12_000, 3_000],
        },
        SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 1_000_000,
        },
        Viewport {
            width: 64,
            height: 64,
        },
    )
    .expect("the fixture pose is well formed");
    FrameUniform::unlit(&view)
}

#[test]
fn a_new_frame_has_positive_zero_bias_and_an_all_zero_lane() {
    let frame = frame();
    assert_eq!(frame.material_mip_bias.to_bits(), 0);
    assert_eq!(frame.to_bytes()[BIAS_LANE], [0; 4]);
}

#[test]
fn the_bias_occupies_only_the_last_four_bytes_of_the_block() {
    let unbiased = frame().to_bytes();
    for bias in [-2.0_f32, -1.578_4, -1.377_4] {
        let mut biased = frame();
        biased.material_mip_bias = bias;
        let bytes = biased.to_bytes();
        assert_eq!(bytes[BIAS_LANE], bias.to_le_bytes(), "{bias}");
        assert_eq!(
            bytes[..BIAS_LANE.start],
            unbiased[..BIAS_LANE.start],
            "{bias}: every other byte is the unbiased encoding"
        );
    }
    assert_eq!(unbiased.len(), 320, "the block size is unchanged");
}
