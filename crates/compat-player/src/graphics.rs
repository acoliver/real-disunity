use crate::extraction::{AssetTable, Contribution};
use std::{fs::File, path::Path};
use worldrunner::scene::gpu::{
    device::GpuContext,
    draw::SceneGpu,
    frame::FrameUniform,
    offscreen::{OffscreenTarget, RgbaImage},
    renderer::ScenePipelines,
    wgpu,
};

pub(crate) struct Graphics {
    context: GpuContext,
    scene: SceneGpu,
    pipelines: ScenePipelines,
    target: OffscreenTarget,
}
impl Graphics {
    pub fn new(
        assets: &AssetTable,
        viewport: worldrunner::scene::gpu::Viewport,
    ) -> Result<Self, String> {
        let context =
            pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty()))
                .map_err(|error| error.to_string())?;
        let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
        let mut scene = SceneGpu::empty(context.device(), context.submission(), &pipelines)
            .map_err(|error| error.to_string())?;
        scene
            .set_dynamic_textures(context.device(), context.submission(), &assets.textures)
            .map_err(|error| error.to_string())?;
        let target =
            OffscreenTarget::new(context.device(), viewport).map_err(|error| error.to_string())?;
        Ok(Self {
            context,
            scene,
            pipelines,
            target,
        })
    }
    pub fn textures(&mut self, assets: &AssetTable) -> Result<(), String> {
        self.scene
            .set_dynamic_textures(
                self.context.device(),
                self.context.submission(),
                &assets.textures,
            )
            .map_err(|error| error.to_string())
    }
    pub fn render(&mut self, contribution: &Contribution) -> Result<RgbaImage, String> {
        self.scene
            .set_dynamic_quads(
                self.context.device(),
                self.context.submission(),
                &self.pipelines,
                &contribution.quads,
            )
            .map_err(|error| error.to_string())?;
        self.scene.set_frame(
            self.context.queue(),
            FrameUniform::unlit(&contribution.view),
        );
        let [r, g, b, a] = contribution.clear;
        self.target
            .render_with_clear_color(
                &self.context,
                &self.scene,
                &self.pipelines,
                wgpu::Color { r, g, b, a },
            )
            .map_err(|error| error.to_string())
    }
}

pub(crate) fn write_png(path: &Path, image: &RgbaImage) -> Result<(), String> {
    let _span = worldrunner::scene::viewer::cpu_profile::CpuProfile::span("png_write");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let file = File::create(path).map_err(|error| error.to_string())?;
    let mut encoder = png::Encoder::new(file, image.viewport.width, image.viewport.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fastest);
    encoder.set_filter(png::Filter::Up);
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer
        .write_image_data(&image.pixels)
        .map_err(|error| error.to_string())?;
    writer.finish().map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "graphics_tests.rs"]
mod tests;
