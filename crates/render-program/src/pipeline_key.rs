//! Content keys for the pipeline cache.

/// A hash of everything a pipeline is built from, so equal inputs share one pipeline.
///
/// Plain-data descriptors are hashed through their `Debug` text, which names every field.
pub(crate) struct Key(blake3::Hasher);

impl Key {
    pub(crate) fn new(kind: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(kind.as_bytes());
        Self(hasher)
    }

    pub(crate) fn hash(&mut self, hash: blake3::Hash) {
        self.0.update(hash.as_bytes());
    }

    pub(crate) fn debug(&mut self, value: &impl std::fmt::Debug) {
        self.0.update(format!("{value:?}").as_bytes());
    }

    /// Adds the identity of a wgpu object that compares by identity, such as a bind group layout:
    /// two handles to one object add the same bytes and two objects with equal descriptors add
    /// different ones.
    pub(crate) fn identity(&mut self, object: &impl std::hash::Hash) {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        object.hash(&mut hasher);
        self.0.update(&hasher.finish().to_le_bytes());
    }

    pub(crate) fn finish(&self) -> blake3::Hash {
        self.0.finalize()
    }
}
