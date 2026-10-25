use glam::EulerRot;
use worldrunner_world_simulation::{ComponentDesc, ObjectHandle};

use crate::{
    DisunityCommand, DisunityObjectState,
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
/// Existing command-log inspection remains available during API replacement.
pub struct DisunityWorld {
    /// Compact live observations; vector positions are not object identities.
    pub objects: Vec<Option<DisunityObject>>,
    pub applied_commands: Vec<DisunityCommand>,
    pub pending_commands: Vec<DisunityCommand>,
    pub last_error: Option<String>,
}

fn observe(session: &Session, object: ObjectHandle) -> DisunityObject {
    let transform = session.native.local_transform(object).expect("live object");
    let material_slot = session
        .native
        .renderer(object)
        .expect("live snapshot object")
        .map_or(0, |handle| {
            let ComponentDesc::Renderer { material_slot } = session
                .native
                .component(handle)
                .expect("live renderer")
                .desc
            else {
                unreachable!("renderer component kind")
            };
            material_slot
        });
    DisunityObject {
        position: transform.translation.to_array(),
        // The ABI projects orientation to principal Euler yaw, without turn counts.
        rotation_y_radians: transform.rotation.to_euler(EulerRot::YXZ).0,
        material_slot,
    }
}

pub fn with_world<T>(handle: u64, f: impl FnOnce(&DisunityWorld) -> T) -> Result<T, String> {
    let store = lock_store();
    let session = store.session(handle)?;
    let snapshot = session.native.snapshot();
    let view = DisunityWorld {
        objects: snapshot
            .objects
            .iter()
            .map(|object| Some(observe(session, object.handle)))
            .collect(),
        applied_commands: session.applied_commands.clone(),
        pending_commands: session.pending_commands.clone(),
        last_error: session.last_error.clone(),
    };
    Ok(f(&view))
}

pub fn export_object_states(
    handle: u64,
    out: &mut [DisunityObjectState],
    written: &mut u32,
) -> Result<(), String> {
    *written = 0;
    let store = lock_store();
    let session = store.session(handle)?;
    *written = u32::try_from(session.objects.len()).map_err(|_| "object count overflow")?;
    for ((&encoded, &object), destination) in session.objects.iter().zip(out.iter_mut()) {
        let value = observe(session, object);
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
