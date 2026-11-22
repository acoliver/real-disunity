//! The G-buffer observation through the shadowed scene: with the depth prepass the scene's
//! producers store the ground and the caster, the lighting pass draws with the shadow
//! receivers' sets and lights exactly the stored pixels, and the visible frame stays the forward
//! frame. Without the prepass the lighting pass, which runs before Opaque, lights nothing.
use super::*;
use crate::compositor::{FeatureTarget, FrameRenderer};
use crate::gbuffer::GbufferTarget;
use crate::gbuffer::lighting::LightingTarget;
use crate::gbuffer::reference::Surface;
use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
use crate::terrain::deferred_tests::{BLOCK, COLUMNS, Producer, halves};
use worldrunner_render_post::{FrameConfig, Readback, ToneMap};

/// Columns of blocks the producer writes, from the left edge.
const COVERED: u32 = 20;

pub(super) fn read(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    read_as(context, texture, texture.format())
}

fn read_as(context: &GpuContext, texture: &wgpu::Texture, format: wgpu::TextureFormat) -> Vec<u8> {
    Readback::new(
        context.device(),
        FrameConfig::new([texture.width(), texture.height()], format, 1),
        2_000_000,
    )
    .unwrap()
    .read(context.device(), context.submission(), texture)
    .unwrap()
}

/// A producer that writes a grey upward-facing surface over the first [`COVERED`] columns of
/// blocks, encoded for `view` under `effects`, declaring `depth` when the frame has the depth
/// prepass.
fn producer(
    view: &SceneView,
    effects: &SceneEffects,
    depth: Option<wgpu::TextureFormat>,
) -> Producer {
    let up = Surface {
        shading_normal: Vec3::Z,
        geometric_normal: Vec3::Z,
        base_colour: Vec3::splat(0.5),
        roughness: 0.5,
        metal: 0.0,
        emission: 0.0,
        ao: 1.0,
        flags: 0,
        lighting_record: 0,
    };
    let uniform = crate::frame::FrameUniform::new(view, Some(effects)).unwrap();
    Producer {
        blocks: vec![up; usize::try_from(COLUMNS * COLUMNS).unwrap()],
        view: glam::Mat3::from_mat4(uniform.view),
        covered: COVERED,
        depth,
    }
}

/// Whether the pixel at `index` lies in the columns the producer writes.
fn covered(index: usize, width: u32) -> bool {
    u32::try_from(index).unwrap() % width < COVERED * BLOCK
}

/// The SSAO settings that give the shadowed frame its depth prepass.
pub(super) fn ssao() -> SceneSsao {
    SceneSsao {
        receiver: SceneSsaoReceiver::Nearest,
        depth_footprint: SceneSsaoDepthFootprint::default(),
        kernel: SceneSsaoKernel::Hemisphere,
        filter: SceneSsaoFilter::DepthAwareBox,
        radius_millimeters: 2000,
        intensity_micro: 1_000_000,
        sample_count: 32,
        bias_millimeters: 10,
        horizon_angle_millidegrees: 90_000,
    }
}

/// `source` with the depth prepass when `prepass`.
pub(super) fn with_prepass(source: &RenderScene, prepass: bool) -> RenderScene {
    let mut parts = fixture::effects_parts(600_000);
    if prepass {
        parts.ssao = Some(ssao());
    }
    RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: source.geometry().clone(),
        terrain: source.terrain().cloned(),
        effects: Some(SceneEffects::new(parts).unwrap()),
    })
    .unwrap()
}

/// One shadowed frame of `source` through `path`.
pub(super) struct Captured {
    /// The scene colour, `Rgba16Float`.
    pub(super) colour: Vec<u8>,
    /// The lighting pass's diagnostic colour, present only in observation.
    pub(super) lit: Option<Vec<f32>>,
    /// G3 as stored, present with a G-buffer path.
    pub(super) g3: Option<Vec<u16>>,
    /// The metadata prepass depth in view-space metres, present with the depth prepass.
    pub(super) depth: Option<Vec<f32>>,
}

pub(super) fn capture(
    context: &GpuContext,
    source: &RenderScene,
    view: &SceneView,
    path: SceneOpaquePath,
) -> Result<Captured, ShadowError> {
    capture_with_settings(context, source, view, path, settings())
}

pub(super) fn capture_with_settings(
    context: &GpuContext,
    source: &RenderScene,
    view: &SceneView,
    path: SceneOpaquePath,
    shadows: DirectionalShadowSettings,
) -> Result<Captured, ShadowError> {
    let mut frame = frame(context, source, view, path);
    capture_frame(context, source, view, shadows, &mut frame)
}

fn capture_frame(
    context: &GpuContext,
    source: &RenderScene,
    view: &SceneView,
    shadows: DirectionalShadowSettings,
    frame: &mut FrameRenderer,
) -> Result<Captured, ShadowError> {
    let mut renderer = ShadowedScene::with_target(
        context,
        source,
        shadows,
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        frame.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        frame.targets().depth().texture(),
    )
    .unwrap();
    let mut output = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    renderer.encode_with(context, view, (&color, &depth), &mut encoder, &mut *frame)?;
    frame
        .encode_output(&mut encoder, output.color_view())
        .unwrap();
    output.encode_readback(&mut encoder).unwrap();
    let submission = encoder.finish().submit().unwrap();
    output.read_encoded(context, submission).unwrap();
    Ok(captured(context, frame))
}

/// One shadowed frame of `source` through `path`, with the stock terrain draws `draws`.
pub(super) fn capture_drawing(
    context: &GpuContext,
    source: &RenderScene,
    view: &SceneView,
    path: SceneOpaquePath,
    (draws, shadows): (crate::compositor::TerrainDraws, DirectionalShadowSettings),
) -> Result<Captured, ShadowError> {
    let mut frame = frame(context, source, view, path);
    frame.force_terrain_draws(draws);
    capture_frame(context, source, view, shadows, &mut frame)
}

pub(super) fn frame(
    context: &GpuContext,
    source: &RenderScene,
    view: &SceneView,
    path: SceneOpaquePath,
) -> FrameRenderer {
    let effects = source
        .effects()
        .unwrap()
        .clone()
        .with_opaque_path(path)
        .unwrap();
    FrameRenderer::hdr(
        context,
        [view.viewport().width, view.viewport().height],
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        Some(&effects),
    )
    .unwrap()
}

pub(super) fn captured(context: &GpuContext, frame: &FrameRenderer) -> Captured {
    Captured {
        colour: read(context, frame.targets().texture()),
        lit: frame
            .feature_target(FeatureTarget::DeferredLighting(LightingTarget::Colour))
            .map(|texture| halves(&read(context, texture))),
        g3: frame
            .feature_target(FeatureTarget::Gbuffer(GbufferTarget::G3))
            .map(|texture| {
                read(context, texture)
                    .chunks_exact(2)
                    .map(|code| u16::from_le_bytes([code[0], code[1]]))
                    .collect()
            }),
        depth: frame.feature_target(FeatureTarget::Depth).map(|texture| {
            read(context, texture)
                .chunks_exact(4)
                .map(|value| f32::from_le_bytes([value[0], value[1], value[2], value[3]]))
                .collect()
        }),
    }
}

#[test]
fn gpu_shadowed_observation_keeps_the_forward_frame_and_lights_only_stored_pixels() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for terrain in [false, true] {
        for prepass in [false, true] {
            let mut source = scene(&[[0, 0, 1_000]], 600_000, None);
            if terrain {
                source = super::terrain::terrain_scene(&source, false, false);
            }
            let source = with_prepass(&source, prepass);
            let label = format!("terrain {terrain}, prepass {prepass}");
            let forward = capture(&context, &source, &view(), SceneOpaquePath::Forward).unwrap();
            assert!(forward.lit.is_none() && forward.g3.is_none());
            let observed = capture(
                &context,
                &source,
                &view(),
                SceneOpaquePath::GBUFFER_OBSERVATION,
            )
            .unwrap();
            assert!(
                observed.colour == forward.colour,
                "{label}: the observed frame differs"
            );
            assert_stock_lighting(&observed, prepass, &label);
        }
    }
}

fn assert_stock_lighting(observed: &Captured, prepass: bool, label: &str) {
    let lit = observed.lit.as_ref().unwrap();
    let g3 = observed.g3.as_ref().unwrap();
    let mut shaded = Vec::new();
    for (pixel, (texel, code)) in lit.chunks_exact(4).zip(g3).enumerate() {
        if *code == 0 {
            assert!(
                texel.iter().all(|value| value.to_bits() == 0),
                "{label}: pixel {pixel} is not stored but lit"
            );
        } else {
            assert_eq!(
                texel[3].to_bits(),
                1.0_f32.to_bits(),
                "{label}: pixel {pixel} coverage"
            );
            shaded.push(texel[0]);
        }
    }
    if !prepass {
        // No producer draws without the prepass, so nothing is stored or lit.
        assert!(shaded.is_empty(), "{label}: stored without the prepass");
        return;
    }
    assert!(shaded.len() > g3.len() / 4, "{label}: too little stored");
    // The caster's shadow on the stored ground: some stored pixels receive less sun.
    let brightest = shaded.iter().copied().fold(0.0_f32, f32::max);
    let darkest = shaded.iter().copied().fold(f32::INFINITY, f32::min);
    assert!(darkest > 0.0, "{label}: ambient light");
    assert!(
        darkest < 0.9 * brightest,
        "{label}: no shadow in the lit target ({darkest} {brightest})"
    );
}

/// `source` with screen-space reflections, so the lighting pass also writes the reflection
/// products.
fn reflecting(source: &RenderScene) -> RenderScene {
    let effects = source
        .effects()
        .unwrap()
        .clone()
        .with_ssr(SceneSsr {
            enabled: true,
            max_steps: 64,
            thickness_millimeters: 100,
            roughness_cutoff_micro: 600_000,
            resolution_scale_micro: 1_000_000,
            ..SceneSsr::default()
        })
        .unwrap();
    source
        .with_objects_and_effects(source.geometry().objects().to_vec(), Some(effects))
        .unwrap()
}

/// One shadowed frame of `source` with the observation and [`producer`], rendered through the
/// same-frame depth fit, which records the frame as one capture. The producer declares the scene
/// depth `with_depth`, which it must exactly when the scene's own producer gives the slot one.
fn depth_fitted_frame(
    context: &GpuContext,
    source: &RenderScene,
    view: &SceneView,
    with_depth: bool,
) -> Result<FrameRenderer, String> {
    let effects = source
        .effects()
        .unwrap()
        .clone()
        .with_opaque_path(SceneOpaquePath::GBUFFER_OBSERVATION)
        .unwrap();
    let mut frame = frame(context, source, view, SceneOpaquePath::GBUFFER_OBSERVATION);
    let depth = frame.targets().depth().texture().format();
    frame
        .register(
            context.device(),
            Box::new(producer(view, &effects, with_depth.then_some(depth))),
        )
        .unwrap();
    let mut renderer = ShadowedScene::with_target(
        context,
        source,
        settings(),
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    renderer
        .set_depth_fitting(
            context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::SameFrameCapture,
                range: super::sdsm::policy(),
            }),
        )
        .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        frame.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        frame.targets().depth().texture(),
    )
    .unwrap();
    let fit = renderer
        .render_depth_fitted_host(
            context,
            view,
            (&color, &depth),
            (Some(0), false, &[]),
            &mut frame,
        )
        .map_err(|error| format!("{error:?}"))?
        .expect("the same-frame fit adopts");
    assert_eq!((fit.source_frame, fit.adopted_frame), (0, 0));
    Ok(frame)
}

/// Base colour and the geometric normal are set in the producer's columns. When Opaque reuses
/// the prepass depth the scene's own producer also stores its surfaces outside them; a pixel no
/// producer wrote (G3 is 0) is zero in every target. Returns which pixels were written.
fn assert_gbuffer_written(
    context: &GpuContext,
    frame: &FrameRenderer,
    width: u32,
    label: &str,
) -> Vec<bool> {
    let targets = GbufferTarget::ALL.map(|target| {
        let texture = frame
            .feature_target(FeatureTarget::Gbuffer(target))
            .unwrap_or_else(|| panic!("{label}: {target:?} is not allocated"));
        let texel = usize::try_from(target.format().block_copy_size(None).unwrap()).unwrap();
        (target, texel, read_as(context, texture, target.format()))
    });
    let written: Vec<bool> = targets[3]
        .2
        .chunks_exact(2)
        .map(|code| u16::from_le_bytes([code[0], code[1]]) != 0)
        .collect();
    for (target, texel, bytes) in &targets {
        for (pixel, value) in bytes.chunks_exact(*texel).enumerate() {
            let set = value.iter().any(|&byte| byte != 0);
            if covered(pixel, width) {
                if matches!(target, GbufferTarget::G1 | GbufferTarget::G3) {
                    assert!(set, "{label}: {target:?} pixel {pixel} was not written");
                }
            } else if !written[pixel] {
                assert!(
                    !set,
                    "{label}: {target:?} pixel {pixel} is unwritten but set"
                );
            }
        }
    }
    written
}

/// Every lighting target the frame allocates is zero where no producer wrote, and the
/// lit colour covers and lights the pixels inside them. The reflection products exist exactly
/// with `reflections`.
fn assert_lighting_written(
    context: &GpuContext,
    frame: &FrameRenderer,
    (width, written): (u32, &[bool]),
    reflections: bool,
) {
    for target in [
        LightingTarget::Colour,
        LightingTarget::Environment,
        LightingTarget::Factor,
    ] {
        let texture = frame.feature_target(FeatureTarget::DeferredLighting(target));
        if target != LightingTarget::Colour && !reflections {
            assert!(texture.is_none(), "{target:?} without reflections");
            continue;
        }
        let texture =
            texture.unwrap_or_else(|| panic!("reflections {reflections}: {target:?} is missing"));
        for (pixel, texel) in halves(&read(context, texture)).chunks_exact(4).enumerate() {
            if !covered(pixel, width) {
                if !written[pixel] {
                    assert!(
                        texel.iter().all(|value| value.to_bits() == 0),
                        "reflections {reflections}: {target:?} pixel {pixel} is unwritten but lit"
                    );
                }
            } else if target == LightingTarget::Colour {
                assert_eq!(
                    texel[3].to_bits(),
                    1.0_f32.to_bits(),
                    "reflections {reflections}: pixel {pixel} coverage"
                );
                assert!(
                    texel[..3].iter().any(|value| *value > 0.0),
                    "reflections {reflections}: pixel {pixel} is unlit"
                );
            }
        }
    }
}

#[test]
fn gpu_same_frame_depth_fit_captures_the_gbuffer_and_lighting_targets() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let limits = context.adapter().limits();
    println!(
        "adapter limits: max_color_attachments {} max_color_attachment_bytes_per_sample {}",
        limits.max_color_attachments, limits.max_color_attachment_bytes_per_sample
    );
    let view = view();
    let width = view.viewport().width;
    for reflections in [false, true] {
        let mut source = with_prepass(&scene(&[[0, 0, 1_000]], 600_000, None), true);
        if reflections {
            source = reflecting(&source);
        }
        // Both fixtures complete metadata depth before the synthetic producer and lighting.
        let frame = depth_fitted_frame(&context, &source, &view, true)
            .unwrap_or_else(|error| panic!("the depth-fitted frame failed: {error}"));
        let written = assert_gbuffer_written(
            &context,
            &frame,
            width,
            &format!("reflections {reflections}"),
        );
        assert_lighting_written(&context, &frame, (width, &written), reflections);
    }
}

fn capture_producer(context: &GpuContext, source: &RenderScene, prepass: bool) -> Captured {
    let view = view();
    let mut frame = frame(context, source, &view, SceneOpaquePath::GBUFFER_OBSERVATION);
    let depth = prepass.then(|| frame.targets().depth().texture().format());
    frame
        .register(
            context.device(),
            Box::new(producer(&view, source.effects().unwrap(), depth)),
        )
        .unwrap();
    capture_frame(context, source, &view, settings(), &mut frame).unwrap()
}

#[test]
fn gpu_shadowed_observation_keeps_the_forward_frame_and_lights_only_written_pixels() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for terrain in [false, true] {
        let mut source = scene(&[[0, 0, 1_000]], 600_000, None);
        if terrain {
            source = super::terrain::terrain_scene(&source, false, false);
        }
        let source = with_prepass(&source, true);
        let expected = capture(&context, &source, &view(), SceneOpaquePath::Forward).unwrap();
        assert!(expected.lit.is_none());
        let idle = capture(
            &context,
            &source,
            &view(),
            SceneOpaquePath::GBUFFER_OBSERVATION,
        )
        .unwrap();
        assert!(
            idle.colour == expected.colour,
            "terrain {terrain}: the observed frame differs"
        );
        let drawn = capture_producer(&context, &source, true);
        assert!(
            drawn.colour == expected.colour,
            "terrain {terrain}: the lit frame differs"
        );
        assert_producer_lighting(&drawn, terrain);
    }
}

fn assert_producer_lighting(drawn: &Captured, terrain: bool) {
    let width = view().viewport().width;
    let lit = drawn.lit.as_ref().unwrap();
    let written = drawn.g3.as_ref().unwrap();
    let mut shaded = Vec::new();
    for (pixel, texel) in lit.chunks_exact(4).enumerate() {
        if covered(pixel, width) {
            assert_eq!(
                texel[3].to_bits(),
                1.0_f32.to_bits(),
                "terrain {terrain}: pixel {pixel} coverage"
            );
            shaded.push(texel[0]);
        } else if written[pixel] == 0 {
            assert!(
                texel.iter().all(|value| value.to_bits() == 0),
                "terrain {terrain}: pixel {pixel} is unwritten but lit"
            );
        }
    }
    let brightest = shaded.iter().copied().fold(0.0_f32, f32::max);
    let darkest = shaded.iter().copied().fold(f32::INFINITY, f32::min);
    assert!(darkest > 0.0, "terrain {terrain}: ambient light");
    assert!(
        darkest < 0.9 * brightest,
        "terrain {terrain}: no shadow in the lit target ({darkest} {brightest})"
    );
}

#[test]
fn gpu_shadowed_synthetic_producer_without_prepass_stores_but_lights_nothing() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = with_prepass(&scene(&[[0, 0, 1_000]], 600_000, None), false);
    let expected = capture(&context, &source, &view(), SceneOpaquePath::Forward).unwrap();
    let idle = capture(
        &context,
        &source,
        &view(),
        SceneOpaquePath::GBUFFER_OBSERVATION,
    )
    .unwrap();
    assert_eq!(idle.colour, expected.colour);
    assert!(idle.lit.unwrap().iter().all(|value| value.to_bits() == 0));
    let drawn = capture_producer(&context, &source, false);
    assert_eq!(drawn.colour, expected.colour);
    assert!(drawn.g3.unwrap().iter().any(|code| *code != 0));
    assert!(drawn.lit.unwrap().iter().all(|value| value.to_bits() == 0));
}
