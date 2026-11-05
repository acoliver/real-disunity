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

    pub(crate) fn finish(&self) -> blake3::Hash {
        self.0.finalize()
    }
}
