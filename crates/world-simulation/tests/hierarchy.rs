#[test]
fn wrong_world_and_stale_parent_leave_hierarchy_unchanged() {
    let (mut world, parent, child) = setup();
    let (other, foreign, _) = setup();
    let before = world.snapshot();
    assert_eq!(
        world.set_parent(child, Some(foreign), ParentSpace::KeepLocal),
        Err(WorldError::WrongWorld)
    );
    assert_eq!(world.snapshot(), before);
    drop(other);
    world.destroy_object(parent).unwrap();
    assert_eq!(
        world.set_parent(child, Some(parent), ParentSpace::KeepLocal),
        Err(WorldError::StaleObject(parent))
    );
    assert_eq!(world.parent(child).unwrap(), None);
}

#[test]
fn keep_world_preserves_representable_zero_scale_and_reflections() {
    let (mut world, parent, child) = setup();
    world
        .set_local_transform(
            parent,
            LocalTransform {
                translation: Vec3::ONE,
                ..Default::default()
            },
        )
        .unwrap();
    for scale in [
        Vec3::ZERO,
        Vec3::new(0.0, 2.0, 3.0),
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::new(-2.0, 3.0, 4.0),
    ] {
        world
            .set_parent(child, None, ParentSpace::KeepLocal)
            .unwrap();
        world
            .set_local_transform(
                child,
                LocalTransform {
                    scale,
                    rotation: Quat::from_rotation_y(0.7),
                    ..Default::default()
                },
            )
            .unwrap();
        let before = world.world_matrix(child).unwrap();
        world
            .set_parent(child, Some(parent), ParentSpace::KeepWorld)
            .unwrap();
        assert!(world.world_matrix(child).unwrap().abs_diff_eq(before, 1e-4));
    }
}

use glam::{Mat4, Quat, Vec3};
use worldrunner_world_simulation::{
    LocalTransform, ObjectDesc, ParentSpace, World, WorldError, WorldSettings,
};

fn setup() -> (
    World,
    worldrunner_world_simulation::ObjectHandle,
    worldrunner_world_simulation::ObjectHandle,
) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let parent = world.create_object(ObjectDesc::default()).unwrap();
    let child = world.create_object(ObjectDesc::default()).unwrap();
    (world, parent, child)
}

#[test]
fn parenting_rejects_cycles_and_composes_full_affine_trs() {
    let (mut world, parent, child) = setup();
    let parent_trs = LocalTransform {
        translation: Vec3::X,
        rotation: Quat::from_rotation_z(0.7),
        scale: Vec3::new(2.0, 3.0, 4.0),
    };
    let child_trs = LocalTransform {
        translation: Vec3::Y,
        rotation: Quat::from_rotation_y(0.4),
        scale: Vec3::new(1.0, 2.0, 1.0),
    };
    world.set_local_transform(parent, parent_trs).unwrap();
    world.set_local_transform(child, child_trs).unwrap();
    world
        .set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    assert!(
        world
            .world_matrix(child)
            .unwrap()
            .abs_diff_eq(parent_trs.matrix() * child_trs.matrix(), 1e-5)
    );
    let before = world.snapshot();
    assert_eq!(
        world.set_parent(parent, Some(child), ParentSpace::KeepLocal),
        Err(WorldError::HierarchyCycle)
    );
    assert_eq!(world.snapshot(), before);
    assert_eq!(world.parent(child).unwrap(), Some(parent));
    assert_eq!(world.children(parent).unwrap(), vec![child]);
}

#[test]
fn keep_world_recomputes_local_and_failure_is_atomic() {
    let (mut world, parent, child) = setup();
    world
        .set_local_transform(
            parent,
            LocalTransform {
                translation: Vec3::splat(2.0),
                scale: Vec3::splat(2.0),
                ..Default::default()
            },
        )
        .unwrap();
    let before = world.world_matrix(child).unwrap();
    world
        .set_parent(child, Some(parent), ParentSpace::KeepWorld)
        .unwrap();
    assert!(world.world_matrix(child).unwrap().abs_diff_eq(before, 1e-5));
    world
        .set_parent(child, None, ParentSpace::KeepWorld)
        .unwrap();
    world
        .set_local_transform(
            parent,
            LocalTransform {
                scale: Vec3::ZERO,
                ..Default::default()
            },
        )
        .unwrap();
    let snapshot = world.snapshot();
    assert_eq!(
        world.set_parent(child, Some(parent), ParentSpace::KeepWorld),
        Err(WorldError::SingularParent)
    );
    assert_eq!(world.snapshot(), snapshot);
    world
        .set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    assert_eq!(world.world_matrix(child).unwrap().x_axis, Mat4::ZERO.x_axis);
}

#[test]
fn keep_world_rejects_shear_and_deep_hierarchies_are_iterative() {
    let (mut world, parent, child) = setup();
    world
        .set_local_transform(
            parent,
            LocalTransform {
                scale: Vec3::new(2.0, 1.0, 1.0),
                ..Default::default()
            },
        )
        .unwrap();
    world
        .set_local_transform(
            child,
            LocalTransform {
                rotation: Quat::from_rotation_z(0.7),
                ..Default::default()
            },
        )
        .unwrap();
    let before = world.snapshot();
    assert_eq!(
        world.set_parent(child, Some(parent), ParentSpace::KeepWorld),
        Err(WorldError::NonRepresentableLocalTransform)
    );
    assert_eq!(world.snapshot(), before);
    let mut parent = parent;
    for _ in 0..512 {
        let child = world.create_object(ObjectDesc::default()).unwrap();
        world
            .set_parent(child, Some(parent), ParentSpace::KeepLocal)
            .unwrap();
        parent = child;
    }
    assert!(world.world_matrix(parent).unwrap().is_finite());
}
