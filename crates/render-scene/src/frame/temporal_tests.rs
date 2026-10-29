use super::{PREVIOUS_FRAME_BYTES, TEMPORAL_UNIFORM_BYTES, TemporalView};
use crate::frame::FrameUniform;
use crate::view::{CameraPose, SceneView, Viewport};
use glam::{Mat4, Vec3};
use worldrunner_render_plan::SceneCamera;

fn lane(bytes: &[u8; TEMPORAL_UNIFORM_BYTES], index: usize) -> f32 {
    f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
}

/// The values `first, first + 1, ..., first + 15` in column-major order.
fn counting_values(first: f32) -> [f32; 16] {
    let mut next = first - 1.0;
    std::array::from_fn(|_| {
        next += 1.0;
        next
    })
}

fn counting_matrix(first: f32) -> Mat4 {
    Mat4::from_cols_array(&counting_values(first))
}

fn view(history_valid: bool) -> TemporalView {
    TemporalView {
        unjittered_view_projection: counting_matrix(1.0),
        previous_view_projection: counting_matrix(101.0),
        jitter_pixels: [0.25, -0.125],
        previous_jitter_pixels: [-0.375, 0.0625],
        history_valid,
        previous_animation_time_ms: 0xDEAD_BEEF,
        previous_camera_position: Vec3::new(1.5, -2.5, 3.25),
    }
}

#[test]
fn the_block_holds_current_matrix_previous_matrix_jitters_and_validity_in_order() {
    let bytes = view(true).to_bytes();
    let current: Vec<f32> = (0..16).map(|index| lane(&bytes, index)).collect();
    let previous: Vec<f32> = (16..32).map(|index| lane(&bytes, index)).collect();
    assert_eq!(current, counting_values(1.0));
    assert_eq!(previous, counting_values(101.0));
    let tail: Vec<f32> = (32..40).map(|index| lane(&bytes, index)).collect();
    assert_eq!(tail, [0.25, -0.125, -0.375, 0.0625, 1.0, 0.0, 0.0, 0.0]);
}

#[test]
fn the_previous_camera_and_clock_fill_the_last_sixteen_bytes() {
    let bytes = view(true).to_bytes();
    let tail: Vec<f32> = (40..43).map(|index| lane(&bytes, index)).collect();
    assert_eq!(tail, [1.5, -2.5, 3.25]);
    assert_eq!(lane(&bytes, 43).to_bits(), 0xDEAD_BEEF);
    assert_eq!(&bytes[172..176], &0xDEAD_BEEF_u32.to_le_bytes());
}

#[test]
fn a_frame_without_history_encodes_zero_validity() {
    assert!(lane(&view(false).to_bytes(), 36).abs() < f32::EPSILON);
}

#[test]
fn the_block_is_one_hundred_seventy_six_bytes_of_sixteen_byte_aligned_lanes() {
    assert_eq!(TEMPORAL_UNIFORM_BYTES, 176);
    assert_eq!(TEMPORAL_UNIFORM_BYTES % 16, 0);
}

fn unlit_frame() -> FrameUniform {
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
    let mut frame = FrameUniform::unlit(&view);
    frame.animation_time_milliseconds = 4_321;
    frame
}

#[test]
fn the_previous_frame_block_is_eighty_bytes_of_sixteen_byte_aligned_lanes() {
    assert_eq!(PREVIOUS_FRAME_BYTES, 80);
    assert_eq!(PREVIOUS_FRAME_BYTES % 16, 0);
}

#[test]
fn the_previous_frame_block_carries_the_temporal_views_previous_matrix_camera_and_clock() {
    let mut frame = unlit_frame();
    frame.temporal = Some(view(true));
    let bytes = frame.previous_frame_bytes();
    let temporal = view(true).to_bytes();
    assert_eq!(&bytes[..64], &temporal[64..128], "previous matrix");
    assert_eq!(
        &bytes[64..80],
        &temporal[160..176],
        "camera and clock lanes"
    );
    assert_eq!(&bytes[76..80], &0xDEAD_BEEF_u32.to_le_bytes());
}

#[test]
fn the_previous_frame_block_is_the_current_values_without_a_temporal_view() {
    let frame = unlit_frame();
    assert!(frame.temporal.is_none());
    let bytes = frame.previous_frame_bytes();
    let matrix: Vec<u8> = frame
        .view_projection
        .to_cols_array()
        .iter()
        .flat_map(|lane| lane.to_le_bytes())
        .collect();
    assert_eq!(&bytes[..64], matrix.as_slice());
    let camera: Vec<u8> = frame
        .camera_position
        .to_array()
        .iter()
        .flat_map(|lane| lane.to_le_bytes())
        .collect();
    assert_eq!(&bytes[64..76], camera.as_slice());
    assert_eq!(&bytes[76..80], &4_321_u32.to_le_bytes());
}
