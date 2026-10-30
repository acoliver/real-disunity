use worldrunner_disunity_ffi::{abi::*, canvas::*, ui::*};

fn canvas_world() -> (u64, u64) {
    let (mut world, mut object, mut canvas) = (0, 0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(disunity_ui_create(world, object, 1, 0, &mut canvas).code, 0);
    }
    (world, canvas)
}

fn read(world: u64, canvas: u64) -> DisunityCanvas {
    let mut record = DisunityCanvas::default();
    assert_eq!(
        unsafe { disunity_canvas_read(world, canvas, &mut record) }.code,
        0
    );
    record
}

fn write(world: u64, canvas: u64, record: DisunityCanvas) -> DisunityResult {
    unsafe { disunity_canvas_write(world, canvas, &record) }
}

#[test]
fn a_new_canvas_reports_the_default_settings() {
    let (world, canvas) = canvas_world();
    let record = read(world, canvas);
    assert_eq!(
        (
            record.sorting_layer,
            record.override_sorting,
            record.target_display,
            record.additional_shader_channels,
            record.reference_pixels_per_unit
        ),
        (0, 0, 0, 0, 100.)
    );
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn settings_round_trip_and_survive_other_canvas_writes() {
    let (world, canvas) = canvas_world();
    let mut record = read(world, canvas);
    record.sorting_layer = -3;
    record.override_sorting = 1;
    record.target_display = 2;
    record.additional_shader_channels = 0b10_101;
    record.reference_pixels_per_unit = 32.;
    assert_eq!(write(world, canvas, record).code, 0);
    let stored = read(world, canvas);
    assert_eq!(
        (
            stored.sorting_layer,
            stored.override_sorting,
            stored.target_display,
            stored.additional_shader_channels,
            stored.reference_pixels_per_unit
        ),
        (-3, 1, 2, 0b10_101, 32.)
    );
    let scale = DisunityCanvasScale {
        factor: 2.,
        ..Default::default()
    };
    assert_eq!(
        unsafe { disunity_canvas_scale_write(world, canvas, &scale) }.code,
        0
    );
    assert_eq!(
        disunity_canvas_pixel_perfect_write(world, canvas, 1).code,
        0
    );
    assert_eq!(read(world, canvas).additional_shader_channels, 0b10_101);
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn unusable_settings_are_rejected_and_leave_the_canvas_unchanged() {
    let (world, canvas) = canvas_world();
    let good = read(world, canvas);
    let bad = [
        DisunityCanvas {
            override_sorting: 2,
            ..good
        },
        DisunityCanvas {
            reference_pixels_per_unit: 0.,
            ..good
        },
        DisunityCanvas {
            reference_pixels_per_unit: f32::NAN,
            ..good
        },
        DisunityCanvas {
            additional_shader_channels: 32,
            ..good
        },
    ];
    for record in bad {
        assert_ne!(write(world, canvas, record).code, 0, "{record:?}");
    }
    let after = read(world, canvas);
    assert_eq!(
        (after.override_sorting, after.reference_pixels_per_unit),
        (0, 100.)
    );
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}
