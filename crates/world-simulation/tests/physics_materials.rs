use glam::{Quat, Vec2};
use worldrunner_world_simulation::physics2d::{
    Body2d, Collider2d, ColliderGeometry, PhysicsError, PhysicsMaterial2d, PhysicsMaterialHandle,
    Shape2d,
};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, World, WorldError, WorldSettings,
};

fn material(w: &mut World, friction: f32, bounce: f32) -> PhysicsMaterialHandle {
    w.create_physics_material(PhysicsMaterial2d::new(friction, bounce).unwrap())
        .unwrap()
}

#[test]
fn invalid_bounce_threshold_is_rejected_without_mutation() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let original = w.physics_settings();
    for threshold in [-1.0, f32::NAN, f32::INFINITY] {
        let mut settings = original;
        settings.bounce_threshold = threshold;
        assert_eq!(
            w.set_physics_settings(settings),
            Err(PhysicsError::InvalidSettings.into())
        );
        assert_eq!(w.physics_settings(), original);
    }
}

fn scene(
    friction: f32,
    bounce: f32,
    angle: f32,
) -> (
    World,
    worldrunner_world_simulation::ObjectHandle,
    worldrunner_world_simulation::ComponentHandle,
) {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let floor_material = material(&mut w, friction, 0.0);
    let ball_material = material(&mut w, friction, bounce);
    let floor = w
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                rotation: Quat::from_rotation_z(angle),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    w.add_component(
        floor,
        ComponentDesc::Collider2d(Collider2d {
            geometry: ColliderGeometry::Shapes(vec![Shape2d::Box {
                half_extents: Vec2::new(20.0, 0.5),
            }]),
            shared_material: Some(floor_material),
            ..Default::default()
        }),
    )
    .unwrap();
    let ball = w
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec2::new(0.0, 4.0).extend(0.0),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    let body = w
        .add_component(
            ball,
            ComponentDesc::Body2d(Body2d {
                freeze_rotation: true,
                shared_material: Some(ball_material),
                ..Default::default()
            }),
        )
        .unwrap();
    w.add_component(
        ball,
        ComponentDesc::Collider2d(Collider2d {
            geometry: ColliderGeometry::Shapes(vec![Shape2d::Circle { radius: 0.5 }]),
            ..Default::default()
        }),
    )
    .unwrap();
    (w, ball, body)
}

#[test]
fn combine_rules_and_invalid_values() {
    let a = PhysicsMaterial2d::new(0.25, 0.2).unwrap();
    let b = PhysicsMaterial2d::new(1.0, 0.8).unwrap();
    assert_eq!(a.combine(b), PhysicsMaterial2d::new(0.5, 0.8).unwrap());
    assert_eq!(a.combine(b), b.combine(a));
    for bad in [-1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            PhysicsMaterial2d::new(bad, 0.0),
            Err(PhysicsError::InvalidMaterial)
        );
        assert_eq!(
            PhysicsMaterial2d::new(0.0, bad),
            Err(PhysicsError::InvalidMaterial)
        );
    }
    assert_eq!(
        PhysicsMaterial2d::new(0.0, 1.1),
        Err(PhysicsError::InvalidMaterial)
    );
}

#[test]
fn perfectly_elastic_ball_rebounds_to_drop_height() {
    let (mut w, ball, _) = scene(0.0, 1.0, 0.0);
    let mut hit = false;
    let mut apex = 0.0_f32;
    for _ in 0..800 {
        w.step_physics(0.002).unwrap();
        hit |= !w.contacts().is_empty();
        if hit {
            apex = apex.max(w.local_transform(ball).unwrap().translation.y);
        }
    }
    assert!((apex - 4.0).abs() < 0.04, "apex {apex}");
}

#[test]
fn zero_bounce_stays_on_floor() {
    let (mut w, ball, body) = scene(0.0, 0.0, 0.0);
    for _ in 0..800 {
        w.step_physics(0.002).unwrap();
    }
    assert!((w.local_transform(ball).unwrap().translation.y - 1.0).abs() < 0.001);
    let ComponentDesc::Body2d(b) = w.component(body).unwrap().desc.clone() else {
        panic!()
    };
    assert!(b.velocity.y.abs() < 0.001);
}
#[test]
fn material_bounce_has_a_low_speed_cutoff() {
    for (threshold, speed, expected) in [
        (1.0, 0.01, 0.0),
        (1.0, 1.0, 0.0),
        (1.0, 2.0, 1.0),
        (0.0, 0.01, 0.005),
        (3.0, 2.0, 0.0),
    ] {
        let (mut w, ball, body) = scene(0.0, 0.5, 0.0);
        let mut settings = w.physics_settings();
        settings.bounce_threshold = threshold;
        settings.gravity = Vec2::ZERO;
        w.set_physics_settings(settings).unwrap();
        w.set_local_transform(
            ball,
            LocalTransform {
                translation: Vec2::new(0.0, 1.0).extend(0.0),
                ..Default::default()
            },
        )
        .unwrap();
        let ComponentDesc::Body2d(mut b) = w.component(body).unwrap().desc.clone() else {
            panic!()
        };
        b.velocity.y = -speed;
        w.set_component(body, ComponentDesc::Body2d(b)).unwrap();
        w.step_physics(0.002).unwrap();
        let ComponentDesc::Body2d(b) = &w.component(body).unwrap().desc else {
            panic!()
        };
        assert!(
            (b.velocity.y - expected).abs() < 1e-6,
            "speed {speed}: {:?}",
            b.velocity
        );
    }
}

#[test]
fn nonzero_bounce_settles_without_supporting_rebounds() {
    let (mut w, ball, body) = scene(0.0, 0.5, 0.0);
    for _ in 0..2000 {
        w.step_physics(0.002).unwrap();
    }
    assert!((w.local_transform(ball).unwrap().translation.y - 1.0).abs() < 0.001);
    let ComponentDesc::Body2d(b) = &w.component(body).unwrap().desc else {
        panic!()
    };
    assert!(
        b.velocity.y.abs() < 1e-6,
        "resting velocity {:?}",
        b.velocity
    );
}

#[test]
fn slope_threshold_stops_or_slides() {
    let angle = 0.3_f32;
    for (mu, stops) in [(0.4, true), (0.2, false)] {
        assert_eq!(mu >= angle.tan(), stops);
        let (mut w, ball, body) = scene(mu, 0.0, angle);
        let normal = Vec2::new(-angle.sin(), angle.cos());
        w.set_local_transform(
            ball,
            LocalTransform {
                translation: normal.extend(0.0),
                ..Default::default()
            },
        )
        .unwrap();
        let mut desc = w.component(body).unwrap().desc.clone();
        let ComponentDesc::Body2d(ref mut b) = desc else {
            panic!()
        };
        b.velocity = Vec2::new(-angle.cos(), -angle.sin()) * 0.1;
        w.set_component(body, desc).unwrap();
        for _ in 0..500 {
            w.step_physics(0.002).unwrap();
        }
        let ComponentDesc::Body2d(b) = w.component(body).unwrap().desc.clone() else {
            panic!()
        };
        if stops {
            assert!(b.velocity.length() < 0.002, "velocity {:?}", b.velocity);
        } else {
            let expected = 0.1 + 9.81 * (angle.sin() - mu * angle.cos());
            assert!(
                (b.velocity.length() - expected).abs() < 0.02,
                "velocity {:?}, expected {expected}",
                b.velocity
            );
        }
    }
}

#[test]
fn collider_overrides_body_and_shared_asset_edits_are_live() {
    let (mut w, ball, body) = scene(0.0, 1.0, 0.0);
    let collider = w
        .snapshot()
        .components
        .into_iter()
        .find(|c| c.object == ball && matches!(c.desc, ComponentDesc::Collider2d(_)))
        .unwrap()
        .handle;
    assert_eq!(
        w.collider_material(collider)
            .unwrap()
            .bounciness()
            .to_bits(),
        1.0_f32.to_bits()
    );
    let override_material = material(&mut w, 0.3, 0.0);
    let ComponentDesc::Collider2d(mut c) = w.component(collider).unwrap().desc.clone() else {
        panic!()
    };
    c.shared_material = Some(override_material);
    w.set_component(collider, ComponentDesc::Collider2d(c.clone()))
        .unwrap();
    assert_eq!(
        w.collider_material(collider)
            .unwrap()
            .bounciness()
            .to_bits(),
        0.0_f32.to_bits()
    );
    w.set_physics_material(override_material, PhysicsMaterial2d::new(0.6, 0.5).unwrap())
        .unwrap();
    assert_eq!(
        w.collider_material(collider).unwrap().friction().to_bits(),
        0.6_f32.to_bits()
    );
    c.shared_material = None;
    w.set_component(collider, ComponentDesc::Collider2d(c))
        .unwrap();
    assert_eq!(
        w.collider_material(collider)
            .unwrap()
            .bounciness()
            .to_bits(),
        1.0_f32.to_bits()
    );
    let ComponentDesc::Body2d(mut b) = w.component(body).unwrap().desc.clone() else {
        panic!()
    };
    b.shared_material = None;
    w.set_component(body, ComponentDesc::Body2d(b)).unwrap();
    assert_eq!(
        w.collider_material(collider).unwrap(),
        PhysicsMaterial2d::new(0.0, 0.0).unwrap()
    );
    let mut other = World::new(WorldSettings::default()).unwrap();
    assert_eq!(
        other.set_physics_material(override_material, PhysicsMaterial2d::new(0.0, 0.0).unwrap()),
        Err(WorldError::WrongWorld)
    );
}

#[test]
fn runs_are_bitwise_deterministic() {
    fn run() -> Vec<[u32; 4]> {
        let (mut w, ball, body) = scene(0.2, 0.7, 0.1);
        (0..500)
            .map(|_| {
                w.step_physics(0.003).unwrap();
                let p = w.local_transform(ball).unwrap().translation;
                let ComponentDesc::Body2d(b) = w.component(body).unwrap().desc.clone() else {
                    panic!()
                };
                [
                    p.x.to_bits(),
                    p.y.to_bits(),
                    b.velocity.x.to_bits(),
                    b.velocity.y.to_bits(),
                ]
            })
            .collect()
    }
    assert_eq!(run(), run());
}

#[test]
fn missing_assets_preserve_legacy_frictionless_motion_and_body_restitution() {
    let (mut w, ball, body) = scene(0.0, 0.0, 0.0);
    for c in w.snapshot().components {
        let mut desc = c.desc;
        match &mut desc {
            ComponentDesc::Collider2d(c) => c.shared_material = None,
            ComponentDesc::Body2d(b) => {
                b.shared_material = None;
                b.restitution = 1.0;
                b.velocity.x = 2.0;
            }
            _ => continue,
        }
        w.set_component(c.handle, desc).unwrap();
    }
    for _ in 0..500 {
        w.step_physics(0.002).unwrap();
    }
    let ComponentDesc::Body2d(b) = &w.component(body).unwrap().desc else {
        panic!()
    };
    assert_eq!(b.velocity.x.to_bits(), 2.0_f32.to_bits());
    assert!(w.local_transform(ball).unwrap().translation.y > 2.0);
}

#[test]
fn child_collider_inherits_nearest_body_material_and_rejects_foreign_assets() {
    use worldrunner_world_simulation::ParentSpace;
    let (mut w, parent, _) = scene(0.3, 0.8, 0.0);
    let child = w.create_object(ObjectDesc::default()).unwrap();
    w.set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    let c = w
        .add_component(child, ComponentDesc::Collider2d(Collider2d::default()))
        .unwrap();
    assert_eq!(
        w.collider_material(c).unwrap(),
        PhysicsMaterial2d::new(0.3, 0.8).unwrap()
    );
    let mut other = World::new(WorldSettings::default()).unwrap();
    let foreign = material(&mut other, 0.5, 0.0);
    assert_eq!(
        w.set_component(
            c,
            ComponentDesc::Collider2d(Collider2d {
                shared_material: Some(foreign),
                ..Default::default()
            })
        ),
        Err(WorldError::WrongWorld)
    );
    w.add_component(child, ComponentDesc::Body2d(Body2d::default()))
        .unwrap();
    assert_eq!(
        w.collider_material(c).unwrap(),
        PhysicsMaterial2d::default()
    );
}
