use worldrunner_world_simulation::*;

fn span(n: u64, d: u64) -> TimeSpan {
    TimeSpan::new(n, d).unwrap()
}

fn step(world: &mut World, elapsed: TimeSpan) {
    let mut work = world.advance(elapsed).unwrap();
    loop {
        match work {
            Advance::Callback(c) => {
                work = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::Complete(_) => return,
            _ => panic!("unexpected work"),
        }
    }
}

#[test]
fn a_frame_longer_than_the_maximum_delta_counts_as_the_maximum() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    step(&mut world, span(5, 1));
    assert_eq!(world.clock().delta, TimeSpan::DEFAULT_MAX_DELTA);
    assert_eq!(world.clock().unscaled_delta, TimeSpan::DEFAULT_MAX_DELTA);
    assert_eq!(world.clock().time, TimeSpan::DEFAULT_MAX_DELTA);
    world.set_max_delta(span(1, 10)).unwrap();
    step(&mut world, span(5, 1));
    assert_eq!(world.clock().delta, span(1, 10));
    step(&mut world, span(1, 100));
    assert_eq!(world.clock().delta, span(1, 100));
}

#[test]
fn a_zero_maximum_delta_is_refused() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    assert_eq!(
        world.set_max_delta(TimeSpan::ZERO),
        Err(WorldError::InvalidTime(TimeError::Configuration))
    );
    assert_eq!(world.clock().max_delta, TimeSpan::DEFAULT_MAX_DELTA);
}

#[test]
fn smooth_delta_is_a_fifth_weighted_average_of_scaled_deltas() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    assert_eq!(world.clock().smooth_delta_nanos, 0);
    step(&mut world, span(1, 10));
    assert_eq!(world.clock().smooth_delta_nanos, 100_000_000);
    world.set_time_scale(span(1, 2)).unwrap();
    step(&mut world, span(1, 5));
    assert_eq!(world.clock().smooth_delta_nanos, 100_000_000);
    step(&mut world, span(3, 10));
    assert_eq!(world.clock().smooth_delta_nanos, 110_000_000);
}

#[test]
fn fixed_steps_run_on_scaled_time_after_clamping() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world.set_time_scale(span(1, 2)).unwrap();
    step(&mut world, span(10, 1));
    assert_eq!(world.clock().fixed_time, span(16, 100));
}

#[test]
fn nanos_round_down_and_saturate() {
    assert_eq!(span(1, 3).nanos(), 333_333_333);
    assert_eq!(span(9_007_199_254_740_991, 1).nanos(), u64::MAX);
    assert_eq!(span(18, 1).nanos(), 18_000_000_000);
}
