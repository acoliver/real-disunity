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
        Ok(())
    }
}
