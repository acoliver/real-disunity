use super::FrameRenderer;
impl FrameRenderer {
    /// Install shared linear environment maps and select environment replacement for SSR.
    pub fn set_reflection_environment(
        &mut self,
        device: &wgpu::Device,
        maps: &crate::sky::preparation::PreparedEnvironmentMaps,
    ) -> Result<(), crate::sky::preparation::PreparationError> {
        let (cube, lut) = maps.views()?;
        self.ssr
            .as_mut()
            .expect("SSR must be installed")
            .environment = [cube.clone(), lut.clone()];
        self.environment_conventions[0] = Some(maps.convention());
        let screen = self.ssao.as_mut().expect("SSR metadata must be installed");
        let group = crate::ssr::forward::group(device, screen, &[cube.clone(), lut.clone()]);
        screen
            .geometry
            .forward
            .as_mut()
            .expect("SSR forward pipelines")
            .1 = group;
        self.ssr
            .as_mut()
            .expect("SSR must be installed")
            .composition =
            worldrunner_render_lighting::reflections::Composition::ReplaceEnvironment;
        if let Some(water) = &mut self.water {
            water.environment = [cube, lut];
            self.environment_conventions[1] = Some(maps.convention());
        }
        Ok(())
    }
    /// Select the reflection composition algorithm explicitly.
    pub fn set_reflection_composition(
        &mut self,
        composition: worldrunner_render_lighting::reflections::Composition,
    ) {
        self.ssr
            .as_mut()
            .expect("SSR must be installed")
            .composition = composition;
    }
    /// Install R7's submitted, prefiltered cube and integrated BRDF LUT.
    pub fn set_water_environment(
        &mut self,
        maps: &crate::sky::preparation::PreparedEnvironmentMaps,
    ) -> Result<(), crate::sky::preparation::PreparationError> {
        let (cube, lut) = maps.views()?;
        self.water
            .as_mut()
            .expect("water must be installed")
            .environment = [cube, lut];
        self.environment_conventions[1] = Some(maps.convention());
        Ok(())
    }
}

impl FrameRenderer {
    pub(crate) fn retain_environment_conventions(
        &mut self,
        previous: [Option<worldrunner_render_plan::SceneSpecularConvention>; 2],
    ) {
        self.environment_conventions = [
            self.ssr.as_ref().and(previous[0]),
            self.water.as_ref().and(previous[1]),
        ];
    }
    pub(crate) fn validate_environment_conventions(
        &self,
        selected: worldrunner_render_plan::SceneSpecularConvention,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        for prepared in self.environment_conventions.iter().flatten() {
            if prepared.roughness != selected.roughness
                || prepared.visibility != selected.visibility
            {
                return Err(worldrunner_render_post::FrameError::UnsupportedEffect(
                    "environment BRDF convention mismatch",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn prepared_map_conventions_must_match_frame_and_survive_selection() {
        use worldrunner_render_plan::{SceneSpecularConvention, SceneSpecularVisibility};
        let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
        let mut renderer = super::FrameRenderer::new(context.device());
        renderer.environment_conventions[0] = Some(SceneSpecularConvention::default());
        assert!(
            renderer
                .validate_environment_conventions(SceneSpecularConvention::default())
                .is_ok()
        );
        assert!(
            renderer
                .validate_environment_conventions(SceneSpecularConvention {
                    visibility: SceneSpecularVisibility::SchlickSmith,
                    ..Default::default()
                })
                .is_err()
        );
    }
}
