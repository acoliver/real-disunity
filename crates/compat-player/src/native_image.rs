//! Selection of the native runtime library loaded by this process.
use compat_kit::files;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

const MAX_IMAGE_BYTES: u64 = 256 * 1024 * 1024;

/// The native runtime library that Cargo places next to this executable.
pub(crate) fn beside_executable() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(files::io)?;
    let directory = executable
        .parent()
        .ok_or("executable has no parent directory")?;
    // Test executables live in `target/<profile>/deps`; the library is built one level up.
    let directory = if directory.ends_with("deps") {
        directory.parent().unwrap_or(directory)
    } else {
        directory
    };
    Ok(directory.join(format!(
        "{}worldrunner_disunity_ffi{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    )))
}

/// A private copy of `source` for this process. The CLR and native host stay loaded for the
/// process lifetime, so a later `cargo build` must not replace the file under the loaded image.
/// Every run in the process receives the same copy.
pub(crate) fn process_copy(root: &Path, source: &Path) -> Result<PathBuf, String> {
    static COPY: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    COPY.get_or_init(|| copy_into(source, &root.join(".work/compat/native-images")))
        .clone()
}

fn copy_into(source: &Path, parent: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        format!(
            "native runtime library {} is unavailable ({error}); build worldrunner-disunity-ffi",
            source.display()
        )
    })?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_IMAGE_BYTES {
        return Err("native image must be a bounded, nonempty regular file".into());
    }
    let directory = parent.join(files::unique_id());
    fs::create_dir_all(&directory).map_err(files::io)?;
    let image = directory.join(source.file_name().ok_or("native image has no file name")?);
    fs::copy(source, &image).map_err(files::io)?;
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaded_image_does_not_share_a_rebuildable_output_file() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("canonical.so");
        fs::write(&source, b"first native image").unwrap();
        let loaded = copy_into(&source, &directory.path().join("images")).unwrap();
        fs::write(&source, b"second rebuilt native image").unwrap();
        assert_eq!(fs::read(&loaded).unwrap(), b"first native image");
        assert!(loaded.starts_with(directory.path().join("images")));
    }

    #[test]
    fn missing_empty_oversized_and_symlink_images_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("images");
        let source = directory.path().join("canonical.so");
        assert!(copy_into(&source, &parent).unwrap_err().contains("build"));
        fs::write(&source, []).unwrap();
        assert!(copy_into(&source, &parent).is_err());
        fs::OpenOptions::new()
            .write(true)
            .open(&source)
            .unwrap()
            .set_len(MAX_IMAGE_BYTES + 1)
            .unwrap();
        assert!(copy_into(&source, &parent).is_err());
        #[cfg(unix)]
        {
            fs::write(&source, b"image").unwrap();
            let alias = directory.path().join("alias.so");
            std::os::unix::fs::symlink(&source, &alias).unwrap();
            assert!(copy_into(&alias, &parent).is_err());
        }
    }

    #[test]
    fn library_is_resolved_next_to_the_cargo_profile_directory() {
        let library = beside_executable().unwrap();
        let directory = library.parent().unwrap();
        assert!(!directory.ends_with("deps"), "{}", library.display());
    }
}
