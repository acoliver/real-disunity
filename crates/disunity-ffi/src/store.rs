use std::{collections::BTreeMap, sync::Mutex};

use worldrunner_world_simulation::{ObjectDesc, ObjectHandle, World, WorldSettings};

use crate::DisunityCommand;

#[derive(Debug)]
pub(crate) enum AllocationError {
    InvalidWorld,
    HandleCapacity,
    Native(worldrunner_world_simulation::WorldError),
}

pub(crate) struct Session {
    pub native: World,
    pub presentation:
        Option<worldrunner_world_simulation::presentation::extraction::WorldRenderPlan>,
    pub physics_materials:
        BTreeMap<u64, worldrunner_world_simulation::physics2d::PhysicsMaterialHandle>,
    pub fonts: worldrunner_world_simulation::ui::FontBook,
    pub font_ids: BTreeMap<u64, worldrunner_world_simulation::FontId>,
    pub clones: BTreeMap<u64, crate::cloning::PendingClone>,
    pub constructions: BTreeMap<u64, crate::construction::PendingConstruction>,
    pub dispatch: crate::scheduling::Dispatch,
    pub objects: crate::object_ids::ObjectIds,
    pub pending_commands: Vec<DisunityCommand>,
    pub last_error: Option<String>,
}

struct Entry {
    handle: u64,
    session: Session,
}

pub(crate) struct Store {
    worlds: Vec<Option<Entry>>,
    next_handle: u64,
}

static STORE: Mutex<Store> = Mutex::new(Store {
    worlds: Vec::new(),
    next_handle: 1,
});

pub(crate) fn lock_store() -> std::sync::MutexGuard<'static, Store> {
    STORE.lock().expect("ABI session mutation panicked")
}

impl Store {
    pub(crate) fn allocate_handle(&mut self) -> Result<u64, AllocationError> {
        let handle = self.next_handle;
        self.next_handle = handle
            .checked_add(1)
            .ok_or(AllocationError::HandleCapacity)?;
        Ok(handle)
    }

    pub fn session(&self, handle: u64) -> Result<&Session, crate::errors::AbiError> {
        self.worlds
            .iter()
            .flatten()
            .find(|entry| entry.handle == handle)
            .map(|entry| &entry.session)
            .ok_or_else(|| "world handle is stale".into())
    }

    pub fn session_mut(&mut self, handle: u64) -> Result<&mut Session, crate::errors::AbiError> {
        self.worlds
            .iter_mut()
            .flatten()
            .find(|entry| entry.handle == handle)
            .map(|entry| &mut entry.session)
            .ok_or_else(|| "world handle is stale".into())
    }
}

impl Session {
    pub fn object(&self, handle: u64) -> Result<ObjectHandle, crate::errors::AbiError> {
        let object = self
            .objects
            .get(&handle)
            .copied()
            .ok_or("object handle is stale or belongs to another world")?;
        if !self.native.object_alive(object) {
            // Building the snapshot only on failure keeps the world's own error for the caller.
            self.native
                .object(object)
                .map_err(crate::errors::AbiError::from)?;
        }
        Ok(object)
    }
}

pub(crate) fn create_world(store: &mut Store) -> Result<u64, AllocationError> {
    let mut native = World::new(WorldSettings::default()).map_err(AllocationError::Native)?;
    use worldrunner_world_simulation::{AxisBinding, AxisId, KeyCode};
    let binding = |negative: &[u16], positive: &[u16]| AxisBinding {
        negative: negative.iter().copied().map(KeyCode).collect(),
        positive: positive.iter().copied().map(KeyCode).collect(),
        invert: false,
    };
    native
        .set_input_axes(BTreeMap::from([
            (AxisId(0), binding(&[2, 30], &[5, 31])),
            (AxisId(1), binding(&[20, 29], &[24, 28])),
        ]))
        .map_err(AllocationError::Native)?;
    let handle = store.allocate_handle()?;
    let entry = Some(Entry {
        handle,
        session: Session {
            native,
            presentation: None,
            physics_materials: BTreeMap::new(),
            fonts: worldrunner_world_simulation::ui::FontBook::new(Default::default())
                .map_err(|e| AllocationError::Native(e.into()))?,
            font_ids: BTreeMap::new(),
            clones: BTreeMap::new(),
            constructions: BTreeMap::new(),
            dispatch: Default::default(),
            objects: crate::object_ids::ObjectIds::default(),
            pending_commands: Vec::new(),
            last_error: None,
        },
    });
    if let Some(slot) = store.worlds.iter_mut().find(|slot| slot.is_none()) {
        *slot = entry;
    } else {
        store.worlds.push(entry);
    }
    Ok(handle)
}

pub(crate) fn destroy_world(store: &mut Store, handle: u64) -> Result<(), crate::errors::AbiError> {
    let entry = store
        .worlds
        .iter_mut()
        .find(|slot| slot.as_ref().is_some_and(|e| e.handle == handle))
        .ok_or("world handle is stale")?;
    *entry = None;
    Ok(())
}

pub(crate) fn create_object(store: &mut Store, world: u64) -> Result<u64, AllocationError> {
    store
        .session(world)
        .map_err(|_| AllocationError::InvalidWorld)?;
    let handle = store.allocate_handle()?;
    let session = store.session_mut(world).expect("validated world handle");
    let object = session
        .native
        .create_object(ObjectDesc::default())
        .map_err(AllocationError::Native)?;
    session.objects.insert(handle, object);
    Ok(handle)
}

pub(crate) fn destroy_object(
    store: &mut Store,
    world: u64,
    object: u64,
) -> Result<(), crate::errors::AbiError> {
    let session = store.session_mut(world)?;
    session
        .native
        .destroy(
            session.object(object)?,
            worldrunner_world_simulation::DestroyWhen::EndOfFrame,
        )
        .map_err(crate::errors::AbiError::from)?;
    session.objects.prune(&session.native);
    Ok(())
}

pub(crate) fn last_error(handle: u64) -> Result<Option<String>, crate::errors::AbiError> {
    let store = lock_store();
    Ok(store.session(handle)?.last_error.clone())
}

/// Observes the same native World owned by the ABI image, without a copied store.
/// The callback holds the global store mutex and must not reenter ABI/store functions.
pub fn with_native_world<T>(
    handle: u64,
    f: impl FnOnce(&World) -> T,
) -> Result<T, crate::errors::AbiError> {
    let store = lock_store();
    Ok(f(&store.session(handle)?.native))
}

pub(crate) fn record_error(store: &mut Store, handle: Option<u64>, message: &str) {
    if let Some(handle) = handle
        && let Ok(session) = store.session_mut(handle)
    {
        session.last_error = Some(message.to_owned());
    }
}

pub fn store_statistics() -> (usize, usize) {
    let store = lock_store();
    (store.worlds.iter().flatten().count(), store.worlds.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_handle_exhaustion_is_checked_before_allocation() {
        let mut store = Store {
            worlds: Vec::new(),
            next_handle: u64::MAX,
        };
        assert!(create_world(&mut store).is_err());
        assert!(store.worlds.is_empty());
        assert_eq!(store.next_handle, u64::MAX);
    }
}
