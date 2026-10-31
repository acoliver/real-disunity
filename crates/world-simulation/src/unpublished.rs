//! The open constructions of a world and the objects each has created.
//!
//! Every activity check asks whether an object is still unpublished, so the objects of all
//! open constructions are also kept in one set; the question does not walk every batch.

use std::collections::{BTreeMap, HashSet};

use crate::ObjectHandle;

/// Open constructions by mutation sequence, each with the objects it created in creation
/// order, and the set of all those objects.
#[derive(Clone, Debug, Default)]
pub(crate) struct Unpublished {
    batches: BTreeMap<usize, Vec<ObjectHandle>>,
    objects: HashSet<ObjectHandle>,
}

impl Unpublished {
    /// Opens an empty construction under `sequence`.
    pub fn open(&mut self, sequence: usize) {
        if let Some(previous) = self.batches.insert(sequence, Vec::new()) {
            self.forget(&previous);
        }
    }

    /// Adds `object` to the open construction `sequence`. Returns false when it is not open.
    pub fn push(&mut self, sequence: usize, object: ObjectHandle) -> bool {
        let Some(nodes) = self.batches.get_mut(&sequence) else {
            return false;
        };
        nodes.push(object);
        self.objects.insert(object);
        true
    }

    /// Closes the construction `sequence` and returns its objects.
    pub fn remove(&mut self, sequence: usize) -> Option<Vec<ObjectHandle>> {
        let nodes = self.batches.remove(&sequence)?;
        self.forget(&nodes);
        Some(nodes)
    }

    /// The objects of the open construction `sequence`.
    pub fn get(&self, sequence: usize) -> Option<&[ObjectHandle]> {
        self.batches.get(&sequence).map(Vec::as_slice)
    }

    pub fn contains_key(&self, sequence: usize) -> bool {
        self.batches.contains_key(&sequence)
    }

    /// Whether an open construction created `object`.
    pub fn contains_object(&self, object: ObjectHandle) -> bool {
        self.objects.contains(&object)
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.batches.is_empty()
    }

    fn forget(&mut self, nodes: &[ObjectHandle]) {
        for object in nodes {
            self.objects.remove(object);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Unpublished;
    use crate::{ObjectDesc, World, WorldSettings};

    #[test]
    fn objects_are_unpublished_while_their_construction_is_open() {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let [a, b, c] = [(); 3].map(|()| world.create_object(ObjectDesc::default()).unwrap());
        let mut open = Unpublished::default();
        open.open(1);
        open.open(2);
        assert!(open.push(1, a) && open.push(1, b) && open.push(2, c));
        assert!(
            !open.push(3, a),
            "a construction that is not open takes nothing"
        );
        assert!(open.contains_object(a) && open.contains_object(c));
        assert_eq!(open.get(1), Some(&[a, b][..]));
        assert_eq!(open.remove(1), Some(vec![a, b]));
        assert!(!open.contains_object(a) && !open.contains_object(b));
        assert!(open.contains_object(c) && open.contains_key(2) && !open.contains_key(1));
        assert_eq!(open.remove(1), None);
        open.open(2);
        assert!(
            !open.contains_object(c),
            "reopening a construction forgets its objects"
        );
        assert_eq!(open.remove(2), Some(Vec::new()));
        assert!(open.is_empty());
    }
}
