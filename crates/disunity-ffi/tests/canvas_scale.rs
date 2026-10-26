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

fn read(world: u64, canvas: u64) -> DisunityCanvasScale {
    let mut value = DisunityCanvasScale::default();
    assert_eq!(
        unsafe { disunity_canvas_scale_read(world, canvas, &mut value) }.code,
        0
    );
    value
}

fn write(world: u64, canvas: u64, value: DisunityCanvasScale) -> DisunityResult {
    unsafe { disunity_canvas_scale_write(world, canvas, &value) }
}

fn factor(world: u64, canvas: u64, width: u32, height: u32) -> f32 {
    let mut value = 0.;
    assert_eq!(
        unsafe { disunity_canvas_scale_factor(world, canvas, width, height, &mut value) }.code,
        0
    );
    value
}

/// Pong's `CanvasScaler`: scale with screen size from 1920 by 1080, matching width.
fn pong() -> DisunityCanvasScale {
    DisunityCanvasScale {
        mode: 1,
        screen_match: 0,
        factor: 1.,
        reference_x: 1920.,
        reference_y: 1080.,
        match_height: 0.,
    }
}

#[test]
fn screen_size_scaling_round_trips_and_matches_unity_factors() {
    let (world, canvas) = canvas_world();
    assert_eq!(read(world, canvas).mode, 0);
    assert_eq!(factor(world, canvas, 800, 600), 1.);
    assert_eq!(write(world, canvas, pong()).code, 0);
    let stored = read(world, canvas);
    assert_eq!(
        (stored.mode, stored.reference_x, stored.reference_y),
        (1, 1920., 1080.)
    );
    assert_eq!(factor(world, canvas, 1920, 1080), 1.);
    assert_eq!(factor(world, canvas, 960, 540), 0.5);
    assert_eq!(factor(world, canvas, 960, 1080), 0.5, "matching width");
    let height = DisunityCanvasScale {
        match_height: 1.,
        ..pong()
    };
    assert_eq!(write(world, canvas, height).code, 0);
    assert_eq!(factor(world, canvas, 960, 1080), 1., "matching height");
    let halfway = DisunityCanvasScale {
        match_height: 0.5,
        ..pong()
    };
    assert_eq!(write(world, canvas, halfway).code, 0);
    let expected = 0.5_f32.powf(0.5);
    assert!((factor(world, canvas, 960, 1080) - expected).abs() < 1e-6);
    let constant = DisunityCanvasScale {
        mode: 0,
        factor: 2.,
        ..DisunityCanvasScale::default()
    };
    assert_eq!(write(world, canvas, constant).code, 0);
    assert_eq!(factor(world, canvas, 333, 777), 2.);
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn canvas_records_keep_the_scale_they_do_not_carry() {
    let (world, canvas) = canvas_world();
    assert_eq!(write(world, canvas, pong()).code, 0);
    let mut record = DisunityCanvas::default();
    assert_eq!(
        unsafe { disunity_canvas_read(world, canvas, &mut record) }.code,
        0,
        "reading a screen-scaled canvas"
    );
    record.sorting_order = 7;
    assert_eq!(
        unsafe { disunity_canvas_write(world, canvas, &record) }.code,
        0
    );
    assert_eq!(read(world, canvas).reference_x, 1920.);
    assert_eq!(factor(world, canvas, 960, 540), 0.5);
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

/// Scales the native side must reject, and whether each is a typed unsupported field.
fn rejected_scales() -> [(DisunityCanvasScale, bool); 7] {
    [
        (
            DisunityCanvasScale {
                screen_match: 1,
                ..pong()
            },
            true,
        ),
        (
            DisunityCanvasScale {
                screen_match: 2,
                ..pong()
            },
            true,
        ),
        (DisunityCanvasScale { mode: 2, ..pong() }, true),
        (DisunityCanvasScale { mode: 9, ..pong() }, false),
        (
            DisunityCanvasScale {
                reference_x: 0.,
                ..pong()
            },
            false,
        ),
        (
            DisunityCanvasScale {
                match_height: 1.5,
                ..pong()
            },
            false,
        ),
        (
            DisunityCanvasScale {
                mode: 0,
                factor: 0.,
                ..pong()
            },
            false,
        ),
    ]
}

#[test]
fn unsupported_and_invalid_scales_are_rejected_without_changing_the_canvas() {
    let (world, canvas) = canvas_world();
    assert_eq!(write(world, canvas, pong()).code, 0);
    for (value, typed) in rejected_scales() {
        let result = write(world, canvas, value);
        assert_ne!(result.code, 0, "{value:?}");
        if typed {
            assert_eq!(
                result.detail,
                DisunityErrorReason::InvalidComponent as u64,
                "{value:?}"
            );
        }
        assert_eq!(factor(world, canvas, 960, 540), 0.5, "{value:?}");
    }
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn factor_queries_reject_empty_viewports_and_null_output() {
    let (world, canvas) = canvas_world();
    let mut out = 0.;
    for (width, height) in [(0, 540), (960, 0)] {
        let result =
            unsafe { disunity_canvas_scale_factor(world, canvas, width, height, &mut out) };
        assert_eq!(result.code, result_code::INVALID_ARGUMENT);
    }
    let result =
        unsafe { disunity_canvas_scale_factor(world, canvas, 960, 540, std::ptr::null_mut()) };
    assert_eq!(result.code, result_code::INVALID_ARGUMENT);
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}
