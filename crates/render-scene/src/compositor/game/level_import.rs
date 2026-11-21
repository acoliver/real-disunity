//! One level of one layer of a declared texture or target, imported by handle as a 2D view of
//! exactly that level, so a game binds a layer of an array (sampled, or as a storage image a
//! kernel writes) without making a view of its own.
use super::GameTargets;
use super::resources::{GameLevel, GameResourceError};
use worldrunner_render_post::contracts::TextureRange;
use worldrunner_render_program::binding::EngineSemantic;
use worldrunner_render_program::runtime::Import;

/// How the binding that takes an imported level reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelAccess {
    /// Sampled or loaded as a `texture_2d`: the texture needs `TEXTURE_BINDING`.
    Sampled,
    /// As a `texture_storage_2d` a shader writes or reads: the texture needs `STORAGE_BINDING`.
    Storage,
}

impl LevelAccess {
    fn usage(self) -> wgpu::TextureUsages {
        match self {
            Self::Sampled => wgpu::TextureUsages::TEXTURE_BINDING,
            Self::Storage => wgpu::TextureUsages::STORAGE_BINDING,
        }
    }
}

impl GameTargets {
    /// Level `level.mip` of layer `level.layer` of the target `name`, for the binding sourced
    /// from `semantic`, as [`GameResources::import_texture_level`](super::GameResources::import_texture_level)
    /// imports a level of a declared texture. A storage binding needs a target declared with
    /// `STORAGE_BINDING`.
    ///
    /// # Errors
    /// [`GameResourceError::Import`], naming the target, for an undeclared target, a mip or
    /// layer outside it, or a target without the usage `access` needs.
    pub fn import_level(
        &self,
        name: &'static str,
        semantic: EngineSemantic,
        level: GameLevel,
        access: LevelAccess,
    ) -> Result<Import, GameResourceError> {
        let texture = self
            .texture(name)
            .map_err(|error| GameResourceError::Import {
                label: name.to_owned(),
                reason: error.to_string(),
            })?;
        import_level(name, &texture, semantic, level, access)
    }
}

/// `level` of `texture`, named `label` in refusals, as a 2D view of that one mip of that one
/// layer, for the binding sourced from `semantic`. The import states the level as its range, so
/// a dispatch may bind other levels of the texture beside it and writes only that level.
pub(super) fn import_level(
    label: &str,
    texture: &wgpu::Texture,
    semantic: EngineSemantic,
    level: GameLevel,
    access: LevelAccess,
) -> Result<Import, GameResourceError> {
    let refuse = |reason: String| {
        Err(GameResourceError::Import {
            label: label.to_owned(),
            reason,
        })
    };
    if texture.dimension() != wgpu::TextureDimension::D2 {
        return refuse(format!(
            "a {:?} texture has no 2D view of one layer",
            texture.dimension()
        ));
    }
    if level.mip >= texture.mip_level_count() {
        return refuse(format!(
            "mip {} is outside the {} mips of the texture",
            level.mip,
            texture.mip_level_count()
        ));
    }
    if level.layer >= texture.depth_or_array_layers() {
        return refuse(format!(
            "layer {} is outside the {} layers of the texture",
            level.layer,
            texture.depth_or_array_layers()
        ));
    }
    let needed = access.usage();
    if !texture.usage().contains(needed) {
        return refuse(format!(
            "a {access:?} binding needs {needed:?}, which the texture was not declared with"
        ));
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some(label),
        dimension: Some(wgpu::TextureViewDimension::D2),
        base_mip_level: level.mip,
        mip_level_count: Some(1),
        base_array_layer: level.layer,
        array_layer_count: Some(1),
        ..wgpu::TextureViewDescriptor::default()
    });
    Ok(Import::texture_range(
        semantic,
        texture.clone(),
        view,
        wgpu::TextureViewDimension::D2,
        TextureRange::single(level.mip, level.layer, wgpu::TextureAspect::All),
    ))
}

#[cfg(test)]
#[path = "level_import_tests.rs"]
mod tests;
