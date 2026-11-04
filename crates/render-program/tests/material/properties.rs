//! Setting properties by name: a change shows in the next frame only, a prepared packet keeps
//! the values it was prepared with, and a draw's overrides win over the material.

use worldrunner_render_program::properties::{PropertySet, PropertyValue};

use crate::common::{
    COLOR, DEPTH, Gpu, RGBA8, Rig, SIZE, assert_all, colored, draw_of, full_target_triangle,
    read_target, submit,
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
