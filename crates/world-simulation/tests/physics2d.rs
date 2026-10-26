use glam::{Quat, Vec2, Vec3};
use worldrunner_world_simulation::physics2d::{
    Body2d, BodyType, Collider2d, ContactPhase, PhysicsError, PhysicsPlane, PhysicsSettings,
    QueryFilter, Shape2d, TileCell, TileGrid, TileShape,
};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, DestroyWhen, LocalTransform, ObjectDesc, ObjectHandle,
    ParentSpace, World, WorldError, WorldSettings,
};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}
fn object(w: &mut World, p: Vec2) -> ObjectHandle {
    w.create_object(ObjectDesc {
        local_transform: LocalTransform {
            translation: p.extend(0.0),
            ..Default::default()
        },
        ..Default::default()
    })
    .unwrap()
}
fn body(w: &mut World, o: ObjectHandle, kind: BodyType, v: Vec2) -> ComponentHandle {
    w.add_component(
        o,
        ComponentDesc::Body2d(Body2d {
            body_type: kind,
            velocity: v,
            gravity_scale: 0.0,
            ..Default::default()
        }),
    )
    .unwrap()
}
fn collider(w: &mut World, o: ObjectHandle, trigger: bool) -> ComponentHandle {
    w.add_component(
        o,
        ComponentDesc::Collider2d(Collider2d {
            trigger,
            ..Default::default()
        }),
    )
    .unwrap()
}
fn pair(trigger: bool) -> (World, ObjectHandle, ComponentHandle, ComponentHandle) {
    let mut w = world();
    let a = object(&mut w, Vec2::ZERO);
    let b = object(&mut w, Vec2::new(0.8, 0.0));
    body(&mut w, a, BodyType::Dynamic, Vec2::ZERO);
    let ca = collider(&mut w, a, trigger);
    let cb = collider(&mut w, b, false);
    (w, a, ca, cb)
}

#[test]
fn enter_stay_exit_and_typed_identity() {
    let (mut w, a, ca, cb) = pair(true);
    w.step_physics(0.01).unwrap();
    assert_eq!(w.contacts().len(), 1);
    assert_eq!(w.contacts()[0].phase, ContactPhase::Enter);
    assert_eq!(
        (w.contacts()[0].collider_a, w.contacts()[0].collider_b),
        (ca, cb)
    );
    w.step_physics(0.01).unwrap();
    assert_eq!(w.contacts()[0].phase, ContactPhase::Stay);
    w.set_local_transform(
        a,
        LocalTransform {
            translation: Vec3::new(4.0, 0.0, 0.0),
            ..Default::default()
        },
    )
    .unwrap();
    w.step_physics(0.01).unwrap();
    assert_eq!(w.contacts()[0].phase, ContactPhase::Exit);
    w.step_physics(0.01).unwrap();
    assert!(w.contacts().is_empty());
}

#[test]
fn triggers_do_not_block_and_fast_crossings_exit_in_same_step() {
    let mut w = world();
    let a = object(&mut w, Vec2::new(-5.0, 0.0));
    let b = object(&mut w, Vec2::ZERO);
    body(&mut w, a, BodyType::Kinematic, Vec2::new(100.0, 0.0));
    collider(&mut w, a, true);
    collider(&mut w, b, false);
    w.step_physics(0.1).unwrap();
    assert!((w.local_transform(a).unwrap().translation.x - 5.0).abs() < 1e-4);
    assert_eq!(
        w.contacts().iter().map(|c| c.phase).collect::<Vec<_>>(),
        [ContactPhase::Enter, ContactPhase::Exit]
    );
    assert!(w.contacts().iter().all(|c| c.trigger));
}

#[test]
fn compound_body_uses_nearest_ancestor_and_has_no_self_contact() {
    let mut w = world();
    let root = object(&mut w, Vec2::ZERO);
    let child = object(&mut w, Vec2::new(0.2, 0.0));
    let obstacle = object(&mut w, Vec2::new(0.8, 0.0));
    let rb = body(&mut w, root, BodyType::Dynamic, Vec2::ZERO);
    w.set_parent(child, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    collider(&mut w, root, true);
    collider(&mut w, child, true);
    collider(&mut w, obstacle, false);
    w.step_physics(0.01).unwrap();
    assert_eq!(w.contacts().len(), 2);
    assert!(w.contacts().iter().all(|c| c.body_a == Some(rb)));
}

#[test]
fn swept_convex_and_circle_bodies_cannot_tunnel() {
    for shape in [
        Shape2d::Box {
            half_extents: Vec2::splat(0.5),
        },
        Shape2d::Circle { radius: 0.5 },
        Shape2d::Polygon(vec![
            Vec2::new(-0.5, -0.5),
            Vec2::new(0.5, -0.5),
            Vec2::new(0.0, 0.5),
        ]),
    ] {
        let mut w = world();
        let a = object(&mut w, Vec2::new(-5.0, 0.0));
        let b = object(&mut w, Vec2::ZERO);
        body(&mut w, a, BodyType::Dynamic, Vec2::new(1000.0, 0.0));
        w.add_component(
            a,
            ComponentDesc::Collider2d(Collider2d {
                shapes: vec![shape],
                ..Default::default()
            }),
        )
        .unwrap();
        collider(&mut w, b, false);
        w.step_physics(0.01).unwrap();
        assert!(w.local_transform(a).unwrap().translation.x <= -0.99);
        assert!(w.contacts().iter().any(|c| c.phase == ContactPhase::Enter));
    }
}

#[test]
fn tile_shapes_preserve_empty_and_explicit_geometry() {
    let grid = TileGrid {
        cells: vec![
            TileCell {
                position: [0, 0],
                shape: TileShape::Grid,
            },
            TileCell {
                position: [1, 0],
                shape: TileShape::Empty,
            },
            TileCell {
                position: [2, 0],
                shape: TileShape::Convex(vec![Vec2::ZERO, Vec2::X, Vec2::Y]),
            },
        ],
        ..Default::default()
    };
    let mut w = world();
    let o = object(&mut w, Vec2::ZERO);
    let c = w
        .add_component(
            o,
            ComponentDesc::Collider2d(Collider2d {
                shapes: grid.shapes().unwrap(),
                ..Default::default()
            }),
        )
        .unwrap();
    assert_eq!(
        w.overlap_point(Vec2::splat(0.5), QueryFilter::default())
            .unwrap(),
        [c]
    );
    assert!(
        w.overlap_point(Vec2::new(1.5, 0.5), QueryFilter::default())
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        w.overlap_point(Vec2::new(2.1, 0.1), QueryFilter::default())
            .unwrap(),
        [c]
    );
    assert!(
        w.overlap_point(Vec2::new(2.9, 0.9), QueryFilter::default())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn queries_filter_layers_and_triggers_and_sort_distance() {
    let mut w = world();
    for (x, layer, trigger) in [(0.0, 1, true), (3.0, 2, false)] {
        let o = w
            .create_object(ObjectDesc {
                layer,
                local_transform: LocalTransform {
                    translation: Vec3::new(x, 0.0, 0.0),
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        collider(&mut w, o, trigger);
    }
    let all = w
        .raycast(Vec2::new(-2.0, 0.0), Vec2::X, 10.0, QueryFilter::default())
        .unwrap();
    assert_eq!(all.len(), 2);
    assert!(all[0].distance < all[1].distance);
    let filter = QueryFilter {
        layers: 1 << 2,
        include_triggers: false,
    };
    assert_eq!(
        w.raycast(Vec2::new(-2.0, 0.0), Vec2::X, 10.0, filter)
            .unwrap()
            .len(),
        1
    );
    assert!(w.overlap_point(Vec2::ZERO, filter).unwrap().is_empty());
    assert!(w.raycast(Vec2::ZERO, Vec2::ZERO, 1.0, filter).is_err());
}

#[test]
fn frozen_and_zero_angular_motion_preserve_exact_quaternion() {
    for frozen in [false, true] {
        let mut w = world();
        let o = object(&mut w, Vec2::ZERO);
        let rotation = Quat::from_rotation_z(0.713);
        w.set_local_transform(
            o,
            LocalTransform {
                rotation,
                ..Default::default()
            },
        )
        .unwrap();
        w.add_component(
            o,
            ComponentDesc::Body2d(Body2d {
                freeze_rotation: frozen,
                angular_velocity: if frozen { 3.0 } else { 0.0 },
                gravity_scale: 0.0,
                velocity: Vec2::X,
                ..Default::default()
            }),
        )
        .unwrap();
        for _ in 0..100 {
            w.step_physics(0.01).unwrap();
        }
        assert_eq!(w.local_transform(o).unwrap().rotation, rotation);
    }
}

#[test]
fn destruction_defers_invalidation_and_filters_pending_delivery() {
    let (mut w, a, ca, cb) = pair(true);
    w.step_physics(0.01).unwrap();
    w.destroy_component(ca).unwrap();
    assert!(w.component(ca).is_ok());
    assert!(w.next_contact().is_none());
    assert!(w.component(cb).is_ok());
    w.finish_phase();
    let replacement = collider(&mut w, a, true);
    assert_ne!(replacement, ca);
    w.step_physics(0.01).unwrap();
    assert!(
        w.contacts()
            .iter()
            .any(|c| c.collider_a == replacement && c.phase == ContactPhase::Enter)
    );
    w.destroy(a, DestroyWhen::EndOfFrame).unwrap();
    assert!(w.component(replacement).is_ok());
    assert!(w.next_contact().is_none());
    assert!(w.component(cb).is_ok());
    w.finish_phase();
    assert!(w.object(a).is_err());
    assert!(w.component(replacement).is_err());
    assert!(w.component(cb).is_ok());
}

#[test]
fn disabled_collider_suppresses_delivery_and_exits_next_step() {
    let (mut w, _, ca, _) = pair(true);
    w.step_physics(0.01).unwrap();
    let ComponentDesc::Collider2d(mut desc) = w.component(ca).unwrap().desc.clone() else {
        panic!()
    };
    desc.enabled = false;
    w.set_component(ca, ComponentDesc::Collider2d(desc))
        .unwrap();
    assert!(w.next_contact().is_none());
    w.step_physics(0.01).unwrap();
    assert_eq!(w.contacts()[0].phase, ContactPhase::Exit);
}

#[test]
fn invalid_input_and_iteration_exhaustion_fail_before_mutation() {
    let mut w = world();
    let o = object(&mut w, Vec2::ZERO);
    assert!(
        w.add_component(
            o,
            ComponentDesc::Body2d(Body2d {
                mass: 0.0,
                ..Default::default()
            })
        )
        .is_err()
    );
    assert!(
        w.add_component(
            o,
            ComponentDesc::Collider2d(Collider2d {
                shapes: vec![Shape2d::Circle { radius: f32::NAN }],
                ..Default::default()
            })
        )
        .is_err()
    );
    let before = w.snapshot();
    assert!(w.step_physics(f32::NAN).is_err());
    assert_eq!(before, w.snapshot());
    w.set_physics_settings(PhysicsSettings {
        max_iterations: 1,
        ..Default::default()
    })
    .unwrap();
    body(&mut w, o, BodyType::Dynamic, Vec2::X);
    w.add_component(o, ComponentDesc::Collider2d(Collider2d::default()))
        .unwrap();
    let b = object(&mut w, Vec2::new(0.8, 0.0));
    collider(&mut w, b, false);
    let before = w.snapshot();
    assert_eq!(
        w.step_physics(1.0),
        Err(WorldError::Physics(PhysicsError::IterationLimit))
    );
    assert_eq!(before, w.snapshot());
}

#[test]
fn static_kinematic_and_layer_policy() {
    let mut w = world();
    let a = object(&mut w, Vec2::new(-2.0, 0.0));
    let b = object(&mut w, Vec2::ZERO);
    body(&mut w, a, BodyType::Dynamic, Vec2::new(10.0, 0.0));
    body(&mut w, b, BodyType::Kinematic, Vec2::X);
    collider(&mut w, a, false);
    collider(&mut w, b, false);
    w.step_physics(0.2).unwrap();
    assert!(
        w.local_transform(a).unwrap().translation.x < w.local_transform(b).unwrap().translation.x
    );
    assert!((w.local_transform(b).unwrap().translation.x - 0.2).abs() < 1e-4);
    w.set_layer_collision(0, 0, false).unwrap();
    w.step_physics(0.01).unwrap();
    assert!(w.contacts().iter().all(|c| c.phase == ContactPhase::Exit));
}

#[test]
fn bounded_traces_are_deterministic_across_worlds() {
    let trace = || {
        let (mut w, _, _, _) = pair(false);
        let mut trace = Vec::new();
        for _ in 0..20 {
            w.step_physics(0.01).unwrap();
            trace.push((
                w.snapshot()
                    .objects
                    .iter()
                    .map(|o| o.local_transform)
                    .collect::<Vec<_>>(),
                w.contacts()
                    .iter()
                    .map(|c| (c.phase, c.normal, c.point))
                    .collect::<Vec<_>>(),
            ));
        }
        trace
    };
    assert_eq!(trace(), trace());
}

#[test]
fn explicit_plane_maps_native_z_up_coordinates() {
    let mut w = world();
    w.set_physics_settings(PhysicsSettings {
        plane: PhysicsPlane::Xz,
        gravity: Vec2::new(0.0, -10.0),
        ..Default::default()
    })
    .unwrap();
    let o = object(&mut w, Vec2::ZERO);
    w.add_component(o, ComponentDesc::Body2d(Body2d::default()))
        .unwrap();
    collider(&mut w, o, false);
    w.step_physics(0.1).unwrap();
    let p = w.local_transform(o).unwrap().translation;
    assert!(p.z < 0.0);
    assert_eq!(p.y.to_bits(), 0.0_f32.to_bits());
}
