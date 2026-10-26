//! Textures and the sprites cut from them.
//!
//! A texture asset is an image file plus the `TextureImporter` settings in its `.meta` file.
//! Importing one decodes the image to RGBA8 and lists its sprites with the fileIDs other files
//! use to reference them.

mod image;
mod package;
mod physics;
mod settings;
mod sprites;

use std::{
    collections::{BTreeMap, btree_map::Entry},
    path::{Path, PathBuf},
};

use crate::{
    guid::Guid,
    project::{ImportError, Project},
    yaml::Node,
};

pub use image::Image;
pub use settings::{Filter, SpriteMode, TextureSettings, Wrap};
pub use sprites::Sprite;

/// The fileID of the texture object inside a texture asset.
pub const TEXTURE_FILE_ID: i64 = 2_800_000;
/// The fileID of the sprite of a texture imported in single sprite mode.
pub const SINGLE_SPRITE_FILE_ID: i64 = 21_300_000;

#[derive(Debug, thiserror::Error)]
pub enum TextureError {
    #[error(transparent)]
    Import(#[from] ImportError),
    #[error("{path}: {message}")]
    Invalid { path: PathBuf, message: String },
    #[error("texture asset {0} is not in the project")]
    Missing(Guid),
    #[error("{path} has no sprite with fileID {file_id}")]
    NoSprite { path: PathBuf, file_id: i64 },
}

impl TextureError {
    fn invalid(path: &Path, message: impl Into<String>) -> Self {
        Self::Invalid {
            path: path.to_owned(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    path: PathBuf,
    settings: TextureSettings,
    image: Image,
    sprites: Vec<Sprite>,
}

/// Imports the texture asset `guid`: its settings, decoded image and sprites.
/// A texture the project does not hold may be a known package texture.
pub fn import_texture(project: &Project, guid: Guid) -> Result<Texture, TextureError> {
    let Some(asset) = project.asset(guid) else {
        return Texture::package(guid).ok_or(TextureError::Missing(guid));
    };
    let path = asset.path.clone();
    let meta = project.read_plain(&path.with_extension(meta_extension(&path)))?;
    let importer = meta
        .get("TextureImporter")
        .and_then(Node::as_mapping)
        .ok_or_else(|| TextureError::invalid(&path, "asset is not imported as a texture"))?;
    let settings =
        TextureSettings::read(importer).map_err(|message| TextureError::invalid(&path, message))?;
    let image = image::decode(&project.read_bytes(&path)?)
        .map_err(|message| TextureError::invalid(&path, message))?;
    settings
        .check_size(image.width, image.height)
        .map_err(|message| TextureError::invalid(&path, message))?;
    let sprites = sprites::read(&path, importer, &settings, &image)
        .map_err(|message| TextureError::invalid(&path, message))?;
    Ok(Texture {
        path,
        settings,
        image,
        sprites,
    })
}

/// `Ball.png` has its settings in `Ball.png.meta`.
fn meta_extension(path: &Path) -> String {
    path.extension().map_or_else(
        || "meta".to_owned(),
        |ext| format!("{}.meta", ext.to_string_lossy()),
    )
}

impl Texture {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn settings(&self) -> &TextureSettings {
        &self.settings
    }

    pub fn image(&self) -> &Image {
        &self.image
    }

    pub fn sprites(&self) -> &[Sprite] {
        &self.sprites
    }

    /// The sprite a `{fileID, guid}` reference into this texture names.
    pub fn sprite(&self, file_id: i64) -> Option<&Sprite> {
        self.sprites.iter().find(|sprite| sprite.file_id == file_id)
    }
}

/// Textures imported once each, keyed by asset GUID.
#[derive(Debug, Default)]
pub struct Textures {
    loaded: BTreeMap<Guid, Texture>,
}

impl Textures {
    pub fn get(&mut self, project: &Project, guid: Guid) -> Result<&Texture, TextureError> {
        match self.loaded.entry(guid) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => Ok(entry.insert(import_texture(project, guid)?)),
        }
    }

    /// The texture and sprite a `{fileID, guid}` sprite reference names.
    pub fn sprite(
        &mut self,
        project: &Project,
        guid: Guid,
        file_id: i64,
    ) -> Result<(&Texture, &Sprite), TextureError> {
        let texture = self.get(project, guid)?;
        let sprite = texture
            .sprite(file_id)
            .ok_or_else(|| TextureError::NoSprite {
                path: texture.path.clone(),
                file_id,
            })?;
        Ok((texture, sprite))
    }

    pub fn iter(&self) -> impl Iterator<Item = (Guid, &Texture)> {
        self.loaded.iter().map(|(guid, texture)| (*guid, texture))
    }
}
