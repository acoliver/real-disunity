//! Setting properties by name: a change shows in the next frame only, a prepared packet keeps
//! the values it was prepared with, and a draw's overrides win over the material.

use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::registry::ProgramDesc;
use worldrunner_render_program::state::RenderState;
use worldrunner_render_program::variant::VariantError;

use crate::common::{
    COLOR, DEPTH, Gpu, RGBA8, Rig, SIZE, assert_all, colored, draw_of, features,
    full_target_triangle, read_target, submit, variant,
};

const NEW: [f32; 4] = [0.75, 0.25, 0.5, 1.0];

#[test]
fn a_changed_property_shows_in_the_next_frame_and_not_in_a_packet_prepared_before() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let second = rig
        .runtime
        .create_render_target("second", [SIZE, SIZE], RGBA8, Some(DEPTH))
        .unwrap();
    let triangle = full_target_triangle(&mut rig.runtime);
    let draw = draw_of(rig.material, triangle, 3);
    let (first_target, material) = (rig.target, rig.material);

    submit(&mut rig.runtime, |runtime, frame| {
        runtime.clear_target(frame, second, [0.0; 4], 1.0)?;
        let before = runtime.prepare_material_draw(frame, &draw)?;
        runtime.set_property(material, "color", PropertyValue::Vector(NEW))?;
        let after = runtime.prepare_material_draw(frame, &draw)?;
        runtime.record_material(frame, first_target, &before)?;
        runtime.record_material(frame, second, &after)
    })
    .unwrap();
    assert_all(&read_target(&mut rig.runtime, first_target).unwrap(), COLOR);
    assert_all(&read_target(&mut rig.runtime, second).unwrap(), NEW);

    let again = rig.render(&draw).unwrap();
    assert_all(&again, NEW);
}

#[test]
fn a_packet_draws_its_prepared_values_again_after_the_material_changes_twice() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let triangle = full_target_triangle(&mut rig.runtime);
    let draw = draw_of(rig.material, triangle, 3);
    let (target, material) = (rig.target, rig.material);

    let (packet, _) = submit(&mut rig.runtime, |runtime, frame| {
        runtime.prepare_material_draw(frame, &draw)
    })
    .unwrap();
    rig.runtime
        .set_property(material, "color", PropertyValue::Vector(NEW))
        .unwrap();
    rig.runtime
        .set_property(
            material,
            "color",
            PropertyValue::Vector([0.0, 1.0, 0.0, 1.0]),
        )
        .unwrap();
    submit(&mut rig.runtime, |runtime, frame| {
        runtime.record_material(frame, target, &packet)
    })
    .unwrap();
    assert_all(&rig.read().unwrap(), COLOR);
}

#[test]
fn a_per_draw_override_wins_over_the_material_for_that_draw_only() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let triangle = full_target_triangle(&mut rig.runtime);
    let mut draw = draw_of(rig.material, triangle, 3);
    draw.overrides = colored(NEW);
    assert_all(&rig.render(&draw).unwrap(), NEW);

    draw.overrides = PropertySet::new();
    assert_all(&rig.render(&draw).unwrap(), COLOR);
}

/// Renders the rig's material over the whole target and checks it is `want`.
fn assert_renders(rig: &mut Rig, want: [f32; 4]) {
    let triangle = full_target_triangle(&mut rig.runtime);
    let draw = draw_of(rig.material, triangle, 3);
    assert_all(&rig.render(&draw).unwrap(), want);
}

#[test]
fn an_unknown_feature_fails_by_name_and_leaves_the_enabled_features_as_they_were() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let material = rig.material;
    assert_eq!(
        rig.runtime.set_features(material, &["TRIPLE"]).unwrap_err(),
        ProgramError::Variant(VariantError::UnknownFeature {
            name: "TRIPLE".to_owned()
        })
    );
    assert_renders(&mut rig, COLOR);

    rig.runtime.set_features(material, &["DOUBLE"]).unwrap();
    assert!(
        rig.runtime
            .set_features(material, &["DOUBLE", "BLOOM"])
            .is_err()
    );
    assert_renders(&mut rig, [0.5, 0.5, 0.75, 1.0]);
}

#[test]
fn a_feature_combination_no_variant_has_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let program = runtime
        .register_program(ProgramDesc {
            label: "paired".to_owned(),
            state: RenderState::opaque_depth(),
            variants: vec![
                variant(&[], "vs", "fs"),
                variant(&["A"], "vs", "fs"),
                variant(&["B"], "vs", "fs_double"),
            ],
        })
        .unwrap();
    let material = runtime.create_material(program, colored(COLOR)).unwrap();
    assert_eq!(
        runtime.set_features(material, &["A", "B"]).unwrap_err(),
        ProgramError::Variant(VariantError::NoMatch {
            wanted: features(&["A", "B"]).into_iter().collect(),
            available: 3
        })
    );
    runtime.set_features(material, &["B", "B"]).unwrap();
}

#[test]
fn a_value_of_the_wrong_kind_is_refused_by_property_name_and_the_old_value_still_draws() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let material = rig.material;
    rig.runtime
        .set_property(material, "color", PropertyValue::Vector(NEW))
        .unwrap();
    for wrong in [
        PropertyValue::Int(3),
        PropertyValue::Float(1.0),
        PropertyValue::Bool(true),
        PropertyValue::Matrix([0.0; 16]),
    ] {
        let error = rig
            .runtime
            .set_property(material, "color", wrong)
            .unwrap_err();
        assert!(
            matches!(&error, ProgramError::WrongPropertyKind { property, .. } if property == "color"),
            "{error:?}"
        );
        assert!(error.to_string().contains("color"), "{error}");
    }
    assert_renders(&mut rig, NEW);
}

#[test]
fn an_unknown_property_is_refused_by_name_when_set_created_with_or_overridden() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let unknown = ProgramError::UnknownProperty {
        program: "tint".to_owned(),
        property: "colour".to_owned(),
    };
    assert_eq!(
        rig.runtime
            .set_property(rig.material, "colour", PropertyValue::Vector(NEW))
            .unwrap_err(),
        unknown
    );
    let mut bad = colored(NEW);
    bad.set("colour", PropertyValue::Vector(NEW));
    assert_eq!(
        rig.runtime.create_material(rig.program, bad).unwrap_err(),
        unknown
    );
    let mut wrong = PropertySet::new();
    wrong.set("color", PropertyValue::Int(1));
    assert!(matches!(
        rig.runtime.create_material(rig.program, wrong).unwrap_err(),
        ProgramError::WrongPropertyKind { .. }
    ));

    let triangle = full_target_triangle(&mut rig.runtime);
    let mut draw = draw_of(rig.material, triangle, 3);
    draw.overrides.set("colour", PropertyValue::Vector(NEW));
    assert_eq!(rig.render(&draw).unwrap_err(), unknown);
    draw.overrides = PropertySet::new();
    assert_all(&rig.render(&draw).unwrap(), COLOR);
}

#[test]
fn handles_from_another_runtime_or_a_released_material_are_refused() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let mut other = Rig::new(&gpu);
    let foreign = ProgramError::UnknownHandle { kind: "material" };
    assert_eq!(
        other.runtime.set_features(rig.material, &[]).unwrap_err(),
        foreign
    );
    assert_eq!(
        other
            .runtime
            .set_property(rig.material, "color", PropertyValue::Vector(NEW))
            .unwrap_err(),
        foreign
    );
    assert_eq!(
        other
            .runtime
            .create_material(rig.program, PropertySet::new())
            .unwrap_err(),
        ProgramError::UnknownHandle { kind: "program" }
    );

    assert!(rig.runtime.release_material(rig.material));
    assert!(!rig.runtime.release_material(rig.material));
    assert_eq!(
        rig.runtime
            .set_property(rig.material, "color", PropertyValue::Vector(NEW))
            .unwrap_err(),
        foreign
    );
}
