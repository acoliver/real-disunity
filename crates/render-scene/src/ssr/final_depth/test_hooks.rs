//! Test-only accessors of this module's types.

use super::*;

impl FinalDepth {
    pub fn encode_for_test(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        input: &wgpu::Texture,
    ) {
        let groups = self.groups(
            device,
            &input.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        for (i, (texture, group)) in self.levels.iter().zip(&groups).enumerate() {
            worldrunner_render_post::fullscreen_pass(
                encoder,
                Some("final depth reference"),
                wgpu::RenderPassColorAttachment {
                    view: &texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                },
                self.pipeline(i),
                &[group],
            );
        }
    }
}
