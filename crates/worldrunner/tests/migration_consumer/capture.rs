use worldrunner::scene::gpu::{
    self,
    device::GpuContext,
    draw::SceneGpu,
    offscreen::{OffscreenTarget, RgbaImage},
    renderer::ScenePipelines,
    wgpu,
};

use super::super::policy::{Readiness, admit};

pub fn capture(
    context: &GpuContext,
    scene: &SceneGpu,
    pipelines: &ScenePipelines,
    target: &mut OffscreenTarget,
    readiness: Readiness,
    stage: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView),
) -> Result<RgbaImage, Box<dyn std::error::Error>> {
    admit(readiness)?;
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("migration caller frame"),
        });
    draw(&mut encoder, scene, pipelines, target);
    stage(&mut encoder, target.color_view());
    context.queue().submit([encoder.finish()]);
    Ok(target.read_rgba8(context)?)
}

fn draw(
    encoder: &mut wgpu::CommandEncoder,
    scene: &SceneGpu,
    pipelines: &ScenePipelines,
    target: &OffscreenTarget,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("migration caller-owned pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target.color_view(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: target.depth_view(),
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        ..Default::default()
    });
    scene.draw(&mut pass, pipelines);
}

pub fn pixel(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let index = usize::try_from((y * image.viewport.width + x) * 4).unwrap();
    image.pixels[index..index + 4].try_into().unwrap()
}

pub fn assert_metadata(image: &RgbaImage, viewport: gpu::Viewport) {
    assert_eq!(image.viewport, viewport);
    assert_eq!(
        image.pixels.len(),
        usize::try_from(viewport.width * viewport.height * 4).unwrap()
    );
    assert_eq!(
        gpu::offscreen::aligned_bytes_per_row(viewport.width).unwrap(),
        (viewport.width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
    );
}
