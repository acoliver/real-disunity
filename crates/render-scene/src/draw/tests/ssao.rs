use super::*;
use crate::compositor::{FrameRenderer, encode_scene};
use worldrunner_render_plan::{SceneSsao, TexelFormat};
use worldrunner_render_post::{AttachmentView, FrameConfig, Readback, ResourceRef, ToneMap};

fn encode_profile(
    context: &GpuContext,
    gpu: &mut SceneGpu,
    pipelines: &ScenePipelines,
    profile: Option<&SceneEffects>,
    source: &RenderScene,
) -> Result<(), worldrunner_render_post::FrameError> {
    let view = SceneView::new(
        pose(),
        camera(),
        Viewport {
            width: 65,
            height: 49,
        },
    )
    .unwrap();
    let mut renderer = FrameRenderer::hdr(
        context,
        [65, 49],
        COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        profile,
    )
    .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let result = encode_scene(
        &mut renderer,
        context.device(),
        &mut encoder,
        (gpu, pipelines),
        (&color, &depth),
        FrameUniform::new(&view, source.effects()).unwrap(),
    );
    encoder.finish().submit().unwrap();
    context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    result
}

#[test]
fn unshadowed_ssao_requires_exact_profile_settings_before_preparation() {
    use worldrunner_render_post::FrameError;
    let context = context();
    let source = source(1_000_000, true, 255);
    let pipelines =
        ScenePipelines::with_ssao(context.device(), wgpu::TextureFormat::Rgba16Float, 1).unwrap();
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let original = *source.effects().unwrap().ssao().unwrap();
    let before = gpu.frame.get();
    for changed in crate::ssao::tests::differing_settings(original) {
        let mut parts = effect_parts();
        parts.ssao = Some(changed);
        let effects = SceneEffects::new(parts).unwrap();
        assert!(matches!(
            encode_profile(&context, &mut gpu, &pipelines, Some(&effects), &source),
            Err(FrameError::SsaoSettings(
                "SSAO resident/profile settings mismatch"
            ))
        ));
        assert_eq!(gpu.frame.get(), before);
    }
    encode_profile(&context, &mut gpu, &pipelines, source.effects(), &source).unwrap();
    assert!(gpu.frame.get().is_some());
}

fn settings(intensity: u64) -> SceneSsao {
    SceneSsao {
        kernel: worldrunner_render_plan::SceneSsaoKernel::Hemisphere,
        filter: worldrunner_render_plan::SceneSsaoFilter::DepthAwareBox,
        radius_millimeters: 2000,
        intensity_micro: intensity,
        sample_count: 64,
        bias_millimeters: 10,
        horizon_angle_millidegrees: 90_000,
    }
}
fn source(intensity: u64, ambient: bool, alpha: u8) -> RenderScene {
    source_with_policy(
        intensity,
        ambient,
        alpha,
        worldrunner_render_plan::SceneSsaoKernel::Hemisphere,
        worldrunner_render_plan::SceneSsaoFilter::DepthAwareBox,
    )
}
fn source_with_policy(
    intensity: u64,
    ambient: bool,
    alpha: u8,
    kernel: worldrunner_render_plan::SceneSsaoKernel,
    filter: worldrunner_render_plan::SceneSsaoFilter,
) -> RenderScene {
    let mut parts = effect_parts();
    parts.ssao = Some(SceneSsao {
        kernel,
        filter,
        ..settings(intensity)
    });
    parts.sun.irradiance_micro = if ambient { 0 } else { 500_000 };
    parts.ambient.sky_irradiance_micro = if ambient { 500_000 } else { 0 };
    parts.ambient.ground_irradiance_micro = parts.ambient.sky_irradiance_micro;
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: Box::new([180, 140, 110, alpha]),
    };
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![texture],
            vec![material(
                SceneAlphaMode::Mask {
                    cutoff_micro: 500_000,
                },
                true,
            )],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![quad(), card(None)],
            vec![
                SceneObject::new(
                    identity(),
                    MeshHandle::from_raw(0),
                    MaterialHandle::from_raw(0),
                ),
                SceneObject::new(
                    identity(),
                    MeshHandle::from_raw(1),
                    MaterialHandle::from_raw(0),
                ),
            ],
        )
        .unwrap(),
        terrain: None,
        effects: Some(SceneEffects::new(parts).unwrap()),
    })
    .unwrap()
}
fn capture(
    context: &GpuContext,
    source: &RenderScene,
    samples: u32,
) -> (Vec<u8>, Vec<u8>, Vec<f32>) {
    let size = [65, 49];
    let view = SceneView::new(
        pose(),
        camera(),
        Viewport {
            width: size[0],
            height: size[1],
        },
    )
    .unwrap();
    let mut renderer = FrameRenderer::hdr(
        context,
        size,
        COLOR_FORMAT,
        samples,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let pipelines =
        ScenePipelines::with_ssao(context.device(), wgpu::TextureFormat::Rgba16Float, samples)
            .unwrap();
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), source, &pipelines).unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    encode_scene(
        &mut renderer,
        context.device(),
        &mut encoder,
        (&mut gpu, &pipelines),
        (&color, &depth),
        FrameUniform::new(&view, source.effects()).unwrap(),
    )
    .unwrap();
    renderer.targets().encode_resolve(&mut encoder);
    encoder.finish().submit().unwrap();
    let read = |texture: &wgpu::Texture| read_texture(context, size, texture);
    let ssao = renderer.ssao.as_ref().unwrap();
    let normals = read(&ssao.normals);
    let ao = read(&ssao.passes.filtered)
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    (read(renderer.targets().texture()), normals, ao)
}
#[test]
fn gpu_ssao_ambient_only_and_msaa_combinations() {
    let context = context();
    for samples in [1, 4] {
        let (sun_baseline, _, _) = capture(&context, &source(0, false, 255), samples);
        let (sun_ao, _, ao) = capture(&context, &source(2_000_000, false, 255), samples);
        assert!(
            ao.iter().any(|value| *value < 0.9),
            "fixture must contain an occluded corner"
        );
        assert_eq!(
            sun_baseline, sun_ao,
            "AO must leave direct sunlight unchanged"
        );
        let (sky_baseline, _, _) = capture(&context, &source(0, true, 255), samples);
        let (sky_ao, _, _) = capture(&context, &source(2_000_000, true, 255), samples);
        assert_ne!(sky_baseline, sky_ao, "sky-lit corner must darken");
        assert!(
            sky_baseline
                .chunks_exact(2)
                .zip(sky_ao.chunks_exact(2))
                .all(
                    |(before, after)| u16::from_le_bytes(after.try_into().unwrap())
                        <= u16::from_le_bytes(before.try_into().unwrap())
                )
        );
    }
}
#[test]
fn gpu_ssao_uniform_alpha_mask_prepass_uses_colour_cutoff() {
    let context = context();
    for samples in [1, 4] {
        let (hidden, normals, _) = capture(&context, &source(1_000_000, true, 100), samples);
        assert!(
            hidden
                .chunks_exact(8)
                .all(|pixel| pixel[..6].iter().all(|byte| *byte == 0)),
            "masked colour has no coverage"
        );
        assert!(
            normals.iter().all(|byte| *byte == 0),
            "masked prepass has no coverage"
        );
        let (visible, normals, _) = capture(&context, &source(1_000_000, true, 200), samples);
        assert!(visible.iter().any(|byte| *byte != 0));
        assert!(
            normals
                .chunks_exact(8)
                .any(|pixel| pixel[6..8] == 0x3c00u16.to_le_bytes())
        );
    }
}

fn read_texture(context: &GpuContext, size: [u32; 2], texture: &wgpu::Texture) -> Vec<u8> {
    Readback::new(
        context.device(),
        FrameConfig::new(size, texture.format(), 1),
        1_000_000,
    )
    .unwrap()
    .read(context.device(), context.submission(), texture)
    .unwrap()
}

#[test]
fn gpu_spiral_bilateral_coverage_and_composition_at_1x_and_4x() {
    let context = context();
    for samples in [1, 4] {
        let scene = |intensity, alpha| {
            source_with_policy(
                intensity,
                true,
                alpha,
                worldrunner_render_plan::SceneSsaoKernel::Spiral,
                worldrunner_render_plan::SceneSsaoFilter::SeparableBilateral,
            )
        };
        let (baseline, normals, _) = capture(&context, &scene(0, 255), samples);
        let (shaded, ao_normals, ao) = capture(&context, &scene(2_000_000, 255), samples);
        assert_eq!(normals, ao_normals);
        assert!(ao.iter().any(|value| *value < 0.99));
        assert_ne!(baseline, shaded);
        let (_, hidden, ao) = capture(&context, &scene(2_000_000, 100), samples);
        assert!(hidden.iter().all(|byte| *byte == 0));
        assert!(ao.iter().all(|value| value.to_bits() == 1.0f32.to_bits()));
    }
}
