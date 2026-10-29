use super::{TEMPORAL_UNIFORM_BYTES, TemporalView};
use glam::Mat4;

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
fn a_frame_without_history_encodes_zero_validity() {
    assert!(lane(&view(false).to_bytes(), 36).abs() < f32::EPSILON);
}

#[test]
fn the_block_is_one_hundred_sixty_bytes_of_sixteen_byte_aligned_lanes() {
    assert_eq!(TEMPORAL_UNIFORM_BYTES, 160);
    assert_eq!(TEMPORAL_UNIFORM_BYTES % 16, 0);
}
