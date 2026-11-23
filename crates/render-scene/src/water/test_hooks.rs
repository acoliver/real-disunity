//! Test-only accessors of this module's types.

use super::*;

impl Water {
    /// Draws bodies of `key` with `source` compiled for `options` in place of the composed text.
    pub(crate) fn substitute(&self, key: ([u32; 5], bool), source: &str, options: [u32; 5]) {
        self.programs.borrow_mut().substitute(key, source, options);
    }
}
