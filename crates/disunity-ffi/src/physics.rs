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

/// Sets whether the body is drawn between its poses either side of the last physics step,
/// Unity's `RigidbodyInterpolation2D.Interpolate` (1) or `None` (0).
#[unsafe(no_mangle)]
pub extern "C" fn disunity_body_interpolation_write(
    world: u64,
    handle: u64,
    interpolate: u32,
) -> DisunityResult {
    if interpolate > 1 {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (id, mut body) = body_component(s, handle)?;
        body.interpolation = if interpolate == 1 {
            Interpolation::Interpolate
        } else {
            Interpolation::None
        };
        s.native
            .set_component(id, ComponentDesc::Body2d(body))
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// `interpolate` is writable for one flag.
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
        unsafe { *interpolate = u32::from(body.interpolation == Interpolation::Interpolate) };
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
