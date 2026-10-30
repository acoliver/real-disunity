use glam::Vec3;
use worldrunner_world_simulation::{
    LocalTransform, ObjectDesc, ObjectEdit, ObjectHandle, World, WorldSettings,
};

fn tracked() -> World {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world.track_changes();
    world.enable_spatial_index(4.0).unwrap();
    world
}

fn at(world: &mut World, position: Vec3) -> ObjectHandle {
    world
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: position,
                ..LocalTransform::default()
            },
            ..ObjectDesc::default()
        })
        .unwrap()
}

fn chain(world: &mut World) -> [ObjectHandle; 3] {
    let target = at(world, Vec3::ZERO);
    let near = at(world, Vec3::new(1.0, 0.0, 0.0));
    let far = at(world, Vec3::new(2.0, 0.0, 0.0));
    world.set_follow_target(near, Some(target)).unwrap();
    world.set_follow_target(far, Some(near)).unwrap();
    world.drain_changes();
    world.refresh_spatial_index();
    [target, near, far]
}

fn moved(target: ObjectHandle) -> [ObjectEdit; 1] {
    [ObjectEdit::Translation(target, Vec3::new(100.0, 0.0, 0.0))]
}

#[test]
fn journal_records_the_target_and_each_shifted_follower() {
    let mut world = tracked();
    let [target, near, far] = chain(&mut world);
    world.apply_edits(&moved(target)).unwrap();
    let changes = world.drain_changes();
    assert_eq!(changes.transforms, vec![target, near, far]);
    assert!(changes.created.is_empty() && changes.parents.is_empty());
}

#[test]
fn spatial_queries_see_shifted_followers() {
    let mut world = tracked();
    let [target, near, far] = chain(&mut world);
    world.apply_edits(&moved(target)).unwrap();
    assert_eq!(
        world
            .objects_in_radius(Vec3::new(101.0, 0.0, 0.0), 1.5)
            .unwrap(),
        vec![target, near, far]
    );
    assert!(
        world
            .objects_in_radius(Vec3::new(1.0, 0.0, 0.0), 3.0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn failed_staged_batch_restores_followers_links_and_journal() {
    let mut world = tracked();
    let [target, near, far] = chain(&mut world);
    let before = world.snapshot();

    assert!(world.step_with_edits(&moved(target), -1.0).is_err());

    assert_eq!(world.snapshot(), before);
    assert_eq!(world.follow_target(near).unwrap(), Some(target));
    assert_eq!(world.follow_target(far).unwrap(), Some(near));
    assert_eq!(world.followers(target).unwrap(), vec![near]);
    assert!(world.drain_changes().is_empty());
    assert_eq!(
        world
            .objects_in_radius(Vec3::new(2.0, 0.0, 0.0), 0.5)
            .unwrap(),
        vec![far]
    );
    assert!(
        world
            .objects_in_radius(Vec3::new(102.0, 0.0, 0.0), 0.5)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_rejected_shift_leaves_the_world_unchanged() {
    let mut world = tracked();
    let target = at(&mut world, Vec3::ZERO);
    let follower = at(&mut world, Vec3::splat(f32::MAX));
    world.set_follow_target(follower, Some(target)).unwrap();
    world.drain_changes();
    let before = world.snapshot();

    let overflow = LocalTransform {
        translation: Vec3::splat(f32::MAX),
        ..LocalTransform::default()
    };
    assert!(world.set_local_transform(target, overflow).is_err());

    assert_eq!(world.snapshot(), before);
    assert!(world.drain_changes().is_empty());
}
