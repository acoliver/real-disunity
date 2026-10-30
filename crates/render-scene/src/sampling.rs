use worldrunner_render_plan::TextureAnisotropy;

#[derive(Default)]
pub(crate) struct MaterialSamplers {
    samplers: std::collections::HashMap<TextureAnisotropy, wgpu::Sampler>,
}

impl MaterialSamplers {
    /// wgpu caps the ratio at 16, or at 1 without anisotropic filtering support.
    pub(crate) fn get(
        &mut self,
        device: &wgpu::Device,
        material: &worldrunner_render_plan::SceneMaterial,
    ) -> &wgpu::Sampler {
        let key = material.anisotropy();
        self.samplers.entry(key).or_insert_with(|| {
            device.create_sampler(&crate::upload::material_sampler_descriptor(key))
        })
    }
}

#[cfg(test)]
pub(crate) mod tests;
