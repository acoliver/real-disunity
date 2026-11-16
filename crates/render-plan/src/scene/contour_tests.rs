use super::*;

#[test]
fn contour_defaults_and_all_ids_have_exact_integer_state() {
    let table = SceneContours::default();
    assert_eq!(table.reference_extent(), [1920, 1080]);
    assert!(table.styles().iter().all(Option::is_none));
    assert!(SceneContours::new([1920, 1080], vec![None; 255]).is_err());
    assert!(SceneContours::new([1920, 1080], vec![None; 257]).is_err());
    let mut styles = vec![None; 256];
    styles[255] = Some(SceneContourStyle::default());
    let table = SceneContours::new([1920, 1080], styles.clone()).unwrap();
    assert!(table.validate_id(255).is_ok());
    assert!(table.validate_id(0).is_ok());
    assert!(table.validate_id(1).is_err());
    styles[0] = Some(SceneContourStyle::default());
    assert!(SceneContours::new([1920, 1080], styles.clone()).is_err());
    assert!(SceneContours::new([0, 1080], vec![None; 256]).is_err());
    assert!(SceneContours::new([1920, 0], vec![None; 256]).is_err());
    let object = SceneObject::new(
        SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    assert_eq!(object.contour_id(), 0);
    assert_eq!(object.with_contour_id(255).contour_id(), 255);
    assert_eq!(
        object
            .with_contour_id(255)
            .with_transform(*object.transform())
            .contour_id(),
        255
    );
}

#[test]
fn contour_style_quantity_bounds_are_inclusive() {
    let mut style = SceneContourStyle {
        size_micro: 32_000_000,
        color_micro: [16_000_000, 0, 16_000_000, 1_000_000],
        pulse_period_milliseconds: 1,
        pulse_power_micro: 16_000_000,
        ..SceneContourStyle::default()
    };
    assert!(style.validate().is_ok());
    for field in 0..4 {
        let mut invalid = style;
        invalid.color_micro[field] += 1;
        if field == 1 {
            invalid.color_micro[field] = 16_000_001;
        }
        assert!(invalid.validate().is_err());
    }
    style.size_micro += 1;
    assert!(style.validate().is_err());
    style.size_micro = 0;
    style.pulse_power_micro = 0;
    assert!(style.validate().is_err());
    style.pulse_power_micro = 16_000_001;
    assert!(style.validate().is_err());
    style.pulse_period_milliseconds = 0;
    assert!(style.validate().is_ok(), "steady styles ignore curve power");
}

#[test]
fn contour_fill_quantity_bounds_are_inclusive() {
    let fill = SceneContourFill {
        filler_color_micro: [16_000_000; 4].map(|v| v / 16),
        edge_color_micro: [0; 4],
        inner_width_micro: 32_000_000,
        effect_strength_micro: 1_000_000,
        grain_strength_micro: 1_000_000,
        aberration_shift_micro: 32_000_000,
        aberration_strength_micro: 1_000_000,
        interlacing_strength_micro: 1_000_000,
    };
    assert!(fill.validate().is_ok());
    for field in 0..6 {
        let mut invalid = fill;
        match field {
            0 => invalid.inner_width_micro += 1,
            1 => invalid.effect_strength_micro += 1,
            2 => invalid.grain_strength_micro += 1,
            3 => invalid.aberration_shift_micro += 1,
            4 => invalid.aberration_strength_micro += 1,
            _ => invalid.interlacing_strength_micro += 1,
        }
        assert!(invalid.validate().is_err());
    }
    for field in 0..4 {
        let mut invalid = fill;
        invalid.filler_color_micro[field] = if field == 3 { 1_000_001 } else { 16_000_001 };
        assert!(invalid.validate().is_err());
        let mut invalid = fill;
        invalid.edge_color_micro[field] = if field == 3 { 1_000_001 } else { 16_000_001 };
        assert!(invalid.validate().is_err());
    }
}
