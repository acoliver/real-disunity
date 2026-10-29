//! What a built player reports through `Application`: its names and version, the Unity
//! version that built it, and the platform it was built for.

use std::path::Path;

use serde::Serialize;
use worldrunner_unity_serialized::{
    ClassId,
    managers::{BuildSettings, QualitySettings},
    player_settings::{LAYOUT_RELEASE, read_player_identity},
};

use crate::{PlayerError, player_objects::OpenFile};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerIdentity {
    pub company_name: String,
    pub product_name: String,
    /// The project's bundle version; empty when the player's Unity release stores
    /// `PlayerSettings` in a layout this importer does not read.
    pub version: String,
    pub unity_version: String,
    /// Unity's `RuntimePlatform` name, such as `WindowsPlayer`.
    pub platform: &'static str,
}

/// Reads the identity of the player whose data directory is `data_dir`.
///
/// # Errors
/// Fails when `globalgamemanagers` cannot be read, its build or player settings do not
/// decode, or the decoded names differ from those in `app.info`.
pub fn player_identity(data_dir: &Path) -> Result<PlayerIdentity, PlayerError> {
    let mut managers = OpenFile::open(&data_dir.join("globalgamemanagers"))?;
    let build = managers
        .file
        .objects()
        .iter()
        .find(|object| object.class_id == ClassId::BUILD_SETTINGS)
        .map(|object| object.path_id);
    let unity_version = match build {
        Some(path_id) => managers.decode::<BuildSettings>(path_id)?.version,
        None => String::new(),
    };
    let settings = managers
        .file
        .objects()
        .iter()
        .find(|object| object.class_id == ClassId::PLAYER_SETTINGS)
        .copied();
    let (mut company_name, mut product_name) = app_info(data_dir);
    let mut version = String::new();
    if let Some(object) = settings.filter(|_| unity_version.starts_with(LAYOUT_RELEASE)) {
        let bytes = managers.bytes(&object)?;
        let identity = read_player_identity(&bytes, managers.file.header.big_endian)
            .map_err(|source| managers.error(source))?;
        let decoded = (identity.company_name, identity.product_name);
        let has_app_info = !company_name.is_empty() || !product_name.is_empty();
        if has_app_info && decoded != (company_name.clone(), product_name.clone()) {
            return Err(PlayerError::IdentityMismatch {
                path: data_dir.join("globalgamemanagers"),
                decoded,
                app_info: (company_name, product_name),
            });
        }
        (company_name, product_name) = decoded;
        version = identity.version;
    }
    Ok(PlayerIdentity {
        company_name,
        product_name,
        version,
        unity_version,
        platform: platform(data_dir),
    })
}

/// The quality levels a player defines and the one it starts on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct QualityLevels {
    /// Level names in level order.
    pub names: Vec<String>,
    /// The index of the level the player was built to start on.
    pub current: i32,
}

/// Reads the quality levels of the player whose data directory is `data_dir`; `None` when
/// `globalgamemanagers` has no `QualitySettings` object.
///
/// # Errors
/// Fails when `globalgamemanagers` cannot be read or its quality settings do not decode.
pub fn player_quality(data_dir: &Path) -> Result<Option<QualityLevels>, PlayerError> {
    let mut managers = OpenFile::open(&data_dir.join("globalgamemanagers"))?;
    let Some(path_id) = managers
        .file
        .objects()
        .iter()
        .find(|object| object.class_id == ClassId::QUALITY_SETTINGS)
        .map(|object| object.path_id)
    else {
        return Ok(None);
    };
    let settings = managers.decode::<QualitySettings>(path_id)?;
    Ok(Some(QualityLevels {
        names: settings.names,
        current: settings.current,
    }))
}

/// The scene paths in the build of the player whose data directory is `data_dir`, in build
/// order: scene `i` is stored in the file `level{i}`. Empty when `globalgamemanagers` has no
/// `BuildSettings` object.
///
/// # Errors
/// Fails when `globalgamemanagers` cannot be read or its build settings do not decode.
pub fn player_scene_paths(data_dir: &Path) -> Result<Vec<String>, PlayerError> {
    let mut managers = OpenFile::open(&data_dir.join("globalgamemanagers"))?;
    let Some(path_id) = managers
        .file
        .objects()
        .iter()
        .find(|object| object.class_id == ClassId::BUILD_SETTINGS)
        .map(|object| object.path_id)
    else {
        return Ok(Vec::new());
    };
    Ok(managers.decode::<BuildSettings>(path_id)?.scenes)
}

/// The company and product names from `app.info`, which every player writes as two lines.
fn app_info(data_dir: &Path) -> (String, String) {
    let text = std::fs::read_to_string(data_dir.join("app.info")).unwrap_or_default();
    let mut lines = text.lines().map(str::to_owned);
    (
        lines.next().unwrap_or_default(),
        lines.next().unwrap_or_default(),
    )
}

/// The platform a player was built for: Windows when `UnityPlayer.dll` sits beside the data
/// directory, macOS when the data directory is an app bundle's `Contents/Resources/Data`, and
/// Linux otherwise.
fn platform(data_dir: &Path) -> &'static str {
    let parent = data_dir.parent().unwrap_or(data_dir);
    let in_bundle = data_dir
        .ancestors()
        .any(|ancestor| ancestor.extension().is_some_and(|ext| ext == "app"));
    if parent.join("UnityPlayer.dll").exists() {
        "WindowsPlayer"
    } else if in_bundle {
        "OSXPlayer"
    } else {
        "LinuxPlayer"
    }
}
