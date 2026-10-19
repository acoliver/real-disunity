use super::*;
use crate::{metadata, scratch::Directory};

const FILE: &str = "crates/model/src/lib.rs";

#[test]
fn load_normalizes_source_paths_before_deriving_policy_names() {
    let dir = Directory::new("source-paths").unwrap();
    dir.write(FILE, "pub struct Settings;").unwrap();
    let root = metadata::canonical(dir.path()).unwrap();
    let path = dir.path().join("crates/model/src/../src/lib.rs");
    let files = load(&root, vec![path]).unwrap();
    assert_eq!(files[0].relative, FILE);
}

#[test]
fn load_normalizes_the_root_as_well_as_sources() {
    let dir = Directory::new("source-root").unwrap();
    dir.write(FILE, "pub struct Settings;").unwrap();
    let root = dir.path().join("crates/..");
    let path = metadata::canonical(&dir.path().join(FILE)).unwrap();
    let files = load(&root, vec![path]).unwrap();
    assert_eq!(files[0].relative, FILE);
}

#[test]
fn load_rejects_paths_that_resolve_outside_the_workspace() {
    let dir = Directory::new("source-containment").unwrap();
    dir.write("inside/lib.rs", "pub struct Inside;").unwrap();
    dir.write("outside.rs", "pub struct Outside;").unwrap();
    let root = dir.path().join("inside");
    let path = root.join("../outside.rs");
    let error = load(&root, vec![path]).err().unwrap();
    assert!(error.contains("outside workspace"), "{error}");
}

#[cfg(unix)]
#[test]
fn canonical_containment_rejects_symlink_escapes() {
    let dir = Directory::new("source-symlink").unwrap();
    dir.write("inside/lib.rs", "pub struct Inside;").unwrap();
    dir.write("outside.rs", "pub struct Outside;").unwrap();
    let root = dir.path().join("inside");
    std::os::unix::fs::symlink(dir.path().join("outside.rs"), root.join("link.rs")).unwrap();
    assert!(
        contained(&root, "link.rs")
            .unwrap_err()
            .contains("escapes workspace")
    );
    let error = load(&root, vec![root.join("link.rs")]).err().unwrap();
    assert!(error.contains("outside workspace"), "{error}");
    assert!(
        rust_files(&root, &mut Vec::new())
            .unwrap_err()
            .contains("symlink")
    );
}

#[cfg(windows)]
#[test]
fn load_accepts_ordinary_windows_sources_under_a_verbatim_root() {
    let dir = Directory::new("source-windows").unwrap();
    dir.write(FILE, "pub struct Settings;").unwrap();
    let root = metadata::canonical(dir.path()).unwrap();
    let canonical = metadata::canonical(&dir.path().join(FILE)).unwrap();
    let text = canonical.to_str().unwrap();
    let ordinary = text.strip_prefix(r"\\?\").unwrap();
    let ordinary = ordinary.strip_prefix(r"UNC\").map_or_else(
        || PathBuf::from(ordinary),
        |unc| PathBuf::from(format!(r"\\{unc}")),
    );
    assert!(ordinary.is_absolute());
    assert!(ordinary.strip_prefix(&root).is_err());
    let files = load(&root, vec![ordinary]).unwrap();
    assert_eq!(files[0].relative, FILE);
}
