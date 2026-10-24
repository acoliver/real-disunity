use sha2::Digest;
use std::{fs, path::Path};
use worldrunner_compat_content::{ImportProfile, script_census};

fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("Assets/Scripts/Nested")).unwrap();
    fs::write(
        root.path().join("Assets/Scripts/Main.cs"),
        "class Main {}\n",
    )
    .unwrap();
    fs::write(
        root.path().join("Assets/Scripts/Nested/Helper.cs"),
        "class Helper {}\n",
    )
    .unwrap();
    root
}

#[test]
fn regular_sources_are_sorted_and_hashed() {
    let root = project();
    let census = script_census(
        root.path(),
        Path::new("Assets/Scripts"),
        &ImportProfile::default(),
    )
    .unwrap();
    assert_eq!(census.len(), 2);
    assert_eq!(census[0].path, "Assets/Scripts/Main.cs");
    assert_eq!(
        census[0].sha256,
        format!("{:x}", sha2::Sha256::digest(b"class Main {}\n"))
    );
    assert_eq!(census[0].byte_length, 14);
    assert_eq!(census[1].path, "Assets/Scripts/Nested/Helper.cs");
    assert_eq!(
        census[1].sha256,
        format!("{:x}", sha2::Sha256::digest(b"class Helper {}\n"))
    );
    assert_eq!(census[1].byte_length, 16);
}

#[cfg(unix)]
#[test]
fn every_symlink_in_selected_tree_is_rejected_before_following_it() {
    use std::os::unix::fs::symlink;
    let root = project();
    fs::write(root.path().join("outside.cs"), "class Outside {}\n").unwrap();
    symlink(
        root.path().join("outside.cs"),
        root.path().join("Assets/Scripts/Alias.cs"),
    )
    .unwrap();
    let error = script_census(
        root.path(),
        Path::new("Assets/Scripts"),
        &ImportProfile::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("symlink"), "{error}");
}

#[cfg(unix)]
#[test]
fn dangling_directory_and_cycle_symlinks_are_rejected() {
    use std::os::unix::fs::symlink;
    for target in ["missing.cs", "Nested", "Alias"] {
        let root = project();
        symlink(target, root.path().join("Assets/Scripts/Alias")).unwrap();
        let error = script_census(
            root.path(),
            Path::new("Assets/Scripts"),
            &ImportProfile::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("symlink"), "{target}: {error}");
    }
}

#[cfg(unix)]
#[test]
fn symlink_target_outside_project_is_rejected() {
    use std::os::unix::fs::symlink;
    let root = project();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("External.cs");
    fs::write(&target, "class External {}\n").unwrap();
    symlink(target, root.path().join("Assets/Scripts/External.cs")).unwrap();
    let error = script_census(
        root.path(),
        Path::new("Assets/Scripts"),
        &ImportProfile::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("symlink"), "{error}");
}

#[cfg(unix)]
#[test]
fn symlinked_selected_directory_ancestor_is_rejected() {
    use std::os::unix::fs::symlink;
    let root = project();
    fs::rename(root.path().join("Assets"), root.path().join("RealAssets")).unwrap();
    symlink(root.path().join("RealAssets"), root.path().join("Assets")).unwrap();
    let error = script_census(
        root.path(),
        Path::new("Assets/Scripts"),
        &ImportProfile::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("symlink"), "{error}");
}
