use glam::EulerRot;
use worldrunner_world_simulation::{ComponentDesc, ObjectHandle};

use crate::{
    DisunityObjectState,
    store::{Session, lock_store},
};

/// Detached observation retained for existing Rust inspection callers.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityObject {
    pub position: [f32; 3],
    pub rotation_y_radians: f32,
    pub material_slot: u32,
}

/// A detached read-only observation, never live storage or a mutation target.
pub struct DisunityWorld {
    /// Compact live observations; vector positions are not object identities.
    pub objects: Vec<Option<DisunityObject>>,
    pub last_error: Option<String>,
}

pub(crate) fn observe(
    session: &Session,
    object: ObjectHandle,
) -> Result<DisunityObject, crate::errors::AbiError> {
    let transform = session
        .native
        .local_transform(object)
        .map_err(crate::errors::AbiError::from)?;
    let material_slot = match session
        .native
        .renderer(object)
        .map_err(crate::errors::AbiError::from)?
    {
        None => 0,
        Some(handle) => match session
            .native
            .component(handle)
            .map_err(crate::errors::AbiError::from)?
            .desc
        {
            ComponentDesc::Renderer { material_slot } => material_slot,
            _ => return Err("renderer component kind mismatch".into()),
        },
    };
    Ok(DisunityObject {
        position: session
            .native
            .world_position(object)
            .map_err(crate::errors::AbiError::from)?
            .to_array(),
        rotation_y_radians: transform.rotation.to_euler(EulerRot::YXZ).0,
        material_slot,
    })
}

pub fn with_world<T>(
    handle: u64,
    f: impl FnOnce(&DisunityWorld) -> T,
) -> Result<T, crate::errors::AbiError> {
    let store = lock_store();
    let session = store.session(handle)?;
    let snapshot = session.native.snapshot();
    let view = DisunityWorld {
        objects: snapshot
            .objects
            .iter()
            .map(|object| observe(session, object.handle).map(Some))
            .collect::<Result<_, _>>()?,
        last_error: session.last_error.clone(),
    };
    Ok(f(&view))
}

pub fn export_object_states(
    handle: u64,
    out: &mut [DisunityObjectState],
    written: &mut u32,
) -> Result<(), crate::errors::AbiError> {
    *written = 0;
    let store = lock_store();
    let session = store.session(handle)?;
    *written = u32::try_from(session.objects.len()).map_err(|_| "object count overflow")?;
    for ((&encoded, &object), destination) in session.objects.iter().zip(out.iter_mut()) {
        let value = observe(session, object)?;
        *destination = DisunityObjectState {
            object: encoded,
            x: value.position[0],
            y: value.position[1],
            z: value.position[2],
            rotation_y_radians: value.rotation_y_radians,
            material_slot: value.material_slot,
        };
    }
    Ok(())
}
