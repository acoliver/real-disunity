//! Frozen masked-caster evaluation for byte comparisons with the base shader.
use super::*;
use crate::shadows::ShadowedScene;

impl ShadowedScene {
    pub(crate) fn caster_texture_for_test(&self) -> &wgpu::Texture {
        self.map.depth.texture()
    }

    pub(crate) fn late_card_casters_for_test(&mut self, device: &wgpu::Device) {
        let source = depth_source();
        let body = crate::scene_metadata::receiver::function_source(&source, "fs_depth_masked");
        let source = source.replace(body, "fn fs_depth_masked(input: VertexOutput) { if !scene_alpha_covered_pass(scene_sampled_alpha(input),input,true) { discard; } }");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("base masked caster"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[
                Some(self.pipelines.frame_layout()),
                Some(self.pipelines.material_layout()),
            ],
            ..Default::default()
        });
        let vertices = crate::geometry::vertex_attributes();
        let instances = crate::geometry::instance_attributes();
        let buffers = crate::renderer::vertex_buffers(&vertices, &instances);
        let mut config = self.settings.configuration();
        config.caster_bias.slope_scale = f32::from_bits(self.caster_slope_bits);
        let objects: [_; 8] = std::array::from_fn(|index| {
            let key = PipelineKey::ALL[(index / 4) * 16 + index % 4];
            object_depth_pipeline(device, key, &layout, &module, &buffers, config)
        });
        self.depth_pipelines = std::array::from_fn(|index| {
            let key = PipelineKey::ALL[index];
            objects[usize::from(key.sampling.is_impostor()) * 4 + index % 4].clone()
        });
    }
}
