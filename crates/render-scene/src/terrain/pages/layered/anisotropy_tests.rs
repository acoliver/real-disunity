use super::anisotropy::{Footprint, principal_axes};
use glam::Vec2;

#[test]
fn finite_degenerate_footprints_remain_bounded() {
    for derivatives in [
        [[7.071_068e-14; 2], [0.0; 2]],
        [[f32::MAX / 16.0; 2], [0.0; 2]],
        [[f32::MAX; 2]; 2],
        [[f32::from_bits(1); 2], [0.0; 2]],
        [[1.0, 2.0], [0.0; 2]],
        [[0.125, 1e-30], [0.0, 0.25]],
        [[0.0; 2]; 2],
    ] {
        let size = Vec2::new(8.0, 4.0);
        let (axis, major, minor) = principal_axes(derivatives, size);
        assert!(axis.is_finite() && major.is_finite() && minor.is_finite());
        assert!((axis.length() - 1.0).abs() < 1e-6);
        let footprint = Footprint::new(derivatives, size, 16);
        assert!(footprint.direction.is_finite());
        assert!(footprint.lod.is_finite());
        assert!((1..=16).contains(&footprint.count));
        assert!(footprint.lod >= 0.0);
        assert!((0.0..=3.0).contains(&footprint.lod.clamp(0.0, 3.0)));
        if derivatives[0][0].abs() < 1e-12 {
            assert_eq!(footprint.count, 1);
            assert_eq!(footprint.lod.to_bits(), 0.0_f32.to_bits());
        }
    }
}

#[test]
fn production_principal_axes_preserve_worked_examples() {
    let diagonal = 0.375 / 2.0_f32.sqrt();
    for (derivatives, level, major, minor, lod) in [
        (
            [[diagonal, diagonal], [-diagonal, diagonal]],
            2,
            3.0,
            1.5,
            1.5_f32.log2(),
        ),
        (
            [[2.0, 0.25], [2.0, -0.25]],
            16,
            16.0 * 2.0_f32.sqrt(),
            2.0_f32.sqrt(),
            0.5,
        ),
    ] {
        let size = Vec2::new(8.0, 4.0);
        let (axis, actual_major, actual_minor) = principal_axes(derivatives, size);
        let footprint = Footprint::new(derivatives, size, level);
        assert!(axis.is_finite() && footprint.direction.is_finite() && footprint.lod.is_finite());
        assert!((axis.length() - 1.0).abs() < 1e-6);
        assert!((axis - Vec2::X).length() < 1e-6);
        assert!((actual_major - major).abs() < 1e-5);
        assert!((actual_minor - minor).abs() < 1e-5);
        assert!((footprint.direction - Vec2::new(major / 8.0, 0.0)).length() < 1e-6);
        assert_eq!(footprint.count, level);
        assert!((1..=level).contains(&footprint.count));
        assert!((footprint.lod - lod).abs() < 1e-6);
        assert!((0.0..=3.0).contains(&footprint.lod));
    }
}
