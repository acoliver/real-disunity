//! The reset block is a real, valid block; history validity follows the frame's camera data.
use super::{extents_key, temporal_block};
use crate::frame::temporal::TemporalView;
use glam::{Mat4, Vec3};
use worldrunner_render_post::{FrameConfig, ProjectionJitter, TEMPORAL_BLOCK_BYTES, TaaSettings};

const OUTPUT: [u32; 2] = [64, 48];
const RENDER: [u32; 2] = [43, 32];

fn config(render: [u32; 2], output: [u32; 2]) -> FrameConfig {
    FrameConfig::new(output, wgpu::TextureFormat::Rgba16Float, 1).with_render_extent(render)
}

fn view(valid: bool) -> TemporalView {
    TemporalView {
        unjittered_view_projection: Mat4::IDENTITY,
        previous_view_projection: Mat4::IDENTITY,
        jitter_pixels: [0.25, -0.375],
        previous_jitter_pixels: [0.0, 0.0],
        history_valid: valid,
        previous_animation_time_ms: 0,
        previous_camera_position: Vec3::ZERO,
    }
}

fn block(view: Option<&TemporalView>) -> worldrunner_render_post::TemporalBlock {
    let jitter = ProjectionJitter::for_extents(RENDER, OUTPUT).unwrap();
    temporal_block(config(RENDER, OUTPUT), jitter, TaaSettings::default(), view)
}

#[test]
fn a_frame_without_camera_data_uploads_a_valid_block_without_history() {
    let block = block(None);
    assert!(!block.history_valid);
    assert_eq!((block.render, block.output), (RENDER, OUTPUT));
    assert!(
        block.jitter.iter().all(|&v| v.abs() < f32::EPSILON),
        "{:?}",
        block.jitter
    );
    let jitter = ProjectionJitter::for_extents(RENDER, OUTPUT).unwrap();
    assert_eq!(block.phase_count, jitter.length());
    assert_eq!(
        block.to_bytes().map(|bytes| bytes.len()),
        Ok(TEMPORAL_BLOCK_BYTES)
    );
}

#[test]
fn history_validity_and_jitter_follow_the_camera_data() {
    let valid = block(Some(&view(true)));
    assert!(valid.history_valid);
    for (got, want) in valid.jitter.into_iter().zip([0.25_f32, -0.375]) {
        assert!((got - want).abs() < 1e-6, "{:?}", valid.jitter);
    }
    assert!(!block(Some(&view(false))).history_valid);
}

#[test]
fn history_is_keyed_by_the_render_and_the_output_extent() {
    let base = extents_key(config(RENDER, OUTPUT));
    assert_ne!(
        base,
        extents_key(config([RENDER[0] + 1, RENDER[1]], OUTPUT))
    );
    assert_ne!(
        base,
        extents_key(config(RENDER, [OUTPUT[0] + 2, OUTPUT[1]]))
    );
    assert_eq!(base, extents_key(config(RENDER, OUTPUT)));
}
