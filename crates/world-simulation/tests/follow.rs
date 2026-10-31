use glam::{Quat, Vec3};
use worldrunner_world_simulation::{
    LocalTransform, ObjectDesc, ObjectEdit, ObjectHandle, ParentSpace, World, WorldError,
    WorldSettings,
};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
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

fn translation(world: &World, object: ObjectHandle) -> Vec3 {
    world.local_transform(object).unwrap().translation
}

fn move_to(world: &mut World, object: ObjectHandle, position: Vec3) {
    let value = LocalTransform {
        translation: position,
        ..world.local_transform(object).unwrap()
    };
    world.set_local_transform(object, value).unwrap();
}

#[test]
fn follower_shifts_by_the_targets_displacement() {
    let mut world = world();
    let target = at(&mut world, Vec3::new(1.0, 0.0, 0.0));
    let follower = at(&mut world, Vec3::new(10.0, 5.0, 0.0));
    world.set_follow_target(follower, Some(target)).unwrap();
    assert_eq!(world.follow_target(follower).unwrap(), Some(target));
    assert_eq!(world.followers(target).unwrap(), vec![follower]);

    move_to(&mut world, target, Vec3::new(4.0, 2.0, -1.0));

    assert_eq!(translation(&world, follower), Vec3::new(13.0, 7.0, -1.0));
    assert_eq!(translation(&world, target), Vec3::new(4.0, 2.0, -1.0));
}

#[test]
fn followers_of_followers_shift_too() {
    let mut world = world();
    let a = at(&mut world, Vec3::ZERO);
    let b = at(&mut world, Vec3::new(0.0, 10.0, 0.0));
    let c = at(&mut world, Vec3::new(0.0, 20.0, 0.0));
    world.set_follow_target(b, Some(a)).unwrap();
    world.set_follow_target(c, Some(b)).unwrap();

    move_to(&mut world, a, Vec3::new(3.0, 0.0, 0.0));
    assert_eq!(translation(&world, b), Vec3::new(3.0, 10.0, 0.0));
    assert_eq!(translation(&world, c), Vec3::new(3.0, 20.0, 0.0));

    move_to(&mut world, b, Vec3::new(3.0, 11.0, 0.0));
    assert_eq!(translation(&world, a), Vec3::new(3.0, 0.0, 0.0));
    assert_eq!(translation(&world, c), Vec3::new(3.0, 21.0, 0.0));
}

#[test]
fn a_follow_cycle_shifts_each_object_once() {
    let mut world = world();
    let a = at(&mut world, Vec3::ZERO);
    let b = at(&mut world, Vec3::new(5.0, 0.0, 0.0));
    let c = at(&mut world, Vec3::new(9.0, 0.0, 0.0));
    world.set_follow_target(a, Some(b)).unwrap();
    world.set_follow_target(b, Some(a)).unwrap();
    world.set_follow_target(c, Some(b)).unwrap();

    move_to(&mut world, a, Vec3::new(2.0, 0.0, 0.0));

    assert_eq!(translation(&world, a), Vec3::new(2.0, 0.0, 0.0));
    assert_eq!(translation(&world, b), Vec3::new(7.0, 0.0, 0.0));
    assert_eq!(translation(&world, c), Vec3::new(11.0, 0.0, 0.0));
}

#[test]
fn setting_a_target_replaces_and_none_clears_it() {
    let mut world = world();
    let first = at(&mut world, Vec3::ZERO);
    let second = at(&mut world, Vec3::ZERO);
    let follower = at(&mut world, Vec3::ZERO);
    world.set_follow_target(follower, Some(first)).unwrap();
    world.set_follow_target(follower, Some(second)).unwrap();
    assert!(world.followers(first).unwrap().is_empty());
    assert_eq!(world.followers(second).unwrap(), vec![follower]);

    move_to(&mut world, first, Vec3::X);
    assert_eq!(translation(&world, follower), Vec3::ZERO);
    move_to(&mut world, second, Vec3::Y);
    assert_eq!(translation(&world, follower), Vec3::Y);

    world.set_follow_target(follower, None).unwrap();
    assert_eq!(world.follow_target(follower).unwrap(), None);
    assert!(world.followers(second).unwrap().is_empty());
    move_to(&mut world, second, Vec3::Z);
    assert_eq!(translation(&world, follower), Vec3::Y);
}

#[test]
fn an_object_cannot_follow_itself_or_a_stale_object() {
    let mut world = world();
    let object = at(&mut world, Vec3::ZERO);
    let other = at(&mut world, Vec3::ZERO);
    assert_eq!(
        world.set_follow_target(object, Some(object)),
        Err(WorldError::FollowSelf)
    );
    world.destroy_object(other).unwrap();
    assert_eq!(
        world.set_follow_target(object, Some(other)),
        Err(WorldError::StaleObject(other))
    );
    assert_eq!(world.follow_target(object).unwrap(), None);
}

#[test]
fn destroying_a_target_clears_its_followers_links() {
    let mut world = world();
    let target = at(&mut world, Vec3::ZERO);
    let follower = at(&mut world, Vec3::ZERO);
    let other = at(&mut world, Vec3::ZERO);
    world.set_follow_target(follower, Some(target)).unwrap();
    world.set_follow_target(target, Some(other)).unwrap();

    world.destroy_object(target).unwrap();

    assert_eq!(world.follow_target(follower).unwrap(), None);
    assert!(world.followers(other).unwrap().is_empty());
    move_to(&mut world, other, Vec3::X);
    assert_eq!(translation(&world, follower), Vec3::ZERO);
}

#[test]
fn destroying_a_follower_removes_it_from_its_targets_set() {
    let mut world = world();
    let target = at(&mut world, Vec3::ZERO);
    let follower = at(&mut world, Vec3::ZERO);
    world.set_follow_target(follower, Some(target)).unwrap();
    world.destroy_object(follower).unwrap();
    assert!(world.followers(target).unwrap().is_empty());
    move_to(&mut world, target, Vec3::X);
}

#[test]
fn rotation_and_scale_do_not_propagate() {
    let mut world = world();
    let target = at(&mut world, Vec3::ZERO);
    let follower = at(&mut world, Vec3::new(1.0, 0.0, 0.0));
    world.set_follow_target(follower, Some(target)).unwrap();

    world
        .set_local_transform(
            target,
            LocalTransform {
                rotation: Quat::from_rotation_z(1.0),
                scale: Vec3::splat(3.0),
                ..LocalTransform::default()
            },
        )
        .unwrap();

    assert_eq!(
        world.local_transform(follower).unwrap().rotation,
        Quat::IDENTITY
    );
    assert_eq!(world.local_transform(follower).unwrap().scale, Vec3::ONE);
    assert_eq!(translation(&world, follower), Vec3::new(1.0, 0.0, 0.0));
}

#[test]
fn every_transform_path_propagates() {
    let mut world = world();
    let target = at(&mut world, Vec3::ZERO);
    let follower = at(&mut world, Vec3::new(0.0, 10.0, 0.0));
    world.set_follow_target(follower, Some(target)).unwrap();

    world
        .set_world_position(target, Vec3::new(1.0, 0.0, 0.0))
        .unwrap();
    assert_eq!(translation(&world, follower), Vec3::new(1.0, 10.0, 0.0));

    world
        .apply_edits(&[ObjectEdit::Translation(target, Vec3::new(3.0, 0.0, 0.0))])
        .unwrap();
    assert_eq!(translation(&world, follower), Vec3::new(3.0, 10.0, 0.0));

    world
        .step_with_edits(
            &[ObjectEdit::Translation(target, Vec3::new(4.0, 0.0, 0.0))],
            0.0,
        )
        .unwrap();
    assert_eq!(translation(&world, follower), Vec3::new(4.0, 10.0, 0.0));
}

#[test]
fn a_follower_with_a_parent_moves_by_the_world_displacement() {
    let mut world = world();
    let target = at(&mut world, Vec3::ZERO);
    let parent = world
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::new(100.0, 0.0, 0.0),
                scale: Vec3::splat(2.0),
                ..LocalTransform::default()
            },
            ..ObjectDesc::default()
        })
        .unwrap();
    let follower = at(&mut world, Vec3::new(1.0, 0.0, 0.0));
    world
        .set_parent(follower, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    world.set_follow_target(follower, Some(target)).unwrap();
    let before = world.world_position(follower).unwrap();

    move_to(&mut world, target, Vec3::new(4.0, 0.0, 6.0));

    assert_eq!(
        world.world_position(follower).unwrap(),
        before + Vec3::new(4.0, 0.0, 6.0)
    );
    assert_eq!(translation(&world, follower), Vec3::new(3.0, 0.0, 3.0));
}

#[test]
fn a_parented_target_displaces_followers_by_its_world_movement() {
    let mut world = world();
    let parent = world
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                scale: Vec3::splat(2.0),
                ..LocalTransform::default()
            },
            ..ObjectDesc::default()
        })
        .unwrap();
    let target = at(&mut world, Vec3::ZERO);
    world
        .set_parent(target, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    let follower = at(&mut world, Vec3::new(0.0, 5.0, 0.0));
    world.set_follow_target(follower, Some(target)).unwrap();

    move_to(&mut world, target, Vec3::new(1.0, 0.0, 0.0));

    assert_eq!(
        world.world_position(target).unwrap(),
        Vec3::new(2.0, 0.0, 0.0)
    );
    assert_eq!(translation(&world, follower), Vec3::new(2.0, 5.0, 0.0));
}
