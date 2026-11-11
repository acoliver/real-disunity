use super::*;
use glam::Vec3;

fn decode(bytes: &[u8]) -> ([f32; 4], Vec<u32>) {
    let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    let frame = [0, 1, 2, 3].map(|i| f32::from_bits(word(i)));
    (frame, (4..4 + CELLS * CELLS).map(word).collect())
}

/// The volumes the shader would visit for a world point, by its cell formula.
fn candidates(bytes: &[u8], point: [f32; 2]) -> u32 {
    let (frame, masks) = decode(bytes);
    let scaled = [0, 1].map(|axis| (point[axis] - frame[axis]) * frame[axis + 2]);
    if scaled
        .iter()
        .any(|&s| !(0.0..f32::from(AXIS_CELLS)).contains(&s))
    {
        return 0;
    }
    let [x, y] = [0, 1].map(|axis| cell(point[axis], frame[axis], frame[axis + 2]));
    masks[y * CELLS + x]
}

fn bounds(min: [f32; 3], max: [f32; 3]) -> Bounds {
    Bounds {
        min: Vec3::from_array(min),
        max: Vec3::from_array(max),
    }
}

#[test]
fn every_point_inside_a_volume_reaches_that_volume() {
    let volumes = [
        bounds([-120.0, 40.0, 0.0], [-118.5, 42.0, 2.0]),
        bounds([300.0, -75.0, -1.0], [304.0, -70.0, 1.0]),
        bounds([301.0, -74.0, 0.0], [302.0, -73.0, 0.5]),
        bounds([5.0, 5.0, 0.0], [5.0, 5.0, 0.0]),
    ];
    let bytes = bytes(&volumes);
    assert_eq!(bytes.len(), BYTES);
    for (index, volume) in volumes.iter().enumerate() {
        for u in 0..=20u8 {
            for v in 0..=20u8 {
                let t = [f32::from(u) / 20.0, f32::from(v) / 20.0];
                let point = [0, 1]
                    .map(|axis| volume.min[axis] + (volume.max[axis] - volume.min[axis]) * t[axis]);
                assert_ne!(
                    candidates(&bytes, point) & (1 << index),
                    0,
                    "volume {index} at {point:?}"
                );
            }
        }
    }
}

#[test]
fn distant_points_visit_no_volume_and_far_cells_stay_empty() {
    let volumes = [
        bounds([-120.0, 40.0, 0.0], [-118.5, 42.0, 2.0]),
        bounds([300.0, -75.0, -1.0], [304.0, -70.0, 1.0]),
    ];
    let bytes = bytes(&volumes);
    assert_eq!(candidates(&bytes, [90.0, -10.0]), 0);
    assert_eq!(candidates(&bytes, [-500.0, 0.0]), 0);
    assert_eq!(candidates(&bytes, [1000.0, 1000.0]), 0);
    let (_, masks) = decode(&bytes);
    let empty = masks.iter().filter(|&&m| m == 0).count();
    assert!(empty > CELLS * CELLS / 2, "{empty} empty cells");
}

#[test]
fn no_selected_volume_writes_a_zero_grid() {
    assert_eq!(bytes(&[]), vec![0; BYTES]);
}
