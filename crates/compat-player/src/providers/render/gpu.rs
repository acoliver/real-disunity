#[cfg(test)]
#[path = "gpu_tests.rs"]
mod tests;

use super::super::binding::Binding;
use crate::extraction::{AssetTable, Contribution};
use worldrunner::scene::{
    gpu::{
        device::GpuContext, draw::SceneGpu, frame::FrameUniform, offscreen::OffscreenTarget,
        renderer::ScenePipelines, wgpu,
    },
    post,
    resources::{
        CanonicalTextureStore,
        texture::{MipLevel, TextureKind, TextureLayer, TextureUpload},
    },
};
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
pub struct Renderer<'a> {
    context: &'a GpuContext,
    scene: SceneGpu,
    pipelines: ScenePipelines,
    chain: post::PostChain<super::stage::Factory>,
    store: CanonicalTextureStore,
    control: super::super::RenderControl,
}
impl<'a> Renderer<'a> {
    pub fn new(
        context: &'a GpuContext,
        assets: &AssetTable,
        control: super::super::RenderControl,
    ) -> Result<Self, String> {
        let pipelines = ScenePipelines::new(context.device(), FORMAT);
        let mut scene = SceneGpu::empty(context.device(), context.queue(), &pipelines);
        scene
            .set_dynamic_textures(
                context.device(),
                context.queue(),
                &pipelines,
                &assets.textures,
            )
            .map_err(|e| e.to_string())?;
        let factory = super::stage::Factory::new(context.device(), FORMAT, control);
        let chain = post::PostChain::new(
            context.device(),
            post::FrameConfig::new([64, 64], FORMAT, 1),
            post::Resources::default(),
            factory,
        )
        .map_err(|e| e.to_string())?;
        let store = CanonicalTextureStore::new(context.device(), context.queue());
        Ok(Self {
            context,
            scene,
            pipelines,
            chain,
            store,
            control,
        })
    }
    pub fn capture(&mut self, contribution: &Contribution) -> Result<Vec<u8>, String> {
        let c = self.context;
        let quads = contribution.quads.as_slice();
        let quads = match self.control {
            super::super::RenderControl::Connected => quads,
            #[cfg(test)]
            super::super::RenderControl::WrongStage => quads,
            #[cfg(test)]
            super::super::RenderControl::Disconnected => &[],
        };
        self.scene
            .set_dynamic_quads(c.device(), c.queue(), &self.pipelines, quads)
            .map_err(|e| e.to_string())?;
        self.scene
            .set_frame(c.queue(), FrameUniform::unlit(&contribution.view));
        let depth = OffscreenTarget::new(c.device(), contribution.view.viewport())
            .map_err(|e| e.to_string())?;
        let output = post::color_texture(c.device(), "qualified caller output", [64, 64], FORMAT);
        let mut encoder = c
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        draw(
            &mut encoder,
            self.chain.targets().scene(),
            depth.depth_view(),
            &self.scene,
            &self.pipelines,
            contribution.clear,
        );
        self.chain.encode(
            &mut encoder,
            &output.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        let mut readback = post::Readback::new(
            c.device(),
            post::FrameConfig::new([64, 64], FORMAT, 1),
            1_048_576,
        )
        .map_err(|e| e.to_string())?;
        readback
            .encode(&mut encoder, &output)
            .map_err(|e| e.to_string())?;
        let submission = c.queue().submit([encoder.finish()]);
        readback
            .read_encoded(c.device(), Some(submission))
            .map_err(|e| e.to_string())
    }
    pub fn retain(&mut self, binding: &Binding, images: &[Vec<u8>]) -> Result<(), String> {
        for (index, pixels) in images.iter().enumerate() {
            let mips = [MipLevel {
                width: 64,
                height: 64,
                bytes_per_row: 256,
                rows: 64,
                bytes: pixels,
            }];
            let layers = [TextureLayer {
                format: FORMAT,
                width: 64,
                height: 64,
                mips: &mips,
            }];
            let id = format!("qualified-capture-{index}");
            let texture = self
                .store
                .get_or_upload(
                    &id,
                    "immutable qualification capture",
                    wgpu::TextureUsages::COPY_SRC,
                    || TextureUpload::new(&layers, 0, TextureKind::D2),
                )
                .map_err(|e| e.to_string())?;
            let mut readback = post::Readback::new(
                self.context.device(),
                post::FrameConfig::new([64, 64], FORMAT, 1),
                1_048_576,
            )
            .map_err(|e| e.to_string())?;
            if readback
                .read(self.context.device(), self.context.queue(), &texture)
                .map_err(|e| e.to_string())?
                != *pixels
            {
                return Err("canonical retained texture changed pixels".into());
            }
            compat_kit::files::atomic_write(
                &binding.path(&format!(
                    ".work/resumed-provider-qualification-v2/capture-{index}.rgba"
                )),
                pixels,
            )
            .map_err(|e| e.clone())?;
        }
        Ok(())
    }
}
fn draw(
    encoder: &mut wgpu::CommandEncoder,
    color: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    scene: &SceneGpu,
    pipelines: &ScenePipelines,
    clear: [f64; 4],
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("qualified canonical unlit-alpha scene"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: color,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: clear[0],
                    g: clear[1],
                    b: clear[2],
                    a: clear[3],
                }),
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
    scene.draw(&mut pass, pipelines);
}
