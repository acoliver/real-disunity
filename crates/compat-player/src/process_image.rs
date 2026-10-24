use crate::native_image;
use compat_kit::files;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

struct AdmittedImage {
    path: PathBuf,
    sha256: String,
    bytes: u64,
}

static PROCESS_IMAGE: Mutex<Option<AdmittedImage>> = Mutex::new(None);

pub(crate) fn admit(root: &Path, source: &Path, build: &Path) -> Result<PathBuf, String> {
    let mut selected = PROCESS_IMAGE.lock().map_err(files::io)?;
    admit_selected(&mut selected, root, source, build)
}

fn admit_selected(
    selected: &mut Option<AdmittedImage>,
    root: &Path,
    source: &Path,
    build: &Path,
) -> Result<PathBuf, String> {
    let (bytes, sha256) = native_image::source_identity(source)?;
    if selected.is_none() {
        let directory = root
            .join(".work/compat/native-images")
            .join(files::unique_id());
        let directory = files::work_path(root, &directory)?;
        fs::create_dir_all(&directory).map_err(files::io)?;
        let path = native_image::snapshot(source, &directory)?;
        if files::hash(&path)? != sha256 {
            return Err("native image changed before process admission; retry the run".into());
        }
        *selected = Some(AdmittedImage {
            path,
            sha256: sha256.clone(),
            bytes,
        });
    }
    let image = selected.as_ref().unwrap();
    if image.sha256 != sha256 {
        return Err(
            "native image changed after process admission; restart the worker to use the new build"
                .into(),
        );
    }
    files::work_path(root, &image.path)?;
    if native_image::source_identity(&image.path)? != (image.bytes, image.sha256.clone()) {
        return Err("admitted process native image changed".into());
    }
    files::write_json(
        &build.join("native-image.json"),
        &json!({"source":source,"image":image.path,"sha256":image.sha256,"bytes":bytes,
            "scope":"process","process_id":std::process::id()}),
    )?;
    Ok(image.path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_artifact_directories_share_one_owned_process_image() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".work")).unwrap();
        let source = root.path().join("canonical.so");
        fs::write(&source, b"native image").unwrap();
        let first = tempfile::tempdir_in(root.path().join(".work")).unwrap();
        let mut selected = None;
        let image = admit_selected(&mut selected, root.path(), &source, first.path()).unwrap();
        drop(first);
        let second = tempfile::tempdir_in(root.path().join(".work")).unwrap();
        assert_eq!(
            admit_selected(&mut selected, root.path(), &source, second.path()).unwrap(),
            image
        );
        let proof: serde_json::Value =
            files::read_json(&second.path().join("native-image.json")).unwrap();
        assert_eq!(proof["scope"], "process");
        assert!(image.is_file());
        fs::write(&source, b"rebuilt image").unwrap();
        assert!(
            admit_selected(&mut selected, root.path(), &source, second.path())
                .unwrap_err()
                .contains("restart the worker")
        );
        assert_eq!(fs::read(&image).unwrap(), b"native image");
    }
}
