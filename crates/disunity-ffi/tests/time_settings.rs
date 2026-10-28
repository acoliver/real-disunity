//! Time.timeScale, Time.maximumDeltaTime, Time.unscaledDeltaTime and Time.smoothDeltaTime through the ABI.
use worldrunner_disunity_ffi::{abi::*, scheduling::*};

fn frame(world: u64, nanos: u64) {
    let mut work = DisunityAdvance::default();
    let result = unsafe { disunity_world_advance(world, nanos, 1_000_000_000, &mut work) };
    assert_eq!(result.code, 0);
    assert_eq!(work.kind, 0);
}

fn read(world: u64) -> (DisunityClock, DisunityTime) {
    let (mut clock, mut time) = (DisunityClock::default(), DisunityTime::default());
    unsafe {
        assert_eq!(disunity_world_clock(world, &mut clock).code, 0);
        assert_eq!(disunity_world_time_read(world, &mut time).code, 0);
    }
    (clock, time)
}

fn write(world: u64, setting: u32, numerator: u64, denominator: u64) -> i32 {
    disunity_world_time_write(world, setting, numerator, denominator).code
}

fn destroy(world: u64) {
    assert_eq!(unsafe { disunity_world_destroy(world) }.code, 0);
}

fn world() -> u64 {
    let mut world = 0;
    assert_eq!(unsafe { disunity_world_create(&mut world) }.code, 0);
    world
}

#[test]
fn defaults_are_scale_one_and_a_third_of_a_second() {
    let world = world();
    let (_, time) = read(world);
    assert_eq!(time.time_scale, 1.0);
    assert!((time.maximum_delta - 0.333_333_333).abs() < 1e-12);
    destroy(world);
}

#[test]
fn scale_halves_delta_and_time_but_not_the_unscaled_values() {
    let world = world();
    assert_eq!(write(world, time_setting::TIME_SCALE, 50, 100), 0);
    frame(world, 20_000_000);
    frame(world, 20_000_000);
    let (clock, time) = read(world);
    assert!((clock.delta - 0.01).abs() < 1e-12);
    assert!((clock.time - 0.02).abs() < 1e-12);
    assert!((time.unscaled_delta - 0.02).abs() < 1e-12);
    assert!((time.unscaled_time - 0.04).abs() < 1e-12);
    assert_eq!(time.time_scale, 0.5);
    destroy(world);
}

#[test]
fn scale_denominators_above_one_hundred_are_refused() {
    let world = world();
    assert_ne!(write(world, time_setting::TIME_SCALE, 333, 1000), 0);
    assert_eq!(read(world).1.time_scale, 1.0);
    destroy(world);
}

#[test]
fn a_hundredth_scale_keeps_frames_advancing() {
    let world = world();
    assert_eq!(write(world, time_setting::TIME_SCALE, 33, 100), 0);
    assert_eq!(read(world).1.time_scale, 0.33);
    for _ in 0..200 {
        frame(world, 16_666_667);
    }
    assert_eq!(read(world).0.frame, 200);
    destroy(world);
}

#[test]
fn a_long_frame_counts_as_the_maximum_delta() {
    let world = world();
    frame(world, 2_000_000_000);
    let (clock, time) = read(world);
    assert!((clock.delta - 0.333_333_333).abs() < 1e-12);
    assert!((time.unscaled_delta - 0.333_333_333).abs() < 1e-12);
    assert_eq!(
        write(
            world,
            time_setting::MAXIMUM_DELTA,
            100_000_000,
            1_000_000_000
        ),
        0
    );
    frame(world, 2_000_000_000);
    assert!((read(world).0.delta - 0.1).abs() < 1e-12);
    destroy(world);
}

#[test]
fn smooth_delta_moves_a_fifth_of_the_way_to_each_frame() {
    let world = world();
    frame(world, 100_000_000);
    assert!((read(world).1.smooth_delta - 0.1).abs() < 1e-12);
    frame(world, 200_000_000);
    assert!((read(world).1.smooth_delta - 0.12).abs() < 1e-9);
    destroy(world);
}

#[test]
fn invalid_settings_are_refused_and_change_nothing() {
    let world = world();
    for (setting, numerator, denominator) in [
        (time_setting::TIME_SCALE, 101, 1),
        (time_setting::TIME_SCALE, 1, 0),
        (time_setting::MAXIMUM_DELTA, 0, 1),
        (time_setting::MAXIMUM_DELTA, 1, 0),
        (7, 1, 1),
    ] {
        assert_ne!(
            write(world, setting, numerator, denominator),
            0,
            "{setting} {numerator}/{denominator}"
        );
    }
    let (_, time) = read(world);
    assert_eq!(time.time_scale, 1.0);
    assert!((time.maximum_delta - 0.333_333_333).abs() < 1e-12);
    destroy(world);
}
