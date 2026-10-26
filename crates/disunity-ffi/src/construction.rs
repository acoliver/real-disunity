//! Batch construction across the ABI. Objects created in one construction, with their components,
//! scripts and fields, stay unpublished until activation runs their `Awake` and `OnEnable`
//! together. Component and script creation calls work unchanged on constructing objects.
use crate::{
    abi::DisunityResult,
    errors::AbiError,
    scheduling::DisunityAdvance,
    store::{Session, lock_store},
};
use std::collections::BTreeSet;
use worldrunner_world_simulation::{ComponentHandle, Construction, ObjectDesc, ObjectHandle};

pub(crate) struct PendingConstruction {
    native: Construction,
    objects: Vec<u64>,
}

/// Opens a construction and writes its handle into `output`.
/// # Safety
/// `output` is writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_construction_begin(
    world: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let result = (|| {
        let mut store = lock_store();
        store.session(world)?;
        let handle = store.allocate_handle().map_err(AbiError::from)?;
        let session = store.session_mut(world)?;
        let native = session
            .native
            .begin_construction()
            .map_err(AbiError::from)?;
        session.constructions.insert(
            handle,
            PendingConstruction {
                native,
                objects: Vec::new(),
            },
        );
        Ok(handle)
    })();
    unsafe { write(world, output, result) }
}

/// Creates an object that stays unpublished until the construction is activated.
/// # Safety
/// `output` is writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_construction_object_create(
    world: u64,
    construction: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let result = (|| {
        let mut store = lock_store();
        store.session(world)?;
        let handle = store.allocate_handle().map_err(AbiError::from)?;
        let session = store.session_mut(world)?;
        let pending = session
            .constructions
            .get_mut(&construction)
            .ok_or("unknown construction")?;
        let object = session
            .native
            .create_object_in(pending.native, ObjectDesc::default())
            .map_err(AbiError::from)?;
        pending.objects.push(handle);
        session.objects.insert(handle, object);
        Ok(handle)
    })();
    unsafe { write(world, output, result) }
}

/// Publishes the construction and starts its lifecycle work. Drain `output` as for any advance.
/// # Safety
/// `output` is writable for one callback record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_construction_activate(
    world: u64,
    construction: u64,
    output: *mut DisunityAdvance,
) -> DisunityResult {
    crate::scheduling::call(world, output, |session| {
        let native = session
            .constructions
            .get(&construction)
            .ok_or("unknown construction")?
            .native;
        let advance = session
            .native
            .activate_construction(native)
            .map_err(AbiError::from)?;
        session.constructions.remove(&construction);
        Ok(advance)
    })
}

/// Removes everything the construction created, including its component and script identities.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_construction_abort(world: u64, construction: u64) -> DisunityResult {
    let result = (|| {
        let mut store = lock_store();
        let session = store.session_mut(world)?;
        let pending = session
            .constructions
            .get(&construction)
            .ok_or("unknown construction")?;
        let owned = Owned::collect(session, &pending.objects);
        let native = pending.native;
        session
            .native
            .discard_construction(native)
            .map_err(AbiError::from)?;
        let pending = session
            .constructions
            .remove(&construction)
            .expect("validated construction");
        owned.release(session, &pending.objects);
        Ok(())
    })();
    result.map_or_else(
        |error: AbiError| crate::abi::translate_error(Some(world), &error),
        |()| DisunityResult::ok(),
    )
}

/// Component and script identities registered on a construction's objects. Collected before
/// discarding, which invalidates the components.
struct Owned {
    scripts: Vec<ComponentHandle>,
    components: Vec<u64>,
}

impl Owned {
    fn collect(session: &Session, objects: &[u64]) -> Self {
        let natives: BTreeSet<ObjectHandle> = objects
            .iter()
            .filter_map(|id| session.objects.get(id).copied())
            .collect();
        let owned = |handle: &ComponentHandle| {
            session
                .native
                .component(*handle)
                .is_ok_and(|component| natives.contains(&component.object))
        };
        Self {
            scripts: session
                .dispatch
                .scripts
                .keys()
                .copied()
                .filter(owned)
                .collect(),
            components: session
                .dispatch
                .components
                .iter()
                .filter(|(_, handle)| owned(handle))
                .map(|(&id, _)| id)
                .collect(),
        }
    }

    fn release(self, session: &mut Session, objects: &[u64]) {
        for script in self.scripts {
            session.dispatch.scripts.remove(&script);
        }
        for component in self.components {
            session.dispatch.components.remove(&component);
        }
        for object in objects {
            session.objects.remove(object);
        }
    }
}

/// # Safety
/// `output` is writable for one value.
unsafe fn write(world: u64, output: *mut u64, result: Result<u64, AbiError>) -> DisunityResult {
    match result {
        Ok(handle) => {
            unsafe { *output = handle };
            DisunityResult::ok()
        }
        Err(error) => crate::abi::translate_error(Some(world), &error),
    }
}
