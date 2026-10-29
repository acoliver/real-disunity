//! SSAO and SSR under TAA jitter on a static camera.
//!
//! Each case renders the same fixed view for `FRAMES` frames in four variants (TAA on or off,
//! the screen-space feature on or off) and reads the tone-mapped 8-bit output. Flicker is the
//! mean absolute difference between consecutive frames over `MEASURED`; the feature's
//! contribution is its output minus the output without it, and bias is how far that
//! contribution moves when TAA is switched on. Values are on the 0..=1 scale of the 8-bit
//! sRGB bytes, which carries a quantisation floor of about 0.001.
use super::*;
use crate::compositor::{FrameRenderer, encode_scene};
use worldrunner_render_plan::{
    SceneSsao, SceneSsaoDepthFootprint, SceneSsaoFilter, SceneSsaoKernel, SceneSsr, TexelFormat,
};
use worldrunner_render_post::{
    AntiAliasing, AttachmentView, FrameConfig, FrameError, MsaaSamples, PostResolve, Readback,
    ResourceRef, ToneMap,
};

const FRAMES: usize = 16;
const MEASURED: std::ops::Range<usize> = 8..FRAMES;
const HDR: TextureFormat = TextureFormat::Rgba16Float;
/// Smallest difference the 8-bit output can resolve reliably.
const QUANTISATION: f64 = 0.001;

#[derive(Clone, Copy, PartialEq)]
enum Feature {
    Ssao,
    Ssr,
}

#[derive(Clone, Copy)]
struct Variant {
    feature_on: bool,
    taa: bool,
}

fn surface(albedo: u32, smooth: bool, mode: SceneAlphaMode) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(albedo),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: if smooth { 100_000 } else { 1_000_000 },
        metalness_micro: 0,
        alpha_mode: mode,
        double_sided: true,
    })
    .unwrap()
}

fn ssao_settings() -> SceneSsao {
    SceneSsao {
        depth_footprint: SceneSsaoDepthFootprint::default(),
        kernel: SceneSsaoKernel::Hemisphere,
        filter: SceneSsaoFilter::DepthAwareBox,
        radius_millimeters: 2000,
        intensity_micro: 1_000_000,
        sample_count: 64,
        bias_millimeters: 10,
        horizon_angle_millidegrees: 90_000,
    }
}

fn ssr_settings() -> SceneSsr {
    SceneSsr {
        enabled: true,
        max_steps: 64,
        thickness_millimeters: 100,
        roughness_cutoff_micro: 600_000,
        resolution_scale_micro: 500_000,
        ..Default::default()
    }
}

/// A floor and a card standing on it: a crease for SSAO, a smooth floor under a card for SSR.
fn scene(feature: Feature, variant: Variant) -> RenderScene {
    let mut parts = effect_parts();
    if variant.taa {
        parts.anti_alias = SceneAntiAlias::taa(900_000);
    }
    let ambient_only = feature == Feature::Ssao;
    if ambient_only {
        parts.ssao = variant.feature_on.then(ssao_settings);
        parts.sun.irradiance_micro = 0;
        parts.ambient.sky_irradiance_micro = 500_000;
        parts.ambient.ground_irradiance_micro = 500_000;
    }
    let mut effects = SceneEffects::new(parts).unwrap();
    if feature == Feature::Ssr && variant.feature_on {
        effects = effects.with_ssr(ssr_settings()).unwrap();
    }
    let floor = u32::from(!ambient_only);
    let mode = SceneAlphaMode::Opaque;
    let ground = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: Box::new([70, 70, 80, 255]),
    };
    let object = |mesh, material| {
        SceneObject::new(
            identity(),
            MeshHandle::from_raw(mesh),
            MaterialHandle::from_raw(material),
        )
    };
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![texture(0xffff), ground],
            vec![surface(0, false, mode), surface(floor, !ambient_only, mode)],
        )
        .unwrap(),
        geometry: SceneGeometry::new(vec![quad(), card(None)], vec![object(0, 1), object(1, 0)])
            .unwrap(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

fn pipelines(
    context: &GpuContext,
    feature: Feature,
    variant: Variant,
    samples: MsaaSamples,
) -> ScenePipelines {
    let device = context.device();
    if feature == Feature::Ssao && variant.feature_on {
        ScenePipelines::with_ssao(device, HDR, samples.count()).unwrap()
    } else {
        ScenePipelines::with_samples(device, HDR, samples.count()).unwrap()
    }
}

fn output_texture(context: &GpuContext) -> wgpu::Texture {
    context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("taa screen space output"),
        size: wgpu::Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: COLOR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn rgb(bytes: &[u8]) -> Vec<f64> {
    bytes
        .chunks_exact(4)
        .flat_map(|texel| [0, 1, 2].map(|lane| f64::from(texel[lane]) / 255.0))
        .collect()
}

/// Renders `FRAMES` frames of the fixed view and returns each as 0..=1 RGB.
fn render(
    context: &GpuContext,
    feature: Feature,
    variant: Variant,
    samples: MsaaSamples,
) -> Vec<Vec<f64>> {
    let device = context.device();
    let scene = scene(feature, variant);
    let mut renderer = FrameRenderer::hdr_antialiased(
        context,
        [TARGET, TARGET],
        COLOR_FORMAT,
        AntiAliasing::new(samples, PostResolve::None),
        ToneMap::Reinhard,
        scene.effects(),
    )
    .unwrap();
    let pipelines = pipelines(context, feature, variant, samples);
    let mut gpu = SceneGpu::upload(device, context.submission(), &scene, &pipelines).unwrap();
    let attach = |name, texture| AttachmentView::new(ResourceRef::new(name), texture).unwrap();
    let color = attach("scene color", renderer.targets().scene().texture());
    let depth = attach("scene depth", renderer.targets().depth().texture());
    let output = output_texture(context);
    let view = output.create_view(&TextureViewDescriptor::default());
    let mut readback = Readback::new(
        device,
        FrameConfig::new([TARGET, TARGET], COLOR_FORMAT, 1),
        1 << 20,
    )
    .unwrap();
    let scene_view = SceneView::new(
        pose(),
        camera(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    (0..FRAMES)
        .map(|index| {
            renderer.select_frame_slot(index % 3);
            let frame = FrameUniform::new(&scene_view, scene.effects()).unwrap();
            context
                .submission()
                .prepare(device, |encoder, _| {
                    let scene = (&mut gpu, &pipelines);
                    encode_scene(
                        &mut renderer,
                        device,
                        encoder,
                        scene,
                        (&color, &depth),
                        frame,
                    )?;
                    renderer.encode_output(encoder, &view).unwrap();
                    Ok::<_, FrameError>(())
                })
                .unwrap()
                .submit()
                .unwrap();
            rgb(&readback
                .read(device, context.submission(), &output)
                .unwrap())
        })
        .collect()
}

fn count(len: usize) -> f64 {
    f64::from(u32::try_from(len).expect("fixture sizes fit in u32"))
}

fn mean_abs_difference(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| (a - b).abs()).sum::<f64>() / count(a.len())
}

fn flicker(frames: &[Vec<f64>]) -> f64 {
    let pairs: Vec<f64> = MEASURED
        .map(|t| mean_abs_difference(&frames[t], &frames[t - 1]))
        .collect();
    pairs.iter().sum::<f64>() / count(pairs.len())
}

fn steady(frames: &[Vec<f64>]) -> Vec<f64> {
    let mut sum = vec![0.0; frames[0].len()];
    for frame in &frames[MEASURED] {
        sum.iter_mut()
            .zip(frame)
            .for_each(|(sum, value)| *sum += value);
    }
    sum.iter().map(|sum| sum / count(MEASURED.len())).collect()
}

fn contribution(on: &[Vec<f64>], off: &[Vec<f64>]) -> Vec<f64> {
    steady(on)
        .iter()
        .zip(steady(off))
        .map(|(on, off)| on - off)
        .collect()
}

#[derive(Debug)]
struct Measurement {
    flicker_taa_with: f64,
    flicker_taa_without: f64,
    flicker_plain_with: f64,
    effect: f64,
    bias: f64,
}

fn measure(feature: Feature, samples: MsaaSamples) -> Measurement {
    let context = context();
    let run = |feature_on, taa| render(&context, feature, Variant { feature_on, taa }, samples);
    let (taa_with, taa_without) = (run(true, true), run(false, true));
    let (plain_with, plain_without) = (run(true, false), run(false, false));
    let taa_effect = contribution(&taa_with, &taa_without);
    let plain_effect = contribution(&plain_with, &plain_without);
    let zero = vec![0.0; plain_effect.len()];
    Measurement {
        flicker_taa_with: flicker(&taa_with),
        flicker_taa_without: flicker(&taa_without),
        flicker_plain_with: flicker(&plain_with),
        effect: mean_abs_difference(&plain_effect, &zero),
        bias: mean_abs_difference(&taa_effect, &plain_effect),
    }
}

/// Regression limits taken from the lavapipe measurement (about 1.4x the measured value) and
/// the smallest feature contribution the scene must show. They must not be loosened.
struct Limits {
    flicker: f64,
    bias: f64,
    effect: f64,
}

/// Fails when TAA with the feature flickers or drifts beyond the thresholds fixed before
/// measuring, or beyond the limits of the measurement itself.
fn assert_stable(label: &str, m: &Measurement, limits: &Limits) {
    eprintln!("{label}: {m:?}");
    assert!(m.effect > limits.effect, "{label}: weak feature {m:?}");
    assert!(
        m.flicker_taa_with <= limits.flicker,
        "{label}: flicker {m:?}"
    );
    assert!(m.bias <= limits.bias, "{label}: bias {m:?}");
    assert!(
        m.flicker_plain_with <= 0.0,
        "{label}: unjittered output must be static"
    );
    let allowed = (0.5 * m.flicker_taa_without).max(QUANTISATION);
    assert!(
        m.flicker_taa_with - m.flicker_taa_without <= allowed,
        "{label}: flicker {m:?}"
    );
    assert!(
        m.bias <= (0.25 * m.effect).max(QUANTISATION),
        "{label}: bias {m:?}"
    );
}

#[test]
fn ssao_under_taa_is_stable_and_unbiased_at_1x() {
    let limits = Limits {
        flicker: 0.0010,
        bias: 0.00066,
        effect: 0.0085,
    };
    assert_stable("ssao 1x", &measure(Feature::Ssao, MsaaSamples::X1), &limits);
}

#[test]
fn ssao_under_taa_is_stable_and_unbiased_at_4x() {
    let limits = Limits {
        flicker: 0.0,
        bias: 0.00001,
        effect: 0.0085,
    };
    assert_stable("ssao 4x", &measure(Feature::Ssao, MsaaSamples::X4), &limits);
}

#[test]
fn ssr_under_taa_is_stable_and_unbiased_at_1x() {
    let limits = Limits {
        flicker: 0.0008,
        bias: 0.00017,
        effect: 0.0060,
    };
    assert_stable("ssr 1x", &measure(Feature::Ssr, MsaaSamples::X1), &limits);
}

#[test]
fn ssr_under_taa_is_stable_and_unbiased_at_4x() {
    let limits = Limits {
        flicker: 0.0,
        bias: 0.00001,
        effect: 0.0060,
    };
    assert_stable("ssr 4x", &measure(Feature::Ssr, MsaaSamples::X4), &limits);
}
