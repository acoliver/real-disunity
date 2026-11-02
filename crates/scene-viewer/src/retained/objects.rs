//! Object edits change the host's object table in place. The materialized scene keeps its
//! previous object list until something needs the whole scene, so an edit costs time in the
//! size of the edit, not of the scene.
use super::RetainedHost;
use crate::{ObjectId, SceneObjectEdit, ViewerError};
use std::collections::BTreeMap;

impl RetainedHost {
    /// Applies `edits` in order, or none of them. On failure returns the index of the first
    /// rejected edit with its error, and the host is unchanged.
    pub(super) fn edit_objects(
        &mut self,
        edits: &[SceneObjectEdit],
    ) -> Result<(), (usize, ViewerError)> {
        // Whether each id touched by the batch exists after the edits checked so far.
        let mut present = BTreeMap::new();
        for (index, edit) in edits.iter().enumerate() {
            self.check_edit(&mut present, *edit)
                .map_err(|error| (index, error))?;
        }
        for edit in edits {
            match *edit {
                SceneObjectEdit::Add { id, object } | SceneObjectEdit::Replace { id, object } => {
                    self.objects.insert(id, object);
                }
                SceneObjectEdit::Remove(id) => {
                    self.objects.remove(&id);
                }
            }
        }
        self.objects_stale |= !edits.is_empty();
        Ok(())
    }

    fn check_edit(
        &self,
        present: &mut BTreeMap<ObjectId, bool>,
        edit: SceneObjectEdit,
    ) -> Result<(), ViewerError> {
        let (id, adds) = match edit {
            SceneObjectEdit::Add { id, object } => {
                self.validate_object(object)?;
                (id, true)
            }
            SceneObjectEdit::Replace { id, object } => {
                self.validate_object(object)?;
                (id, false)
            }
            SceneObjectEdit::Remove(id) => (id, false),
        };
        let exists = present
            .get(&id)
            .copied()
            .unwrap_or_else(|| self.objects.contains_key(&id));
        match (adds, exists) {
            (true, true) => Err(ViewerError::Driver("duplicate object ID".into())),
            (false, false) => Err(ViewerError::Driver("unknown object ID".into())),
            _ => {
                present.insert(id, !matches!(edit, SceneObjectEdit::Remove(_)));
                Ok(())
            }
        }
    }

    /// Brings the materialized scene's object list up to date with the object table. Call this
    /// before anything that reads or replaces the whole scene.
    pub(super) fn materialize_objects(&mut self) -> Result<(), ViewerError> {
        if !self.objects_stale {
            return Ok(());
        }
        self.scene = self
            .scene
            .with_objects_and_effects(
                self.objects.values().copied().collect(),
                self.scene.effects().cloned(),
            )
            .map_err(|error| ViewerError::Driver(error.to_string()))?;
        self.objects_stale = false;
        Ok(())
    }
}

#[cfg(test)]
#[path = "objects_tests.rs"]
mod tests;
