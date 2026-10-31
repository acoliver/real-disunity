//! Byte layout of the temporal camera block.
use crate::{TEMPORAL_CAMERA_BYTES, TaaUniform, TemporalCamera};

fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn camera() -> TemporalCamera {
    let mut reprojection = [[0.0; 4]; 4];
    for (column, row) in reprojection.iter_mut().zip(0..) {
        for (value, lane) in column.iter_mut().zip(0..) {
            *value = f64::from(column_major_marker(row, lane));
        }
    }
    TemporalCamera {
        reprojection,
        depth_terms: [1.25, -0.5],
        valid: true,
    }
}

fn column_major_marker(column: i32, row: i32) -> i32 {
    10 * column + row
}

#[test]
fn the_block_is_five_sixteen_byte_rows() {
    assert_eq!(TEMPORAL_CAMERA_BYTES, 80);
    assert_eq!(TEMPORAL_CAMERA_BYTES % 16, 0);
    assert_eq!(camera().to_bytes().len(), 80);
}

#[test]
fn every_field_sits_at_its_documented_offset() {
    let values = floats(&camera().to_bytes());
    let expected_matrix: Vec<f32> = (0..4)
        .flat_map(|c| (0..4).map(move |r| f32::from(u8::try_from(10 * c + r).unwrap())))
        .collect();
    assert_eq!(values[..16], expected_matrix);
    assert_eq!(values[16..], [1.0, 0.0, 1.25, -0.5]);
}

#[test]
fn an_unusable_camera_uploads_as_no_camera() {
    let none = [0; TEMPORAL_CAMERA_BYTES];
    assert_eq!(TemporalCamera::NONE.to_bytes(), none);
    let invalid = TemporalCamera {
        valid: false,
        ..camera()
    };
    assert_eq!(invalid.to_bytes(), none);
    let mut nan = camera();
    nan.reprojection[2][1] = f64::NAN;
    assert_eq!(nan.to_bytes(), none);
    let mut overflow = camera();
    overflow.reprojection[0][0] = 1e300;
    assert_eq!(overflow.to_bytes(), none);
    let mut terms = camera();
    terms.depth_terms[1] = f64::INFINITY;
    assert_eq!(terms.to_bytes(), none);
}

#[test]
fn the_camera_is_read_off_the_native_taa_block() {
    let native = TaaUniform {
        reprojection: camera().reprojection,
        jitter_pixels: [0.25, -0.25],
        history_valid: true,
        depth_terms: [1.25, -0.5],
    };
    assert_eq!(TemporalCamera::from(&native), camera());
    let reset = TaaUniform {
        history_valid: false,
        ..native
    };
    assert_eq!(TemporalCamera::from(&reset).to_bytes(), [0; 80]);
}
