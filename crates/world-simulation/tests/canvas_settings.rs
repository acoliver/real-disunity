#![cfg(feature = "ui")]
//! Canvas settings the engine stores for the managed API: defaults, round trip and validation.
use worldrunner_world_simulation::ui::*;
use worldrunner_world_simulation::{ComponentDesc, ObjectDesc, World, WorldError, WorldSettings};

fn world_with_canvas() -> (World, worldrunner_world_simulation::ComponentHandle) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            object,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
        .unwrap();
    let handle = world
        .add_component(object, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    (world, handle)
}

#[test]
fn a_new_canvas_does_not_override_sorting_and_has_100_reference_pixels() {
    let canvas = Canvas::default();
    assert!(!canvas.override_sorting);
    assert_eq!(canvas.target_display, 0);
    assert_eq!(canvas.additional_shader_channels, 0);
    assert_eq!(
        canvas.reference_pixels_per_unit.to_bits(),
        100_f32.to_bits()
    );
}

#[test]
fn the_world_stores_the_canvas_settings() {
    let (mut world, handle) = world_with_canvas();
    let settings = Canvas {
        override_sorting: true,
        target_display: 3,
        additional_shader_channels: 0b1_1111,
        reference_pixels_per_unit: 64.,
        sorting_layer: -7,
        ..Canvas::default()
    };
    world
        .set_component(handle, ComponentDesc::Canvas(settings))
        .unwrap();
    assert_eq!(
        world.component(handle).unwrap().desc,
        ComponentDesc::Canvas(settings)
    );
}

#[test]
fn the_world_rejects_unusable_reference_pixels_and_unknown_channels() {
    let (mut world, handle) = world_with_canvas();
    for bad in [
        Canvas {
            reference_pixels_per_unit: 0.,
            ..Canvas::default()
        },
        Canvas {
            reference_pixels_per_unit: -1.,
            ..Canvas::default()
        },
        Canvas {
            reference_pixels_per_unit: f32::NAN,
            ..Canvas::default()
        },
        Canvas {
            additional_shader_channels: 0b10_0000,
            ..Canvas::default()
        },
    ] {
        assert_eq!(
            world.set_component(handle, ComponentDesc::Canvas(bad)),
            Err(WorldError::Ui(UiError::InvalidCanvas)),
            "{bad:?}"
        );
    }
    assert_eq!(
        world.component(handle).unwrap().desc,
        ComponentDesc::Canvas(Canvas::default())
    );
}
