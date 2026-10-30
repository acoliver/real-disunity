use super::{SceneMaterial, SceneResources, SceneTexture, SceneValidationError, TextureHandle};

impl SceneResources {
    /// Validates every texture payload and material texture reference.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        for texture in self.textures.iter() {
            texture.validate()?;
        }
        for material in self.materials.iter() {
            validate_auxiliary_textures(material, &self.textures)?;
            require_texture("material albedo", material.albedo(), &self.textures)?;
            if let Some(atlas) = material.impostor() {
                atlas.validate_channels(material)?;
                if let Some((depth, _)) = atlas.relief() {
                    require_texture("impostor depth", depth, &self.textures)?;
                }
                if let Some(border) = atlas.borders() {
                    require_texture("impostor borders", border, &self.textures)?;
                }
            }
            for (part, handle) in [
                ("material normal", material.normal()),
                ("material roughness_metallic", material.roughness_metallic()),
                ("material ambient_occlusion", material.ambient_occlusion()),
                ("material self_illumination", material.self_illumination()),
            ] {
                if let Some(handle) = handle {
                    require_texture(part, handle, &self.textures)?;
                }
            }
            if let Some(layer) = material.features().and_then(|features| features.layer) {
                let count = self.materials.len() as u64;
                if u64::from(layer.material().raw()) >= count {
                    return Err(SceneValidationError::HandleOutOfRange {
                        part: "material layer",
                        resource: "material",
                        handle: layer.material().raw(),
                        limit: count,
                    });
                }
                if self.materials[layer.material().raw() as usize]
                    .features()
                    .is_some_and(|features| features.layer.is_some())
                {
                    return Err(SceneValidationError::UnsupportedInput {
                        part: "nested material layers",
                    });
                }
            }
            material.alpha_mode().validate()?;
        }
        Ok(())
    }
}

/// Requires a material texture to address a two-dimensional payload.
fn require_texture(
    part: &'static str,
    handle: TextureHandle,
    textures: &[SceneTexture],
) -> Result<(), SceneValidationError> {
    let texture =
        textures
            .get(handle.raw() as usize)
            .ok_or(SceneValidationError::HandleOutOfRange {
                part,
                resource: "texture",
                handle: handle.raw(),
                limit: textures.len() as u64,
            })?;
    if matches!(texture.payload(), SceneTexture::SpecularCube(_)) {
        return Err(SceneValidationError::UnsupportedInput { part });
    }
    Ok(())
}

fn validate_auxiliary_textures(
    material: &SceneMaterial,
    textures: &[SceneTexture],
) -> Result<(), SceneValidationError> {
    material.fade_coverage().validate(textures)?;
    if let Some(field) = material
        .features()
        .and_then(|f| f.wind)
        .and_then(crate::scene::SceneWind::field)
    {
        require_texture("wind force field", field.texture(), textures)?;
    }
    Ok(())
}
