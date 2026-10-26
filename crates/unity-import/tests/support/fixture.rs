//! A temporary Unity project for tests.

use std::fs;

use worldrunner_unity_import::Project;

pub struct Fixture(pub tempfile::TempDir);

impl Fixture {
    pub fn new(editor_version: &str) -> Self {
        let fixture = Self(tempfile::tempdir().unwrap());
        fixture.file(
            "ProjectSettings/ProjectVersion.txt",
            format!("m_EditorVersion: {editor_version}\n"),
        );
        fixture
    }

    pub fn file(&self, path: &str, contents: impl AsRef<[u8]>) {
        let path = self.0.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    /// Writes `contents` at `path` and a `.meta` file naming its GUID and importer.
    pub fn asset(&self, path: &str, guid: &str, importer: &str, contents: impl AsRef<[u8]>) {
        self.file(path, contents);
        self.file(
            &format!("{path}.meta"),
            format!("fileFormatVersion: 2\nguid: {guid}\n{importer}:\n  userData: \n"),
        );
    }

    pub fn project(&self) -> Project {
        Project::open(self.0.path()).unwrap()
    }
}
