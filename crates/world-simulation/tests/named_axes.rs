#[test]
fn opposing_duplicate_names_choose_magnitude_then_lowest_id() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    for id in [10, 11] {
        world
            .register_input_axis(AxisId(id), axis("move", 1, 2))
            .unwrap();
    }
    event(&mut world, 0, InputAction::Axis(AxisId(10), -0.25));
    event(&mut world, 0, InputAction::Axis(AxisId(11), 0.75));
    frame(&mut world);
    assert_eq!(
        world.input_axis(InputPhase::Dynamic, "move", true),
        Ok(0.75)
    );
    event(&mut world, 11, InputAction::Axis(AxisId(10), -0.75));
    frame(&mut world);
    assert_eq!(
        world.input_axis(InputPhase::Dynamic, "move", true),
        Ok(-0.75)
    );
}

#[test]
fn axis_registry_capacity_and_replacement_preserve_existing_entries() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    for id in 0..256 {
        world
            .register_input_axis(AxisId(id), axis(&format!("axis{id}"), 1, 2))
            .unwrap();
    }
    assert!(
        world
            .register_input_axis(AxisId(256), axis("overflow", 1, 2))
            .is_err()
    );
    world
        .register_input_axis(AxisId(10), axis("replacement", 3, 4))
        .unwrap();
    assert_eq!(world.input_axis_id("replacement"), Ok(AxisId(10)));
    assert!(world.input_axis_id("axis10").is_err());
    assert_eq!(world.input_axis_id("axis255"), Ok(AxisId(255)));
    event(&mut world, 0, InputAction::Key(KeyCode(4), true));
    frame(&mut world);
    assert_eq!(
        world.input_axis(InputPhase::Dynamic, "replacement", true),
        Ok(1.0)
    );
}

use worldrunner_world_simulation::{
    Advance, AxisBinding, AxisId, InputAction, InputAxis, InputEvent, InputPhase, KeyCode,
    TimeSpan, World, WorldError, WorldSettings,
};

fn axis(name: &str, negative: u16, positive: u16) -> InputAxis {
    InputAxis {
        name: name.into(),
        binding: AxisBinding {
            negative: vec![KeyCode(negative)],
            positive: vec![KeyCode(positive)],
            invert: false,
        },
        gravity: 2.0,
        sensitivity: 2.0,
        dead: 0.001,
        snap: true,
    }
}
fn frame(world: &mut World) {
    assert!(matches!(
        world.advance(TimeSpan::new(1, 100).unwrap()).unwrap(),
        Advance::Complete(_)
    ));
}
fn event(world: &mut World, tick: u64, action: InputAction) {
    world
        .push_input(&[InputEvent {
            timestamp: TimeSpan::new(tick, 1000).unwrap(),
            action,
        }])
        .unwrap();
}

#[test]
fn independent_names_raw_smoothing_alternates_and_focus() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let mut first = axis("first", 1, 2);
    first.binding.positive.push(KeyCode(5));
    world.register_input_axis(AxisId(10), first).unwrap();
    world
        .register_input_axis(AxisId(11), axis("second", 3, 4))
        .unwrap();
    event(&mut world, 0, InputAction::Key(KeyCode(5), true));
    event(&mut world, 0, InputAction::Key(KeyCode(3), true));
    frame(&mut world);
    assert_eq!(
        world.input_axis(InputPhase::Dynamic, "first", true),
        Ok(1.0)
    );
    assert_eq!(
        world.input_axis(InputPhase::Dynamic, "second", true),
        Ok(-1.0)
    );
    assert!(
        (world
            .input_axis(InputPhase::Dynamic, "first", false)
            .unwrap()
            - 0.02)
            .abs()
            < 1e-6
    );
    assert_eq!(
        world.input_axis(InputPhase::Dynamic, "missing", true),
        Err(WorldError::InvalidInput)
    );
    event(&mut world, 11, InputAction::Focus(false));
    frame(&mut world);
    assert_eq!(
        world.input_axis(InputPhase::Dynamic, "first", false),
        Ok(0.0)
    );
    assert_eq!(
        world.input_axis(InputPhase::Dynamic, "second", true),
        Ok(0.0)
    );
    assert_eq!(world.input_axis(InputPhase::Fixed, "first", false), Ok(0.0));
}

#[test]
fn snap_gravity_invert_dead_zone_and_duplicate_names() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let mut config = axis("move", 1, 2);
    config.binding.invert = true;
    world
        .register_input_axis(AxisId(10), config.clone())
        .unwrap();
    event(&mut world, 0, InputAction::Key(KeyCode(2), true));
    frame(&mut world);
    assert_eq!(
        world.input_axis(InputPhase::Dynamic, "move", true),
        Ok(-1.0)
    );
    event(&mut world, 11, InputAction::Key(KeyCode(2), false));
    event(&mut world, 11, InputAction::Key(KeyCode(1), true));
    frame(&mut world);
    assert!(
        (world
            .input_axis(InputPhase::Dynamic, "move", false)
            .unwrap()
            - 0.02)
            .abs()
            < 1e-6
    );
    event(&mut world, 21, InputAction::Key(KeyCode(1), false));
    frame(&mut world);
    assert_eq!(
        world.input_axis(InputPhase::Dynamic, "move", false),
        Ok(0.0)
    );
    world
        .register_input_axis(AxisId(11), axis("move", 3, 4))
        .unwrap();
    event(&mut world, 31, InputAction::Axis(AxisId(11), 0.0001));
    frame(&mut world);
    assert_eq!(world.input_axis(InputPhase::Dynamic, "move", true), Ok(0.0));
    event(&mut world, 41, InputAction::Axis(AxisId(11), 0.75));
    frame(&mut world);
    assert_eq!(
        world.input_axis(InputPhase::Dynamic, "move", true),
        Ok(0.75)
    );
    config.gravity = f32::NAN;
    assert!(world.register_input_axis(AxisId(10), config).is_err());
    assert_eq!(
        world.input_axis(InputPhase::Dynamic, "move", true),
        Ok(0.75)
    );
}

#[test]
fn reads_are_stable_and_fixed_dynamic_smoothing_is_independent() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world
        .register_input_axis(AxisId(10), axis("move", 1, 2))
        .unwrap();
    event(&mut world, 0, InputAction::Key(KeyCode(2), true));
    frame(&mut world);
    let first = world
        .input_axis(InputPhase::Dynamic, "move", false)
        .unwrap();
    assert_eq!(
        world.input_axis(InputPhase::Dynamic, "move", false),
        Ok(first)
    );
    assert_eq!(world.input_axis(InputPhase::Fixed, "move", false), Ok(0.0));
    frame(&mut world);
    let fixed = world.input_axis(InputPhase::Fixed, "move", false).unwrap();
    let dynamic = world
        .input_axis(InputPhase::Dynamic, "move", false)
        .unwrap();
    assert!((fixed - 0.04).abs() < 1e-6);
    assert!((dynamic - fixed).abs() < 1e-6);
}
