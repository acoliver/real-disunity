//! Validates a serialized transform forest before constructing recursive scene nodes.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use worldrunner_unity_serialized::{classes::Transform, object::PPtr};

use crate::PlayerError;

// Bounds recursive SceneNode construction, display and destruction, including malformed input.
const MAX_DEPTH: usize = 256;

pub(crate) fn local_id(path: &Path, reference: PPtr) -> Result<i64, PlayerError> {
    if reference.file_id == 0 {
        return Ok(reference.path_id);
    }
    Err(PlayerError::UnresolvedReference {
        path: path.to_owned(),
        file_id: reference.file_id,
        path_id: reference.path_id,
    })
}

pub(crate) fn invalid(path: &Path, reason: impl Into<String>) -> PlayerError {
    PlayerError::InvalidHierarchy {
        path: path.to_owned(),
        reason: reason.into(),
    }
}

pub(crate) fn roots(
    path: &Path,
    transforms: &BTreeMap<i64, Transform>,
) -> Result<Vec<i64>, PlayerError> {
    let mut roots = Vec::new();
    let mut linked = BTreeSet::new();
    for (&id, transform) in transforms {
        local_id(path, transform.father)?;
        local_id(path, transform.game_object)?;
        if transform.father.is_null() {
            roots.push(id);
        }
        for &child in &transform.children {
            check_child(path, transforms, id, child)?;
            if !linked.insert(child.path_id) {
                return Err(invalid(
                    path,
                    format!("transform {} appears as a child twice", child.path_id),
                ));
            }
        }
    }
    for (&id, transform) in transforms {
        if !transform.father.is_null() && !linked.contains(&id) {
            return Err(invalid(
                path,
                format!("transform {id} has a parent but is absent from its children"),
            ));
        }
    }
    check_depth(path, transforms, &roots)?;
    Ok(roots)
}

fn check_child(
    path: &Path,
    transforms: &BTreeMap<i64, Transform>,
    parent: i64,
    child: PPtr,
) -> Result<(), PlayerError> {
    let id = local_id(path, child)?;
    let transform = transforms.get(&id).ok_or_else(|| {
        invalid(
            path,
            format!("transform {parent} references missing child {id}"),
        )
    })?;
    if transform.father
        != (PPtr {
            file_id: 0,
            path_id: parent,
        })
    {
        return Err(invalid(
            path,
            format!("child {id} does not name transform {parent} as its parent"),
        ));
    }
    Ok(())
}

fn check_depth(
    path: &Path,
    transforms: &BTreeMap<i64, Transform>,
    roots: &[i64],
) -> Result<(), PlayerError> {
    let mut stack: Vec<_> = roots.iter().map(|&id| (id, 0)).collect();
    let mut visited = BTreeSet::new();
    while let Some((id, depth)) = stack.pop() {
        if depth > MAX_DEPTH || !visited.insert(id) {
            return Err(invalid(
                path,
                format!("cycle or depth beyond {MAX_DEPTH} at transform {id}"),
            ));
        }
        stack.extend(
            transforms[&id]
                .children
                .iter()
                .map(|child| (child.path_id, depth + 1)),
        );
    }
    if visited.len() != transforms.len() {
        return Err(invalid(
            path,
            "a transform cycle is disconnected from every root",
        ));
    }
    Ok(())
}
