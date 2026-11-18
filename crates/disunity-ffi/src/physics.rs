//! Typed projections onto the world's existing 2D physics components and queries.
use crate::{DisunityResult, callbacks::mutate, store::Session};
use glam::Vec2;
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle,
    physics2d::{
        Body2d, BodyType, Collider2d, ColliderGeometry, Interpolation, QueryFilter, Shape2d,
    },
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityBody2d {
    pub kind: u32,
    pub enabled: u32,
    pub mass: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub angular_velocity: f32,
    pub gravity_scale: f32,
    pub freeze_rotation: u32,
    pub freeze_position: u32,
    pub restitution: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityCollider2d {
    pub kind: u32,
    pub enabled: u32,
    pub trigger: u32,
    pub layers: u32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub size_x: f32,
    pub size_y: f32,
    pub radius: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityRayHit {
    pub collider: u64,
    pub point_x: f32,
    pub point_y: f32,
    pub normal_x: f32,
    pub normal_y: f32,
    pub distance: f32,
}

pub(crate) fn component(
    session: &Session,
    handle: u64,
) -> Result<ComponentHandle, crate::errors::AbiError> {
    session
        .dispatch
        .components
        .get(&handle)
        .copied()
        .ok_or_else(|| "component binding missing".into())
}
pub(crate) fn identity(
    session: &Session,
    native: ComponentHandle,
) -> Result<u64, crate::errors::AbiError> {
    session
        .dispatch
        .components
        .identity_of(native)
        .ok_or_else(|| "component has no managed identity".into())
}
pub(crate) fn create(
    world: u64,
    object: u64,
    desc: ComponentDesc,
) -> Result<u64, crate::errors::AbiError> {
    let mut store = crate::lock_store();
    let id = store
        .allocate_handle()
        .map_err(crate::errors::AbiError::from)?;
    let session = store.session_mut(world)?;
    let owner = session.object(object)?;
    let handle = session
        .native
        .add_component(owner, desc)
        .map_err(crate::errors::AbiError::from)?;
    session.dispatch.components.insert(id, handle);
    Ok(id)
}

/// # Safety
/// `output` is writable for one identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_physics_create(
    world: u64,
    object: u64,
    kind: u32,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let desc = match kind {
        0 => ComponentDesc::Body2d(Body2d::default()),
        1 => ComponentDesc::Collider2d(Collider2d::default()),
        2 => ComponentDesc::Collider2d(Collider2d {
            geometry: ColliderGeometry::Shapes(vec![Shape2d::Circle { radius: 0.5 }]),
            ..Default::default()
        }),
        3 => ComponentDesc::Collider2d(Collider2d {
            geometry: ColliderGeometry::Tilemap,
            ..Default::default()
        }),
        _ => return DisunityResult::invalid_argument(),
    };
    match create(world, object, desc) {
        Ok(id) => {
            unsafe { *output = id };
            DisunityResult::ok()
        }
        Err(e) => crate::abi::translate_error(Some(world), &e),
    }
}

fn body(value: DisunityBody2d) -> Result<Body2d, crate::errors::AbiError> {
    if value.enabled > 1 || value.freeze_rotation > 1 || value.freeze_position > 3 {
        return Err(worldrunner_world_simulation::WorldError::Physics(
            worldrunner_world_simulation::physics2d::PhysicsError::InvalidBody,
        )
        .into());
    }
    let body_type = match value.kind {
        0 => BodyType::Dynamic,
        1 => BodyType::Kinematic,
        2 => BodyType::Static,
        _ => {
            return Err(worldrunner_world_simulation::WorldError::Physics(
                worldrunner_world_simulation::physics2d::PhysicsError::InvalidBody,
            )
            .into());
        }
    };
    Ok(Body2d {
        body_type,
        enabled: value.enabled == 1,
        mass: value.mass,
        velocity: Vec2::new(value.velocity_x, value.velocity_y),
        angular_velocity: value.angular_velocity,
        gravity_scale: value.gravity_scale,
        freeze_rotation: value.freeze_rotation == 1,
        freeze_position: u8::try_from(value.freeze_position).expect("validated mask"),
        restitution: value.restitution,
        sleeping: false,
        shared_material: None,
        interpolation: Interpolation::None,
    })
}

/// # Safety
/// `value` points to one readable body record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_body_write(
    world: u64,
    handle: u64,
    value: *const DisunityBody2d,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let value = unsafe { *value };
    mutate(world, |s| {
        let id = component(s, handle)?;
        if !matches!(
            s.native
                .component(id)
                .map_err(crate::errors::AbiError::from)?
                .desc,
            ComponentDesc::Body2d(_)
        ) {
            return Err("component is not a body".into());
        }
        let mut next = body(value)?;
        let ComponentDesc::Body2d(previous) = &s
            .native
            .component(id)
            .map_err(crate::errors::AbiError::from)?
            .desc
        else {
            unreachable!()
        };
        next.shared_material = previous.shared_material;
        next.interpolation = previous.interpolation;
        s.native
            .set_component(id, ComponentDesc::Body2d(next))
            .map_err(crate::errors::AbiError::from)
    })
}

/// The body's native component, or an error naming a component of another kind.
fn body_component(
    session: &Session,
    handle: u64,
) -> Result<(ComponentHandle, Body2d), crate::errors::AbiError> {
    let id = component(session, handle)?;
    match session
        .native
        .component(id)
        .map_err(crate::errors::AbiError::from)?
        .desc
        .clone()
    {
        ComponentDesc::Body2d(body) => Ok((id, body)),
        _ => Err("component is not a body".into()),
    }
}

/// Sets the presentation mode: None (0), Interpolate (1) or Extrapolate (2).
#[unsafe(no_mangle)]
pub extern "C" fn disunity_body_interpolation_write(
    world: u64,
    handle: u64,
    interpolate: u32,
) -> DisunityResult {
    let interpolation = match interpolate {
        0 => Interpolation::None,
        1 => Interpolation::Interpolate,
        2 => Interpolation::Extrapolate,
        _ => return DisunityResult::invalid_argument(),
    };
    mutate(world, |s| {
        let (id, mut body) = body_component(s, handle)?;
        body.interpolation = interpolation;
        s.native
            .set_component(id, ComponentDesc::Body2d(body))
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// `interpolate` is writable for one presentation-mode tag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_body_interpolation_read(
    world: u64,
    handle: u64,
    interpolate: *mut u32,
) -> DisunityResult {
    if interpolate.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, body) = body_component(s, handle)?;
        unsafe {
            *interpolate = match body.interpolation {
                Interpolation::None => 0,
                Interpolation::Interpolate => 1,
                Interpolation::Extrapolate => 2,
            }
        };
        Ok(())
    })
}

/// # Safety
/// `output` is writable for one body record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_body_read(
    world: u64,
    handle: u64,
    output: *mut DisunityBody2d,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let ComponentDesc::Body2d(b) = &s
            .native
            .component(component(s, handle)?)
            .map_err(crate::errors::AbiError::from)?
            .desc
        else {
            return Err("component is not a body".into());
        };
        unsafe {
            *output = DisunityBody2d {
                kind: match b.body_type {
                    BodyType::Dynamic => 0,
                    BodyType::Kinematic => 1,
                    BodyType::Static => 2,
                },
                enabled: u32::from(b.enabled),
                mass: b.mass,
                velocity_x: b.velocity.x,
                velocity_y: b.velocity.y,
                angular_velocity: b.angular_velocity,
                gravity_scale: b.gravity_scale,
                freeze_rotation: u32::from(b.freeze_rotation),
                freeze_position: u32::from(b.freeze_position),
                restitution: b.restitution,
            }
        };
        Ok(())
    })
}

fn geometry(v: DisunityCollider2d) -> Result<ColliderGeometry, crate::errors::AbiError> {
    Ok(match v.kind {
        0 => ColliderGeometry::Shapes(vec![Shape2d::Box {
            half_extents: Vec2::new(v.size_x, v.size_y) * 0.5,
        }]),
        1 => ColliderGeometry::Shapes(vec![Shape2d::Circle { radius: v.radius }]),
        2 => ColliderGeometry::Tilemap,
        _ => return Err("unsupported collider shape".into()),
    })
}

fn collider(
    session: &Session,
    handle: u64,
) -> Result<(ComponentHandle, Collider2d), crate::errors::AbiError> {
    let id = component(session, handle)?;
    match &session.native.component(id)?.desc {
        ComponentDesc::Collider2d(c) => Ok((id, c.clone())),
        _ => Err("component is not a collider".into()),
    }
}

/// Writes a box (kind 0), circle (kind 1) or tilemap (kind 2) collider. A tilemap collider takes
/// its shapes from the Tilemap on its object, so its size and radius are ignored.
/// # Safety
/// `value` points to one readable collider record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_collider_write(
    world: u64,
    handle: u64,
    value: *const DisunityCollider2d,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let v = unsafe { *value };
    mutate(world, |s| {
        if v.enabled > 1 || v.trigger > 1 {
            return Err("invalid collider flags".into());
        }
        let (id, previous) = collider(s, handle)?;
        let value = Collider2d {
            geometry: geometry(v)?,
            offset: Vec2::new(v.offset_x, v.offset_y),
            enabled: v.enabled == 1,
            trigger: v.trigger == 1,
            layers: v.layers,
            ..previous
        };
        s.native
            .set_component(id, ComponentDesc::Collider2d(value))?;
        Ok(())
    })
}

/// # Safety
/// `output` is writable for one collider record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_collider_read(
    world: u64,
    handle: u64,
    output: *mut DisunityCollider2d,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, c) = collider(s, handle)?;
        let mut v = DisunityCollider2d {
            enabled: u32::from(c.enabled),
            trigger: u32::from(c.trigger),
            layers: c.layers,
            offset_x: c.offset.x,
            offset_y: c.offset.y,
            ..Default::default()
        };
        match &c.geometry {
            ColliderGeometry::Shapes(shapes) => match shapes.as_slice() {
                [Shape2d::Box { half_extents }] => {
                    v.size_x = half_extents.x * 2.0;
                    v.size_y = half_extents.y * 2.0;
                }
                [Shape2d::Circle { radius }] => {
                    v.kind = 1;
                    v.radius = *radius;
                }
                _ => return Err("unsupported collider shape projection".into()),
            },
            ColliderGeometry::Tilemap => v.kind = 2,
        }
        unsafe { *output = v };
        Ok(())
    })
}

/// # Safety
/// `output` is writable for one hit. A zero collider means no hit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_physics_raycast(
    world: u64,
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    distance: f32,
    layers: u32,
    output: *mut DisunityRayHit,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let hits = s
            .native
            .raycast(
                Vec2::new(x, y),
                Vec2::new(dx, dy),
                distance,
                QueryFilter {
                    layers,
                    include_triggers: true,
                },
            )
            .map_err(crate::errors::AbiError::from)?;
        let value = if let Some(hit) = hits.first() {
            DisunityRayHit {
                collider: identity(s, hit.collider)?,
                point_x: hit.point.x,
                point_y: hit.point.y,
                normal_x: hit.normal.x,
                normal_y: hit.normal.y,
                distance: hit.distance,
            }
        } else {
            DisunityRayHit::default()
        };
        unsafe { *output = value };
        Ok(())
    })
}

/// # Safety
/// `output` is writable for one collider identity, zero for no overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_physics_overlap_circle(
    world: u64,
    x: f32,
    y: f32,
    radius: f32,
    layers: u32,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let hits = s
            .native
            .overlap_shape(
                &Shape2d::Circle { radius },
                Vec2::new(x, y),
                QueryFilter {
                    layers,
                    include_triggers: true,
                },
            )
            .map_err(crate::errors::AbiError::from)?;
        let id = hits
            .first()
            .map(|&h| identity(s, h))
            .transpose()?
            .unwrap_or(0);
        unsafe { *output = id };
        Ok(())
    })
}

#[cfg(test)]
mod extrapolation_tests {
    use super::*;
    use glam::{Mat4, Quat, Vec3};
    use worldrunner_world_simulation::{
        LocalTransform, ObjectDesc, ObjectHandle, ParentSpace, World,
    };

    struct Fixture {
        world: u64,
        object: u64,
        body: u64,
    }

    impl Fixture {
        fn new(mode: u32) -> Self {
            let mut fixture = Self {
                world: 0,
                object: 0,
                body: 0,
            };
            unsafe {
                assert_eq!(
                    crate::abi::disunity_world_create(&mut fixture.world).code,
                    0
                );
                assert_eq!(
                    crate::abi::disunity_gameobject_create(fixture.world, &mut fixture.object).code,
                    0
                );
                assert_eq!(
                    disunity_physics_create(fixture.world, fixture.object, 0, &mut fixture.body)
                        .code,
                    0
                );
            }
            fixture.write(DisunityBody2d {
                mass: 1.0,
                enabled: 1,
                velocity_x: 10.0,
                angular_velocity: 10.0,
                ..Default::default()
            });
            fixture.mode(mode);
            fixture
        }

        fn mode(&self, mode: u32) {
            assert_eq!(
                disunity_body_interpolation_write(self.world, self.body, mode).code,
                0,
                "requested presentation mode must be accepted"
            );
        }

        fn read_mode(&self) -> u32 {
            let mut mode = u32::MAX;
            assert_eq!(
                unsafe { disunity_body_interpolation_read(self.world, self.body, &mut mode) }.code,
                0
            );
            mode
        }

        fn read(&self) -> DisunityBody2d {
            let mut value = DisunityBody2d::default();
            assert_eq!(
                unsafe { disunity_body_read(self.world, self.body, &mut value) }.code,
                0
            );
            value
        }

        fn write(&self, value: DisunityBody2d) {
            assert_eq!(
                unsafe { disunity_body_write(self.world, self.body, &value) }.code,
                0
            );
        }

        fn step(&self, numerator: u64, denominator: u64) {
            let mut work = crate::scheduling::DisunityAdvance::default();
            assert_eq!(
                unsafe {
                    crate::scheduling::disunity_world_advance(
                        self.world,
                        numerator,
                        denominator,
                        &mut work,
                    )
                }
                .code,
                0
            );
            assert_eq!(work.kind, 0, "synthetic body has no scripts");
        }

        fn inspect<T>(&self, read: impl FnOnce(&World, ObjectHandle) -> T) -> T {
            let store = crate::lock_store();
            let session = store.session(self.world).unwrap();
            read(&session.native, session.object(self.object).unwrap())
        }

        fn edit(&self, edit: impl FnOnce(&mut World, ObjectHandle, ComponentHandle)) {
            let mut store = crate::lock_store();
            let session = store.session_mut(self.world).unwrap();
            let object = session.object(self.object).unwrap();
            let body = component(session, self.body).unwrap();
            edit(&mut session.native, object, body);
        }

        fn poses(&self) -> [Mat4; 2] {
            self.inspect(|w, object| {
                [
                    w.presented_matrix(object).unwrap(),
                    w.world_matrix(object).unwrap(),
                ]
            })
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            assert_eq!(
                unsafe { crate::abi::disunity_world_destroy(self.world) }.code,
                0
            );
        }
    }

    #[test]
    fn extrapolation_mode_round_trips_and_rejects_invalid_tags() {
        let fixture = Fixture::new(2);
        assert_eq!(fixture.read_mode(), 2);
        let mut body = fixture.read();
        body.mass = 2.0;
        fixture.write(body);
        assert_eq!(
            fixture.read_mode(),
            2,
            "ordinary body writes preserve the mode"
        );
        for invalid in [3, u32::MAX] {
            assert_eq!(
                disunity_body_interpolation_write(fixture.world, fixture.body, invalid).code,
                crate::result_code::INVALID_ARGUMENT
            );
            assert_eq!(
                fixture.read_mode(),
                2,
                "invalid tags leave the previous mode unchanged"
            );
        }
        for mode in [0, 1, 2] {
            fixture.mode(mode);
            assert_eq!(fixture.read_mode(), mode);
        }
    }

    #[test]
    fn extrapolation_predicts_linear_angular_and_child_presentation_only() {
        let fixture = Fixture::new(2);
        fixture.step(1, 100);
        assert_eq!(
            fixture.poses(),
            [Mat4::IDENTITY; 2],
            "no completed physics step yet"
        );
        fixture.step(1, 100);
        assert_eq!(
            fixture.poses()[0],
            fixture.poses()[1],
            "zero residual draws the last step"
        );
        fixture.edit(|world, object, _| {
            let child = world.create_object(ObjectDesc::default()).unwrap();
            world
                .set_parent(child, Some(object), ParentSpace::KeepLocal)
                .unwrap();
            world
                .set_local_transform(
                    child,
                    LocalTransform {
                        translation: Vec3::Y,
                        ..Default::default()
                    },
                )
                .unwrap();
        });
        fixture.step(1, 100);
        let expected =
            Mat4::from_rotation_translation(Quat::from_rotation_z(0.3), Vec3::new(0.3, 0.0, 0.0));
        let [drawn, physics] = fixture.poses();
        assert!(drawn.abs_diff_eq(expected, 1e-6));
        assert!((physics.w_axis.x - 0.2).abs() < 1e-6);
        let child_poses = fixture.inspect(|world, _| {
            let child = world.snapshot().objects[1].handle;
            [
                world.presented_matrix(child).unwrap(),
                world.world_matrix(child).unwrap(),
            ]
        });
        assert!(
            child_poses[0]
                .w_axis
                .truncate()
                .abs_diff_eq(expected.transform_point3(Vec3::Y), 1e-6)
        );
        assert_ne!(child_poses[0], child_poses[1]);
        fixture.step(9, 1000);
        assert!(
            (fixture.poses()[0].w_axis.x - 0.39).abs() < 1e-6,
            "prediction stays within the next fixed step"
        );
        fixture.step(1, 1000);
        assert!(fixture.poses()[0].abs_diff_eq(fixture.poses()[1], 1e-6));
        assert!(
            (fixture.poses()[1].w_axis.x - 0.4).abs() < 1e-6,
            "presentation never advanced physics"
        );
    }

    fn wall(fixture: &Fixture) {
        let mut body = fixture.read();
        body.angular_velocity = 0.0;
        fixture.write(body);
        fixture.edit(|world, object, _| {
            let collider = || {
                ComponentDesc::Collider2d(Collider2d {
                    geometry: ColliderGeometry::Shapes(vec![Shape2d::Box {
                        half_extents: Vec2::splat(0.1),
                    }]),
                    ..Default::default()
                })
            };
            world.add_component(object, collider()).unwrap();
            let wall = world.create_object(ObjectDesc::default()).unwrap();
            world
                .set_local_transform(
                    wall,
                    LocalTransform {
                        translation: Vec3::new(0.4, 0.0, 0.0),
                        ..Default::default()
                    },
                )
                .unwrap();
            world.add_component(wall, collider()).unwrap();
        });
    }

    #[test]
    fn extrapolation_uses_post_collision_velocity_without_changing_contacts() {
        let predicted = Fixture::new(2);
        let fixed = Fixture::new(0);
        wall(&predicted);
        wall(&fixed);
        predicted.step(5, 100);
        fixed.step(5, 100);
        assert_eq!(predicted.read().velocity_x, 0.0);
        assert_eq!(predicted.poses()[1], fixed.poses()[1]);
        assert_eq!(
            predicted.poses()[0],
            predicted.poses()[1],
            "a stopped body is not predicted through the wall"
        );
        let contacts = |fixture: &Fixture| {
            fixture.inspect(|world, _| {
                world
                    .contacts()
                    .iter()
                    .map(|c| (c.phase, c.trigger, c.normal, c.point))
                    .collect::<Vec<_>>()
            })
        };
        let observed = contacts(&predicted);
        assert!(!observed.is_empty());
        assert_eq!(observed, contacts(&fixed));
        assert_eq!(predicted.read().velocity_x, fixed.read().velocity_x);
    }

    #[test]
    fn extrapolation_respects_sleep_constraints_and_live_eligibility() {
        let fixture = Fixture::new(2);
        let mut body = fixture.read();
        body.velocity_y = 4.0;
        body.freeze_position = 1;
        body.freeze_rotation = 1;
        fixture.write(body);
        fixture.step(3, 100);
        let expected = Mat4::from_translation(Vec3::new(0.0, 0.12, 0.0));
        assert!(fixture.poses()[0].abs_diff_eq(expected, 1e-6));
        fixture.edit(|world, _, body| world.set_body_sleeping(body, true).unwrap());
        assert_eq!(
            fixture.poses()[0],
            fixture.poses()[1],
            "sleep suppresses prediction immediately"
        );
        fixture.edit(|world, _, body| world.set_body_sleeping(body, false).unwrap());
        for (enabled, kind) in [(0, 0), (1, 2)] {
            let mut body = fixture.read();
            body.enabled = enabled;
            body.kind = kind;
            body.velocity_y = 4.0;
            fixture.write(body);
            assert_eq!(
                fixture.poses()[0],
                fixture.poses()[1],
                "disabled and static bodies draw authoritative poses"
            );
        }
    }

    #[test]
    fn extrapolation_teleports_and_mode_changes_use_authoritative_pose() {
        let fixture = Fixture::new(2);
        fixture.step(3, 100);
        fixture.edit(|world, object, _| {
            world
                .set_local_transform(
                    object,
                    LocalTransform {
                        translation: Vec3::new(5.0, 0.0, 0.0),
                        ..Default::default()
                    },
                )
                .unwrap()
        });
        assert_eq!(
            fixture.poses(),
            [Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0)); 2]
        );
        fixture.step(2, 100);
        let physics = fixture.poses()[1];
        assert!((physics.w_axis.x - 5.2).abs() < 1e-6);
        assert!((fixture.poses()[0].w_axis.x - 5.3).abs() < 1e-6);
        fixture.mode(0);
        assert_eq!(fixture.poses()[0], physics);
        fixture.mode(1);
        assert!((fixture.poses()[0].w_axis.x - 5.1).abs() < 1e-6);
        assert_eq!(fixture.poses()[1], physics);
    }

    #[test]
    fn existing_none_and_interpolate_keep_their_presentation() {
        for (mode, expected) in [(0, 0.2), (1, 0.1)] {
            let fixture = Fixture::new(mode);
            fixture.step(3, 100);
            let [drawn, physics] = fixture.poses();
            assert!((drawn.w_axis.x - expected).abs() < 1e-6);
            assert!((physics.w_axis.x - 0.2).abs() < 1e-6);
        }
    }
}
