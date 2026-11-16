//! Scene textures whose top mip levels the renderer may drop to fit adapter memory (GitHub #376
//! step 3). The scene declares them; none is reducible by default, so a new consumer keeps its
//! authored chain until its producer opts in.
use super::{
    RenderScene, SceneEffects, SceneFadeCoverage, SceneResources, SceneValidationError,
    TextureHandle,
};

impl SceneResources {
    /// The declared reducible textures, in declaration order.
    #[must_use]
    pub fn reducible_textures(&self) -> &[TextureHandle] {
        &self.reducible_textures
    }
}

impl RenderScene {
    /// Declares the textures whose top mip levels may be dropped at upload, replacing any
    /// earlier declaration.
    ///
    /// # Errors
    /// Returns [`SceneValidationError`] when a handle is outside the texture table, or when the
    /// scene also binds it through a consumer that reads level 0 or the authored size.
    pub fn with_reducible_textures(
        &self,
        handles: Vec<TextureHandle>,
    ) -> Result<Self, SceneValidationError> {
        let mut resources = self.resources.clone();
        resources.reducible_textures = handles.into();
        let scene = Self {
            resources,
            ..self.clone()
        };
        scene.validate()?;
        Ok(scene)
    }

    pub(super) fn validate_reducible(&self) -> Result<(), SceneValidationError> {
        let declared = self.resources.reducible_textures();
        if declared.is_empty() {
            return Ok(());
        }
        let limit = super::table_limit(self.resources.textures().len(), "textures")?;
        for handle in declared {
            super::require_handle("reducible textures", handle.raw(), "texture", limit)?;
        }
        match self
            .fixed_level_textures()
            .into_iter()
            .find(|(_, handle)| declared.contains(handle))
        {
            Some((part, _)) => Err(SceneValidationError::UnsupportedInput { part }),
            None => Ok(()),
        }
    }

    /// Every binding that reads its texture at level 0 or by its authored size, named by the
    /// refusal it causes for a reducible texture.
    fn fixed_level_textures(&self) -> Vec<(&'static str, TextureHandle)> {
        let mut found = Vec::new();
        if let Some(effects) = &self.effects {
            effect_textures(effects, &mut found);
        }
        for material in self.resources.materials() {
            let wind = material
                .features()
                .and_then(|features| features.wind)
                .and_then(super::SceneWind::field);
            if let Some(field) = wind {
                found.push((
                    "a reducible texture is a material wind field",
                    field.texture(),
                ));
            }
            if let SceneFadeCoverage::Threshold { texture, .. } = material.fade_coverage() {
                found.push(("a reducible texture is a fade threshold texture", texture));
            }
            if let Some(impostor) = material.impostor() {
                let maps = [
                    impostor.relief().map(|(texture, _)| texture),
                    impostor.borders(),
                ];
                for texture in maps.into_iter().flatten() {
                    found.push((
                        "a reducible texture is an impostor relief or border map",
                        texture,
                    ));
                }
            }
        }
        self.terrain_textures(&mut found);
        found
    }

    /// Terrain palettes and layered page sources are prepared from their own copies, sized by
    /// the authored chain, so their sources stay authored.
    fn terrain_textures(&self, found: &mut Vec<(&'static str, TextureHandle)>) {
        const TERRAIN: &str = "a reducible texture is a terrain palette or layer source";
        let Some(terrain) = &self.terrain else {
            return;
        };
        for surface in terrain.surfaces() {
            let material = &self.resources.materials()[surface.material().raw() as usize];
            let maps = [
                Some(material.albedo()),
                material.normal(),
                material.roughness_metallic(),
            ];
            for texture in maps.into_iter().flatten() {
                found.push((TERRAIN, texture));
            }
            if let Some(layer) = surface.appearance().layered {
                for texture in [layer.albedo_metallic, layer.normal_roughness, layer.height] {
                    found.push((TERRAIN, texture));
                }
            }
        }
        let colorization = terrain
            .layered_source()
            .and_then(|source| source.colorization.as_ref());
        if let Some(colorization) = colorization {
            found.push((
                "a reducible texture is the terrain colorization",
                colorization.texture,
            ));
        }
    }
}

fn effect_textures(effects: &SceneEffects, found: &mut Vec<(&'static str, TextureHandle)>) {
    if let Some(environment) = effects.environment() {
        found.push((
            "a reducible texture is the environment cube",
            environment.cube,
        ));
        found.push((
            "a reducible texture is the environment BRDF table",
            environment.brdf_lut,
        ));
    }
    if let super::SceneSky::DomeTexture { texture } = effects.sky() {
        found.push(("a reducible texture is the sky dome", *texture));
    }
    if let Some(atmosphere) = effects.atmosphere() {
        for texture in atmosphere.discs.iter().filter_map(|disc| disc.texture) {
            found.push(("a reducible texture is a celestial disc", texture));
        }
    }
    let bodies = effects.water_bodies().iter().map(|body| body.material);
    for settings in effects.water_settings().into_iter().chain(bodies) {
        let maps = [
            settings.normal,
            settings.flow,
            settings.wind.map(|wind| wind.field.texture()),
        ];
        for texture in maps.into_iter().flatten() {
            found.push((
                "a reducible texture is a water normal, flow or wind map",
                texture,
            ));
        }
    }
}

#[cfg(test)]
#[path = "reducible_tests.rs"]
mod tests;
