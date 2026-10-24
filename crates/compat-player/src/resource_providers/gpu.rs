use crate::extraction::{AssetTable, Contribution};
use worldrunner::scene::{
    gpu::{
        device::GpuContext, draw::SceneGpu, frame::FrameUniform, offscreen::OffscreenTarget,
        renderer::ScenePipelines, wgpu,
    },
    post,
};
pub(super) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
#[derive(Clone, Copy)]
pub(super) enum Control {
    Connected,
    #[cfg(test)]
    Disconnected,
    #[cfg(test)]
    WrongStage,
}
pub(super) struct Renderer<'gpu> {
    context: &'gpu GpuContext,
    scene: SceneGpu,
    pipelines: ScenePipelines,
    chain: post::PostChain<super::stage::Factory>,
    control: Control,
}
impl<'gpu> Renderer<'gpu> {
    pub fn new(
        context: &'gpu GpuContext,
        assets: &AssetTable,
        control: Control,
    ) -> Result<Self, String> {
        let config = post::ExactSrgb8::frame_config(context.device(), [64, 64])
            .map_err(|error| error.to_string())?;
        let pipelines = ScenePipelines::new(context.device(), config.format);
        let mut scene = SceneGpu::empty(context.device(), context.queue(), &pipelines);
        scene
            .set_dynamic_textures(
                context.device(),
                context.queue(),
                &pipelines,
                &assets.textures,
            )
            .map_err(|e| e.to_string())?;
        let chain = post::PostChain::new(
            context.device(),
            config,
            post::Resources::default(),
            super::stage::Factory::new(context.device(), control),
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            context,
            scene,
            pipelines,
            chain,
            control,
        })
    }
    pub fn capture(&mut self, contribution: &Contribution) -> Result<Vec<u8>, String> {
        let c = self.context;
        let quads = match self.control {
            Control::Connected => contribution.quads.as_slice(),
            #[cfg(test)]
            Control::WrongStage => contribution.quads.as_slice(),
            #[cfg(test)]
            Control::Disconnected => &[],
        };
        self.scene
            .set_dynamic_quads(c.device(), c.queue(), &self.pipelines, quads)
            .map_err(|e| e.to_string())?;
        self.scene
            .set_frame(c.queue(), FrameUniform::unlit(&contribution.view));
        let depth = OffscreenTarget::new(c.device(), contribution.view.viewport())
            .map_err(|e| e.to_string())?;
        let output =
            post::ExactSrgb8::new(c.device(), [64, 64]).map_err(|error| error.to_string())?;
        let mut encoder = c
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.draw(&mut encoder, depth.depth_view(), contribution.clear);
        self.chain.encode(&mut encoder, output.write_view());
        let mut readback = post::Readback::new(
            c.device(),
            post::FrameConfig::new([64, 64], FORMAT, 1),
            1_048_576,
        )
        .map_err(|e| e.to_string())?;
        readback
            .encode(&mut encoder, output.texture())
            .map_err(|e| e.to_string())?;
        let submission = c.queue().submit([encoder.finish()]);
        readback
            .read_encoded(c.device(), Some(submission))
            .map_err(|e| e.to_string())
    }
    fn draw(&self, encoder: &mut wgpu::CommandEncoder, depth: &wgpu::TextureView, clear: [f64; 4]) {
        let [r, g, b, a] = clear;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("exact canonical unlit resource scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.chain.targets().scene(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        self.scene.draw(&mut pass, &self.pipelines);
    }
}
