//! Interpolated 2D bodies are drawn between their poses either side of the last physics step,
//! by the fraction of a step the clock has run past it, as Unity does. Physics keeps the step pose.
use glam::{Vec2, Vec3};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, ObjectHandle, ParentSpace, TimeSpan, World,
    WorldSettings,
    physics2d::{Body2d, Interpolation},
    presentation::{
        Sprite, SpriteRenderer,
        extraction::{SpriteTextures, extract_world},
    },
};

/// A body moving 10 m/s along x from the origin, with a unit sprite, and its component.
fn moving(
    world: &mut World,
    interpolation: Interpolation,
) -> (ObjectHandle, worldrunner_world_simulation::ComponentHandle) {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let body = world
        .add_component(
            object,
            ComponentDesc::Body2d(Body2d {
                velocity: Vec2::new(10.0, 0.0),
                gravity_scale: 0.0,
                interpolation,
                ..Default::default()
            }),
        )
        .unwrap();
    world
        .add_component(
            object,
            ComponentDesc::Sprite(SpriteRenderer {
                sprite: Some(Sprite {
                    identity: 0,
                    texture: 1,
                    texture_size: [1, 1],
                    rect: [0., 0., 1., 1.],
                    pixels_per_unit: 1.,
                    pivot: [0.5, 0.5],
                }),
                ..Default::default()
            }),
        )
        .unwrap();
    (object, body)
}

/// Advances one frame of `hundredths` of a second; the fixed step is 1/50 s.
fn frame(world: &mut World, hundredths: u64) {
    world
        .advance(TimeSpan::new(hundredths, 100).unwrap())
        .unwrap();
}

fn x(world: &World, object: ObjectHandle) -> [String; 2] {
    let drawn = world.presented_matrix(object).unwrap().w_axis.x;
    let physics = world.world_matrix(object).unwrap().w_axis.x;
    [format!("{drawn:.3}"), format!("{physics:.3}")]
}

#[test]
fn interpolated_bodies_draw_between_their_last_two_step_poses() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (body, _) = moving(&mut world, Interpolation::Interpolate);
    let (fixed, _) = moving(&mut world, Interpolation::None);
    frame(&mut world, 1);
    assert_eq!(x(&world, body), ["0.000", "0.000"], "no step has run yet");
    frame(&mut world, 1);
    // One step moved it 0.2 m; the clock is exactly at the step, so it draws the pose before.
    assert_eq!(x(&world, body), ["0.000", "0.200"]);
    frame(&mut world, 1);
    // Half a step past it: halfway between the two step poses.
    assert_eq!(x(&world, body), ["0.100", "0.200"]);
    assert_eq!(
        x(&world, fixed),
        ["0.200", "0.200"],
        "without interpolation it draws the step pose"
    );

    let plan = extract_world(&world, &SpriteTextures::default(), [10, 10]).unwrap();
    let centres: Vec<String> = plan
        .quads
        .iter()
        .map(|q| format!("{:.3}", q.corners.iter().map(|c| c[0]).sum::<f32>() / 4.0))
        .collect();
    assert_eq!(
        centres,
        ["0.100", "0.200"],
        "sprites are extracted at the drawn pose"
    );
}

#[test]
fn children_follow_the_drawn_pose_and_teleports_draw_where_they_land() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (body, component) = moving(&mut world, Interpolation::Interpolate);
    let child = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(child, Some(body), ParentSpace::KeepLocal)
        .unwrap();
    world
        .set_local_transform(
            child,
            LocalTransform {
                translation: Vec3::new(1.0, 0.0, 0.0),
                ..Default::default()
            },
        )
        .unwrap();
    frame(&mut world, 2);
    frame(&mut world, 1);
    assert_eq!(
        x(&world, child),
        ["1.100", "1.200"],
        "a child keeps its offset from the drawn body"
    );

    world
        .set_local_transform(
            body,
            LocalTransform {
                translation: Vec3::new(5.0, 0.0, 0.0),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        x(&world, body),
        ["5.000", "5.000"],
        "a body moved outside physics draws where it is"
    );

    frame(&mut world, 1);
    let ComponentDesc::Body2d(mut settings) = world.component(component).unwrap().desc.clone()
    else {
        unreachable!()
    };
    settings.interpolation = Interpolation::None;
    world
        .set_component(component, ComponentDesc::Body2d(settings))
        .unwrap();
    let [drawn, physics] = x(&world, body);
    assert_eq!(
        drawn, physics,
        "turning interpolation off draws the step pose at once"
    );
}
