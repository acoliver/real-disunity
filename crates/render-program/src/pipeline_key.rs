//! Content keys for the pipeline cache.

/// A hash of everything a pipeline is built from, so equal inputs share one pipeline.
///
/// What a draw adds to its program is hashed through its `Hash` impl; the program's own layout,
/// which is hashed once, is hashed through its `Debug` text, which names every field.
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

    pub(crate) fn value(&mut self, value: &impl std::hash::Hash) {
        value.hash(&mut Feed(&mut self.0));
    }

    pub(crate) fn debug(&mut self, value: &impl std::fmt::Debug) {
        use std::fmt::Write;
        write!(Feed(&mut self.0), "{value:?}").expect("hashing text cannot fail");
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

/// Feeds what is written to it into a content hasher, so a descriptor is hashed as it is
/// produced and never held as a string.
struct Feed<'a>(&'a mut blake3::Hasher);

impl std::hash::Hasher for Feed<'_> {
    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    fn finish(&self) -> u64 {
        let hash = self.0.finalize();
        u64::from_le_bytes(
            hash.as_bytes()[..8]
                .try_into()
                .expect("a hash has 32 bytes"),
        )
    }
}

impl std::fmt::Write for Feed<'_> {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.0.update(text.as_bytes());
        Ok(())
    }
}
