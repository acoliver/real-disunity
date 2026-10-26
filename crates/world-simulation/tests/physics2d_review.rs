use glam::{Vec2, Vec3};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, ObjectHandle, ParentSpace, World, WorldError,
    WorldSettings,
    physics2d::{
        Body2d, BodyType, Collider2d, ColliderGeometry, ContactPhase, PhysicsError,
        PhysicsSettings, QueryFilter, Shape2d,
    },
};

fn world() -> World {
    let mut w = World::new(WorldSettings::default()).unwrap();
    w.set_physics_settings(PhysicsSettings {
        gravity: Vec2::ZERO,
        ..Default::default()
    })
    .unwrap();
    w
}
fn object(w: &mut World, position: Vec2) -> ObjectHandle {
    w.create_object(ObjectDesc {
        local_transform: LocalTransform {
            translation: position.extend(0.0),
            ..Default::default()
        },
        ..Default::default()
    })
    .unwrap()
}
fn collider(w: &mut World, o: ObjectHandle, shape: Shape2d, offset: Vec2, trigger: bool) {
    w.add_component(
        o,
        ComponentDesc::Collider2d(Collider2d {
            geometry: ColliderGeometry::Shapes(vec![shape]),
            offset,
            trigger,
            ..Default::default()
        }),
    )
    .unwrap();
}
fn rotating_pair(trigger: bool, endpoint: bool, thin_box: bool) -> (World, ObjectHandle) {
    let mut w = world();
    let rotor = object(&mut w, Vec2::ZERO);
    w.add_component(
        rotor,
        ComponentDesc::Body2d(Body2d {
            angular_velocity: 1.0,
            ..Default::default()
        }),
    )
    .unwrap();
    collider(
        &mut w,
        rotor,
        Shape2d::Circle { radius: 0.001 },
        Vec2::new(100.0, 0.0),
        trigger,
    );
    let angle = if endpoint { 0.5_f32 } else { 0.25_f32 }.to_radians();
    let p = Vec2::new(angle.cos(), angle.sin()) * 100.0;
    let obstacle = object(&mut w, p);
    let shape = if thin_box {
        Shape2d::Polygon(vec![
            Vec2::new(-0.01, -0.0001),
            Vec2::new(0.01, -0.0001),
            Vec2::new(0.01, 0.0001),
            Vec2::new(-0.01, 0.0001),
        ])
    } else {
        Shape2d::Circle { radius: 0.001 }
    };
    collider(&mut w, obstacle, shape, Vec2::ZERO, false);
    (w, rotor)
}
#[test]
fn angular_sweep_catches_offset_solid_between_samples_and_at_endpoint() {
    for endpoint in [false, true] {
        for thin_box in [false, true] {
            let (mut w, rotor) = rotating_pair(false, endpoint, thin_box);
            w.step_physics(0.5_f32.to_radians()).unwrap();
            assert!(w.contacts().iter().any(|e| e.phase == ContactPhase::Enter));
            let ComponentDesc::Body2d(body) = &w
                .snapshot()
                .components
                .into_iter()
                .find(|c| c.object == rotor && matches!(c.desc, ComponentDesc::Body2d(_)))
                .unwrap()
                .desc
            else {
                unreachable!()
            };
            assert!(
                (body.angular_velocity - 0.5).abs() < 0.01,
                "angular impulse must match the offset circle inertia: {}",
                body.angular_velocity
            );
        }
    }
}
#[test]
fn angular_trigger_crossing_emits_enter_and_exit() {
    let (mut w, _) = rotating_pair(true, false, false);
    w.step_physics(0.5_f32.to_radians()).unwrap();
    assert_eq!(
        w.contacts().iter().map(|e| e.phase).collect::<Vec<_>>(),
        [ContactPhase::Enter, ContactPhase::Exit]
    );
}
#[test]
fn long_rotating_box_cannot_cross_thin_obstacle() {
    let mut w = world();
    let rotor = object(&mut w, Vec2::ZERO);
    w.add_component(
        rotor,
        ComponentDesc::Body2d(Body2d {
            angular_velocity: 1.0,
            ..Default::default()
        }),
    )
    .unwrap();
    collider(
        &mut w,
        rotor,
        Shape2d::Box {
            half_extents: Vec2::new(100.0, 0.001),
        },
        Vec2::ZERO,
        false,
    );
    let obstacle = object(&mut w, Vec2::new(99.0, 99.0 * 0.25_f32.to_radians().sin()));
    collider(
        &mut w,
        obstacle,
        Shape2d::Box {
            half_extents: Vec2::splat(0.001),
        },
        Vec2::ZERO,
        false,
    );
    w.step_physics(0.5_f32.to_radians()).unwrap();
    assert!(!w.contacts().is_empty());
}
#[test]
fn static_body_below_moving_parent_is_rejected_atomically() {
    for angular in [false, true] {
        let mut w = world();
        let parent = object(&mut w, Vec2::ZERO);
        let child = object(&mut w, Vec2::X);
        w.set_parent(child, Some(parent), ParentSpace::KeepLocal)
            .unwrap();
        w.add_component(
            parent,
            ComponentDesc::Body2d(Body2d {
                body_type: BodyType::Kinematic,
                velocity: if angular {
                    Vec2::ZERO
                } else {
                    Vec2::new(10.0, 0.0)
                },
                angular_velocity: if angular { 1.0 } else { 0.0 },
                ..Default::default()
            }),
        )
        .unwrap();
        w.add_component(
            child,
            ComponentDesc::Body2d(Body2d {
                body_type: BodyType::Static,
                ..Default::default()
            }),
        )
        .unwrap();
        collider(
            &mut w,
            child,
            Shape2d::Box {
                half_extents: Vec2::splat(0.5),
            },
            Vec2::ZERO,
            false,
        );
        let before = w.snapshot();
        let query = w.overlap_point(Vec2::X, QueryFilter::default()).unwrap();
        assert_eq!(
            w.step_physics(1.0),
            Err(WorldError::Physics(PhysicsError::UnsupportedHierarchy))
        );
        assert_eq!(w.snapshot(), before);
        assert!(w.contacts().is_empty());
        assert_eq!(
            w.overlap_point(Vec2::X, QueryFilter::default()).unwrap(),
            query
        );
    }
}
#[test]
fn finite_high_speed_circle_quadratic_does_not_tunnel() {
    for vertex in [false, true] {
        let mut w = world();
        let moving = object(&mut w, Vec2::new(-128.0, 0.0));
        w.add_component(
            moving,
            ComponentDesc::Body2d(Body2d {
                velocity: Vec2::new(2.0_f32.powi(60), 0.0),
                freeze_rotation: true,
                ..Default::default()
            }),
        )
        .unwrap();
        collider(
            &mut w,
            moving,
            Shape2d::Circle { radius: 0.5 },
            Vec2::ZERO,
            false,
        );
        let obstacle = object(&mut w, Vec2::ZERO);
        let shape = if vertex {
            Shape2d::Polygon(vec![Vec2::ZERO, Vec2::new(1.0, -1.0), Vec2::new(1.0, 1.0)])
        } else {
            Shape2d::Circle { radius: 0.5 }
        };
        collider(&mut w, obstacle, shape, Vec2::ZERO, false);
        w.step_physics(2.0_f32.powi(-52)).unwrap();
        assert!(w.local_transform(moving).unwrap().translation.x < 0.0);
        assert!(!w.contacts().is_empty());
    }
}
#[test]
fn grazing_circle_ray_hit_and_miss_are_distinguished() {
    let mut w = world();
    let o = object(&mut w, Vec2::ZERO);
    collider(
        &mut w,
        o,
        Shape2d::Circle { radius: 1.0 },
        Vec2::ZERO,
        false,
    );
    for (y, hit) in [
        (f32::from_bits(1.0_f32.to_bits() - 1), true),
        (f32::from_bits(1.0_f32.to_bits() + 1), false),
    ] {
        let hits = w
            .raycast(Vec2::new(-128.0, y), Vec2::X, 256.0, QueryFilter::default())
            .unwrap();
        assert_eq!(!hits.is_empty(), hit);
        assert!(
            hits.iter()
                .all(|h| h.point.is_finite() && h.normal.is_finite() && h.distance.is_finite())
        );
    }
}
#[test]
fn tiny_positive_and_zero_shapes_are_rejected_without_mutation() {
    for shape in [
        Shape2d::Box {
            half_extents: Vec2::splat(1e-30),
        },
        Shape2d::Box {
            half_extents: Vec2::ZERO,
        },
        Shape2d::Circle { radius: 0.0 },
    ] {
        let mut w = world();
        let o = object(&mut w, Vec2::ZERO);
        let before = w.snapshot();
        assert!(
            w.add_component(
                o,
                ComponentDesc::Collider2d(Collider2d {
                    geometry: ColliderGeometry::Shapes(vec![shape]),
                    ..Default::default()
                })
            )
            .is_err()
        );
        assert_eq!(w.snapshot(), before);
    }
}
#[test]
fn world_vertex_collapse_is_rejected_before_queries_and_motion() {
    let mut w = world();
    let o = object(&mut w, Vec2::splat(1e10));
    collider(
        &mut w,
        o,
        Shape2d::Box {
            half_extents: Vec2::splat(0.5),
        },
        Vec2::ZERO,
        false,
    );
    let before = w.snapshot();
    assert!(matches!(
        w.step_physics(0.1),
        Err(WorldError::Physics(PhysicsError::InvalidShape))
    ));
    assert_eq!(w.snapshot(), before);
    assert!(w.overlap_point(Vec2::ZERO, QueryFilter::default()).is_err());
    assert!(
        w.overlap_shape(
            &Shape2d::Box {
                half_extents: Vec2::splat(0.5)
            },
            Vec2::splat(1e10),
            QueryFilter::default()
        )
        .is_err()
    );
    assert_eq!(
        w.local_transform(o).unwrap().translation,
        Vec3::new(1e10, 1e10, 0.0)
    );
}

#[test]
fn angular_contact_first_reached_at_step_endpoint_is_resolved() {
    let (mut w, rotor) = rotating_pair(false, true, false);
    let obstacle = w
        .snapshot()
        .objects
        .into_iter()
        .find(|o| o.handle != rotor)
        .unwrap()
        .handle;
    let angle = 0.5_f32.to_radians();
    let endpoint = Vec2::new(angle.cos(), angle.sin()) * 100.0;
    let tangent = Vec2::new(-angle.sin(), angle.cos());
    w.set_local_transform(
        obstacle,
        LocalTransform {
            translation: (endpoint + tangent * 0.002).extend(0.0),
            ..Default::default()
        },
    )
    .unwrap();
    w.step_physics(angle).unwrap();
    assert!(w.contacts().iter().any(|e| e.phase == ContactPhase::Enter));
    let component = w
        .snapshot()
        .components
        .into_iter()
        .find(|c| c.object == rotor && matches!(c.desc, ComponentDesc::Body2d(_)))
        .unwrap();
    let ComponentDesc::Body2d(body) = component.desc else {
        unreachable!()
    };
    assert!((body.angular_velocity - 0.5).abs() < 0.01);
}

#[test]
fn angular_advancement_exhaustion_preserves_completed_state() {
    let (mut w, _) = rotating_pair(false, false, false);
    w.set_physics_settings(PhysicsSettings {
        gravity: Vec2::ZERO,
        max_iterations: 1,
        ..Default::default()
    })
    .unwrap();
    let before = w.snapshot();
    assert_eq!(
        w.step_physics(0.5_f32.to_radians()),
        Err(WorldError::Physics(PhysicsError::IterationLimit))
    );
    assert_eq!(w.snapshot(), before);
    assert!(w.contacts().is_empty());
}
