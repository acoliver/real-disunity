//! Shared physics assets and component references over the native world.
use crate::{
    DisunityResult, callbacks::mutate, errors::AbiError, physics::component, store::Session,
};
use worldrunner_world_simulation::{
    ComponentDesc,
    physics2d::{PhysicsMaterial2d, PhysicsMaterialHandle},
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityPhysicsMaterial2d {
    pub friction: f32,
    pub bounciness: f32,
}
fn decode(value: DisunityPhysicsMaterial2d) -> Result<PhysicsMaterial2d, AbiError> {
    PhysicsMaterial2d::new(value.friction, value.bounciness)
        .map_err(|e| worldrunner_world_simulation::WorldError::from(e).into())
}
fn material(s: &Session, handle: u64) -> Result<PhysicsMaterialHandle, AbiError> {
    s.physics_materials
        .get(&handle)
        .copied()
        .ok_or_else(|| worldrunner_world_simulation::WorldError::WrongWorld.into())
}
/// # Safety
/// `output` is writable for one material identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_physics_material_create(
    world: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let result = (|| {
        let mut store = crate::lock_store();
        let id = store.allocate_handle().map_err(AbiError::from)?;
        let s = store.session_mut(world)?;
        let native = s.native.create_physics_material(
            PhysicsMaterial2d::new(0.4, 0.0).expect("valid asset defaults"),
        )?;
        s.physics_materials.insert(id, native);
        Ok::<_, AbiError>(id)
    })();
    match result {
        Ok(id) => {
            unsafe { *output = id };
            DisunityResult::ok()
        }
        Err(e) => crate::abi::translate_error(Some(world), &e),
    }
}
/// # Safety
/// `output` is writable for one material record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_physics_material_read(
    world: u64,
    handle: u64,
    output: *mut DisunityPhysicsMaterial2d,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let value = s.native.physics_material(material(s, handle)?)?;
        unsafe {
            *output = DisunityPhysicsMaterial2d {
                friction: value.friction(),
                bounciness: value.bounciness(),
            }
        };
        Ok(())
    })
}
/// # Safety
/// `value` points to one readable material record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_physics_material_write(
    world: u64,
    handle: u64,
    value: *const DisunityPhysicsMaterial2d,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let value = unsafe { *value };
    mutate(world, |s| {
        s.native
            .set_physics_material(material(s, handle)?, decode(value)?)?;
        Ok(())
    })
}
/// # Safety
/// `output` is writable for one identity; zero denotes no explicit asset.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_shared_material_read(
    world: u64,
    handle: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let c = s.native.component(component(s, handle)?)?;
        let native = match &c.desc {
            ComponentDesc::Body2d(b) => b.shared_material,
            ComponentDesc::Collider2d(c) => c.shared_material,
            _ => return Err("component has no physics material".into()),
        };
        let id = match native {
            None => 0,
            Some(h) => s
                .physics_materials
                .iter()
                .find_map(|(&id, &value)| (value == h).then_some(id))
                .ok_or("physics material has no managed identity")?,
        };
        unsafe { *output = id };
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn disunity_shared_material_write(
    world: u64,
    handle: u64,
    asset: u64,
) -> DisunityResult {
    mutate(world, |s| {
        let native = if asset == 0 {
            None
        } else {
            Some(material(s, asset)?)
        };
        let id = component(s, handle)?;
        let mut desc = s.native.component(id)?.desc.clone();
        match &mut desc {
            ComponentDesc::Body2d(b) => b.shared_material = native,
            ComponentDesc::Collider2d(c) => c.shared_material = native,
            _ => return Err("component has no physics material".into()),
        }
        s.native.set_component(id, desc)?;
        Ok(())
    })
}
