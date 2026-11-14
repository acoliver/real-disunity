//! Frozen masked-caster evaluation for byte comparisons with the base shader.
use super::*;
use crate::shadows::ShadowedScene;

impl ShadowedScene {
    pub(crate) fn caster_texture_for_test(&self) -> &wgpu::Texture {
        self.map.depth.texture()
    }

    pub(crate) fn late_card_casters_for_test(&mut self, _device: &wgpu::Device) {
        let source = depth_source();
        let body = crate::scene_metadata::receiver::function_source(&source, "fs_depth_masked");
        let source = source.replace(body, "fn fs_depth_masked(input: VertexOutput) { if !scene_alpha_covered_pass(scene_sampled_alpha(input),input,true) { discard; } }");
        let mut config = self.settings.configuration();
        config.caster_bias.slope_scale = f32::from_bits(self.caster_slope_bits);
        self.object_caster = object_caster_from(&self.pipelines, &source, config);
    }
}
