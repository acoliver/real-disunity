use compat_kit::files;
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_IMAGE_BYTES: u64 = 256 * 1024 * 1024;

pub(crate) fn source_identity(source: &Path) -> Result<(u64, String), String> {
    let metadata = fs::symlink_metadata(source).map_err(files::io)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_IMAGE_BYTES {
        return Err("native image must be a bounded, nonempty regular file".into());
    }
    Ok((metadata.len(), files::hash(source)?))
}

pub(crate) fn snapshot(source: &Path, build: &Path) -> Result<PathBuf, String> {
    let (bytes, before) = source_identity(source)?;
    let directory = build.join(format!("native-{}", files::unique_id()));
    fs::create_dir(&directory).map_err(files::io)?;
    let image = directory.join(source.file_name().ok_or("native image has no file name")?);
    copy_bounded(source, &image, bytes)?;
    let copied = files::hash(&image)?;
    if before != copied || before != files::hash(source)? {
        return Err("canonical native image changed during admission; retry the run".into());
    }
    files::write_json(
        &build.join("native-image.json"),
        &json!({"source":source,"image":image,"sha256":copied,"bytes":bytes}),
    )?;
    Ok(image)
}

fn copy_bounded(source: &Path, image: &Path, expected: u64) -> Result<(), String> {
    let mut input = fs::File::open(source)
        .map_err(files::io)?
        .take(MAX_IMAGE_BYTES + 1);
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(image)
        .map_err(files::io)?;
    let count = std::io::copy(&mut input, &mut output).map_err(files::io)?;
    if count != expected || count > MAX_IMAGE_BYTES {
        return Err("native image size changed during admission".into());
    }
    output.flush().map_err(files::io)?;
    output.sync_all().map_err(files::io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaded_image_does_not_share_a_rebuildable_output_file() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("canonical.so");
        let build = directory.path().join("build");
        fs::create_dir(&build).unwrap();
        fs::write(&source, b"first native image").unwrap();
        let loaded = snapshot(&source, &build).unwrap();
        fs::write(&source, b"second rebuilt native image").unwrap();
        assert_eq!(fs::read(&loaded).unwrap(), b"first native image");
        assert!(loaded.starts_with(&build));
        let manifest: serde_json::Value =
            files::read_json(&build.join("native-image.json")).unwrap();
        assert_eq!(manifest["sha256"], files::hash(&loaded).unwrap());
        assert_eq!(manifest["image"], loaded.to_string_lossy().as_ref());
    }

    #[test]
    fn empty_oversized_and_symlink_images_are_rejected_before_loading() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("canonical.so");
        fs::write(&source, []).unwrap();
        assert!(snapshot(&source, directory.path()).is_err());
        fs::OpenOptions::new()
            .write(true)
            .open(&source)
            .unwrap()
            .set_len(MAX_IMAGE_BYTES + 1)
            .unwrap();
        assert!(snapshot(&source, directory.path()).is_err());
        #[cfg(unix)]
        {
            let alias = directory.path().join("alias.so");
            std::os::unix::fs::symlink(&source, &alias).unwrap();
            assert!(snapshot(&alias, directory.path()).is_err());
        }
    }
}
