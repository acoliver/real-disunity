//! Test-only accessors of this module's types.

use super::*;

impl Programs {
    /// How many programs have been built, one for each key a feature has drawn with.
    pub(crate) fn built(&self) -> usize {
        self.by_key.len()
    }

    /// Draws bodies of `key` with `source` compiled for `options` from now on, in place of the
    /// composed text: the substitution the equation tests make.
    pub(crate) fn substitute(&mut self, key: Key, source: &str, options: [u32; 5]) {
        self.by_key.remove(&key);
        self.sources.insert(key, (source.to_owned(), options));
    }
}
