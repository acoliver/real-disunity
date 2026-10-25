use super::resident;
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
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            resident::encode(
                context.device(),
                encoder,
                (scene, pipelines),
                (target.color_view(), target.depth_view()),
            )
            .unwrap();
            stage(encoder, target.color_view());
            target.encode_readback(encoder)
        })?;
    let submission = prepared.submit()?;
    Ok(target.read_encoded(context, submission)?)
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
