//! Two-pass native cloning. Script wrappers bind before the lifecycle scope opens.
use crate::{abi::DisunityResult, store::lock_store};
use std::collections::BTreeMap;
use worldrunner_world_simulation::{ComponentHandle, Instantiation};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityCloneRow {
    pub source: u64,
    pub target: u64,
    pub owner: u64,
    /// 0: object, 1: managed script, 2: native component.
    pub kind: u32,
}

pub(crate) struct PendingClone {
    pub native: Instantiation,
    pub rows: Vec<DisunityCloneRow>,
    pub unbound: BTreeMap<u64, ComponentHandle>,
}

/// Allocates the complete native subtree without invoking scripts.
/// # Safety
/// All output pointers are writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_clone_create(
    world: u64,
    template: u64,
    parent: u64,
    root: *mut u64,
    count: *mut u32,
) -> DisunityResult {
    if root.is_null() || count.is_null() {
        return DisunityResult::invalid_argument();
    }
    let result = clone_subtree(world, template, parent);
    match result {
        Ok((handle, length)) => {
            unsafe {
                *root = handle;
                *count = length;
            }
            DisunityResult::ok()
        }
        Err(error) => crate::abi::translate_error(Some(world), &error),
    }
}

fn clone_subtree(
    world: u64,
    template: u64,
    parent: u64,
) -> Result<(u64, u32), crate::errors::AbiError> {
    let mut store = lock_store();
    let session = store.session_mut(world)?;
    let original = session.object(template)?;
    let parent = if parent == 0 {
        None
    } else {
        Some(session.object(parent)?)
    };
    check_identities(session, original)?;
    let clone = session
        .native
        .instantiate_unpublished(original, parent)
        .map_err(crate::errors::AbiError::from)?;
    // The clone names every object and component it created, including those cloned from a
    // template that was never published, so it sizes the identities it needs.
    let needed = clone.objects.len() + clone.components.len();
    let handles = (0..needed)
        .map(|_| store.allocate_handle())
        .collect::<Result<Vec<_>, _>>();
    let session = store.session_mut(world)?;
    match handles {
        Ok(handles) => encode_clone(session, clone, handles.into_iter()),
        Err(error) => {
            session
                .native
                .discard_instantiation(&clone)
                .map_err(crate::errors::AbiError::from)?;
            Err(error.into())
        }
    }
}

fn encode_clone(
    session: &mut crate::store::Session,
    clone: Instantiation,
    mut handles: std::vec::IntoIter<u64>,
) -> Result<(u64, u32), crate::errors::AbiError> {
    let mut rows = Vec::new();
    let mut encoded = BTreeMap::new();
    let mut unbound = BTreeMap::new();
    for (&old, &new) in &clone.objects {
        let source = session
            .objects
            .iter()
            .find(|(_, value)| **value == old)
            .map(|(&id, _)| id)
            .ok_or("source object has no ABI identity")?;
        let target = handles.next().expect("reserved identities");
        session.objects.insert(target, new);
        encoded.insert(new, target);
        rows.push(DisunityCloneRow {
            source,
            target,
            owner: target,
            kind: 0,
        });
    }
    encode_components(
        session,
        &clone,
        &encoded,
        &mut handles,
        &mut rows,
        &mut unbound,
    )?;
    let root = encoded[&clone.root];
    let count = u32::try_from(rows.len()).map_err(|_| "clone rows exceed ABI capacity")?;
    session.clones.insert(
        root,
        PendingClone {
            native: clone,
            rows,
            unbound,
        },
    );
    Ok((root, count))
}

fn encode_components(
    session: &mut crate::store::Session,
    clone: &Instantiation,
    encoded: &BTreeMap<worldrunner_world_simulation::ObjectHandle, u64>,
    handles: &mut std::vec::IntoIter<u64>,
    rows: &mut Vec<DisunityCloneRow>,
    unbound: &mut BTreeMap<u64, ComponentHandle>,
) -> Result<(), crate::errors::AbiError> {
    for (&old, &new) in &clone.components {
        let binding = session
            .dispatch
            .scripts
            .get(&old)
            .map(|&id| (id, 1))
            .or_else(|| {
                session
                    .dispatch
                    .components
                    .identity_of(old)
                    .map(|id| (id, 2))
            });
        if let Some((source, kind)) = binding {
            let target = handles.next().expect("reserved identities");
            let owner = encoded[&session
                .native
                .component(new)
                .map_err(crate::errors::AbiError::from)?
                .object];
            rows.push(DisunityCloneRow {
                source,
                target,
                owner,
                kind,
            });
            if kind == 1 {
                unbound.insert(target, new);
            } else {
                session.dispatch.components.insert(target, new);
            }
        }
    }
    Ok(())
}

/// Copies all clone bindings, with no partial writes.
/// # Safety
/// Rows are writable for capacity entries, written for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_clone_copy(
    world: u64,
    root: u64,
    rows: *mut DisunityCloneRow,
    capacity: u32,
    written: *mut u32,
) -> DisunityResult {
    if written.is_null() || (rows.is_null() && capacity != 0) {
        return DisunityResult::invalid_argument();
    }
    let store = lock_store();
    let Ok(session) = store.session(world) else {
        return DisunityResult::invalid_handle();
    };
    let Some(clone) = session.clones.get(&root) else {
        return DisunityResult::invalid_handle();
    };
    unsafe {
        *written = clone.rows.len() as u32;
    }
    if (capacity as usize) < clone.rows.len() {
        return DisunityResult { code: 2, detail: 0 };
    }
    if !clone.rows.is_empty() {
        unsafe {
            std::ptr::copy_nonoverlapping(clone.rows.as_ptr(), rows, clone.rows.len());
        }
    }
    DisunityResult::ok()
}

#[unsafe(no_mangle)]
pub extern "C" fn disunity_clone_bind(
    world: u64,
    root: u64,
    target: u64,
    identity: u64,
) -> DisunityResult {
    let result = (|| {
        let mut store = lock_store();
        let session = store.session_mut(world)?;
        if identity == 0 || session.dispatch.scripts.contains_identity(identity) {
            return Err("duplicate or zero script identity".into());
        }
        let clone = session.clones.get_mut(&root).ok_or("unknown clone")?;
        let script = clone
            .unbound
            .remove(&target)
            .ok_or("unknown clone script")?;
        session.dispatch.scripts.insert(script, identity);
        Ok(())
    })();
    result.map_or_else(
        |e: crate::errors::AbiError| crate::abi::translate_error(Some(world), &e),
        |()| DisunityResult::ok(),
    )
}

/// # Safety
/// Output is writable for one callback record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_clone_activate(
    world: u64,
    root: u64,
    output: *mut crate::scheduling::DisunityAdvance,
) -> DisunityResult {
    crate::scheduling::call(world, output, |session| {
        let clone = session.clones.get(&root).ok_or("unknown clone")?;
        if !clone.unbound.is_empty() {
            return Err("clone scripts must bind before activation".into());
        }
        let construction = clone.native.construction();
        let advance = session
            .native
            .activate_construction(construction)
            .map_err(crate::errors::AbiError::from)?;
        session.clones.remove(&root);
        Ok(advance)
    })
}

/// Checks that every object in the source subtree has an ABI identity, so encoding the clone
/// cannot fail after the native clone exists.
fn check_identities(
    session: &crate::store::Session,
    root: worldrunner_world_simulation::ObjectHandle,
) -> Result<(), crate::errors::AbiError> {
    let mut pending = vec![root];
    while let Some(object) = pending.pop() {
        if session.objects.id_of(object).is_none() {
            return Err("source object has no ABI identity".into());
        }
        pending.extend(
            session
                .native
                .children(object)
                .map_err(crate::errors::AbiError::from)?,
        );
    }
    Ok(())
}

/// Discards an unpublished subtree after managed binding failure.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_clone_abort(world: u64, root: u64) -> DisunityResult {
    let result = (|| {
        let mut store = lock_store();
        let session = store.session_mut(world)?;
        let clone = session.clones.get(&root).ok_or("unknown clone")?;
        session
            .native
            .discard_instantiation(&clone.native)
            .map_err(crate::errors::AbiError::from)?;
        let clone = session.clones.remove(&root).expect("validated clone");
        for script in clone.native.components.values() {
            session.dispatch.scripts.remove(script);
        }
        for row in clone.rows.iter().filter(|row| row.kind == 2) {
            session.dispatch.components.remove(&row.target);
        }
        for row in clone.rows.iter().filter(|row| row.kind == 0) {
            session.objects.remove(&row.target);
        }
        Ok(())
    })();
    result.map_or_else(
        |e: crate::errors::AbiError| crate::abi::translate_error(Some(world), &e),
        |()| DisunityResult::ok(),
    )
}
