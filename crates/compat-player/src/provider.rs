use std::path::{Path, PathBuf};

pub(crate) fn select(root: &Path, configured: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(configured) = configured {
        if !configured.is_absolute() {
            return Err("COMPAT_NATIVE_LIBRARY must be an absolute path beneath .work".into());
        }
        let selected = compat_kit::files::work_path(root, configured)?;
        crate::native_image::source_identity(&selected)?;
        return Ok(selected);
    }
    let name = if cfg!(target_os = "windows") {
        "worldrunner_disunity_ffi.dll"
    } else if cfg!(target_os = "macos") {
        "libworldrunner_disunity_ffi.dylib"
    } else {
        "libworldrunner_disunity_ffi.so"
    };
    Ok(root.join(".work/target/debug").join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_native_selection_is_contained_regular_and_does_not_change_default() {
        let root = tempfile::tempdir().unwrap();
        let work = root.path().join(".work");
        std::fs::create_dir(&work).unwrap();
        let candidate = work.join("candidate.so");
        std::fs::write(&candidate, b"candidate bytes").unwrap();
        assert_eq!(select(root.path(), Some(&candidate)).unwrap(), candidate);
        assert!(
            select(root.path(), None)
                .unwrap()
                .starts_with(work.join("target/debug"))
        );
        assert!(select(root.path(), Some(Path::new(".work/candidate.so"))).is_err());
        assert!(select(root.path(), Some(root.path())).is_err());
        assert!(select(root.path(), Some(&work)).is_err());
        let absent = work.join("missing.so");
        assert!(select(root.path(), Some(&absent)).is_err());
        std::fs::write(&candidate, []).unwrap();
        assert!(select(root.path(), Some(&candidate)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn native_override_rejects_symlink_and_outside_worktree() {
        let root = tempfile::tempdir().unwrap();
        let work = root.path().join(".work");
        std::fs::create_dir(&work).unwrap();
        let outside = root.path().join("outside.so");
        std::fs::write(&outside, b"bytes").unwrap();
        assert!(select(root.path(), Some(&outside)).is_err());
        let alias = work.join("alias.so");
        std::os::unix::fs::symlink(&outside, &alias).unwrap();
        assert!(select(root.path(), Some(&alias)).is_err());
    }
}
