use super::*;
use crate::Viewport;
use crate::compositor::terms::{CaptureTerm, TermCapture};
use crate::offscreen::LinearRgbaImage;

#[test]
fn sun_visibility_capture_matches_cpu_occluder_projection_across_edge() {
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let view = view()
        .with_raster_sample(
            Viewport {
                width: 64,
                height: 64,
            },
            [0.0; 2],
        )
        .unwrap();
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    let capture = TermCapture::new(context.device(), [64, 64], CaptureTerm::SunVisibility);
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.encode(
                &context,
                &view,
                &target.color_attachment(),
                &target.depth_attachment(),
                encoder,
            )?;
            renderer.encode_term_capture(context.device(), encoder, &capture);
            Ok::<_, ShadowError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    let mut read = worldrunner_render_post::Readback::new(
        context.device(),
        worldrunner_render_post::FrameConfig::new([64, 64], wgpu::TextureFormat::Rgba16Float, 1),
        1024 * 1024,
    )
    .unwrap();
    let bytes = read
        .read(context.device(), context.submission(), &capture.texture)
        .unwrap();
    let image =
        LinearRgbaImage::from_native(view.viewport(), wgpu::TextureFormat::Rgba16Float, &bytes)
            .unwrap();
    let frame = FrameUniform::new(&view, source.effects()).unwrap();
    // A horizontal caster at z=1 projects by -height * light.xy / light.z.
    for (x, expected) in [(-1.3, 1.0), (-0.75, 0.0), (-0.2, 1.0)] {
        let clip = frame.view_projection * Vec4::new(x, 0.0, 0.0, 1.0);
        let ndc = clip.truncate() / clip.w;
        let pixel =
            glam::Vec2::new((ndc.x * 0.5 + 0.5) * 64.0, (0.5 - ndc.y * 0.5) * 64.0).as_uvec2();
        let px = usize::try_from(pixel.x).unwrap();
        let py = usize::try_from(pixel.y).unwrap();
        assert_eq!(
            image.pixels[py * 64 + px].map(f32::to_bits),
            [expected, expected, expected, 1.0].map(f32::to_bits)
        );
    }
}
