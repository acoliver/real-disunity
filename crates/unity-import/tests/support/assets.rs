//! Project assets with `.meta` files, for tests that need them.

use crate::fixture::Fixture;

impl Fixture {
    /// Writes `contents` at `path` and a `.meta` file naming its GUID and importer.
    pub fn asset(&self, path: &str, guid: &str, importer: &str, contents: impl AsRef<[u8]>) {
        self.file(path, contents);
        self.file(
            &format!("{path}.meta"),
            format!("fileFormatVersion: 2\nguid: {guid}\n{importer}:\n  userData: \n"),
        );
    }
}
