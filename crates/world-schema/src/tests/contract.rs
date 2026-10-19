use crate::{EnvironmentOrientation, WorldCoordinateFrame};

#[test]
fn frame_has_only_the_standard_engine_basis() {
    assert_eq!(WorldCoordinateFrame::RightHandedYUp.discriminant(), 0);
}

#[test]
fn orientation_is_a_unit_quaternion_in_xyzw_order() {
    let yaw = EnvironmentOrientation::from_yaw_radians(std::f64::consts::FRAC_PI_2);
    assert_eq!(yaw.validate(), Ok(()));
    let expected = std::f64::consts::FRAC_1_SQRT_2;
    for (actual, expected) in yaw
        .quaternion_xyzw
        .iter()
        .zip([0.0, expected, 0.0, expected])
    {
        assert!((actual - expected).abs() < 1e-12);
    }
}

#[test]
fn orientation_rejects_nonfinite_and_nonunit_quaternions() {
    for quaternion_xyzw in [
        [0.0; 4],
        [0.0, 0.0, 0.0, 2.0],
        [f64::NAN; 4],
        [f64::INFINITY; 4],
        [f64::MAX; 4],
    ] {
        assert!(
            EnvironmentOrientation { quaternion_xyzw }
                .validate()
                .is_err()
        );
    }
}

#[test]
fn named_world_positions_map_to_a_standard_right_handed_y_up_basis() {
    let position = worldrunner_core_types::WorldPosition {
        east_meters: 2.0,
        elevation_meters: 3.0,
        north_meters: 5.0,
    };
    assert_eq!(
        WorldCoordinateFrame::RightHandedYUp
            .position_xyz_meters(position)
            .map(f64::to_bits),
        [2.0, 3.0, -5.0].map(f64::to_bits)
    );
}

#[test]
fn yaw_conversion_preserves_unit_length_across_positive_and_negative_turns() {
    for step in -720..=720 {
        let angle = f64::from(step) * std::f64::consts::PI / 180.0;
        assert_eq!(
            EnvironmentOrientation::from_yaw_radians(angle).validate(),
            Ok(())
        );
    }
}
