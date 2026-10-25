//! Test setup shared by the player's unit and integration tests: the workspace, freshly built
//! native and managed artifacts, owned scratch directories and fixture requests.

#[path = "../../disunity-runtime/tests/support/handshake.rs"]
pub mod artifacts;

use compat_kit::{RunRequest, RunTarget};
use std::path::{Path, PathBuf};

// The including crate root brings `Installation` into scope.
use super::Installation;

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// The workspace with the native runtime library built for these tests.
pub fn installation() -> Installation {
    Installation::new(root(), artifacts::artifacts().native.clone())
}

/// A fresh run-output directory inside the workspace scratch area, where workers write.
pub fn scratch(prefix: &str) -> tempfile::TempDir {
    let parent = root().join(".work/tmp");
    std::fs::create_dir_all(&parent).expect("scratch parent");
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(parent)
        .expect("scratch directory")
}

/// The request the harness sends for `fixture`, with the pinned original checkout as source.
pub fn fixture_request(fixture: &str, artifacts: &Path, desktop: bool) -> RunRequest {
    let fixture = compat_kit::fixture::load(&root(), fixture).expect("fixture");
    RunRequest::for_fixture(
        &fixture,
        RunTarget {
            run_id: format!("{}-test", fixture.name),
            profile: fixture.default_profile.clone(),
            desktop,
            source_root: compat_kit::fixture::source_root(&fixture).expect("original checkout"),
            artifact_root: artifacts.into(),
        },
    )
}
