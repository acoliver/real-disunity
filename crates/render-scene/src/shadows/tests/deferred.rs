//! The G-buffer observation through the shadowed scene: the lighting pass draws with the
//! shadow receivers' sets, the visible frame stays the forward frame, and only pixels a
//! producer wrote are lit. The same-frame depth fit records the frame as one capture, which
//! must declare every G-buffer and lighting target the frame writes.
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

fn read(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    read_as(context, texture, wgpu::TextureFormat::Rgba16Float)
}

/// A producer that writes a grey upward-facing surface over the first [`COVERED`] columns of
/// blocks, encoded for `view` under `effects`.
fn producer(view: &SceneView, effects: &SceneEffects) -> Producer {
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
    }
}

/// Whether the pixel at `index` lies in the columns the producer writes.
fn covered(index: usize, width: u32) -> bool {
    u32::try_from(index).unwrap() % width < COVERED * BLOCK
}

/// The scene colour of one shadowed frame, and with the observation its lighting colour.
fn capture(
    context: &GpuContext,
    source: &RenderScene,
    path: SceneOpaquePath,
    produce: bool,
) -> (Vec<u8>, Option<Vec<f32>>) {
    let view = view();
    let size = [view.viewport().width, view.viewport().height];
    let effects = source
        .effects()
        .unwrap()
        .clone()
        .with_opaque_path(path)
        .unwrap();
    let mut frame = FrameRenderer::hdr(
        context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        Some(&effects),
    )
    .unwrap();
    if produce {
        frame
            .register(context.device(), Box::new(producer(&view, &effects)))
            .unwrap();
    }
    let mut renderer = ShadowedScene::with_target(
        context,
        source,
        settings(),
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
    renderer
        .encode_with(
            context,
            &view,
            (&color, &depth),
            &mut encoder,
            Some(&mut frame),
        )
        .unwrap();
    frame
        .encode_output(&mut encoder, output.color_view())
        .unwrap();
    output.encode_readback(&mut encoder).unwrap();
    let submission = encoder.finish().submit().unwrap();
    output.read_encoded(context, submission).unwrap();
    let raw = read(context, frame.targets().texture());
    let lit = frame
        .feature_target(FeatureTarget::DeferredLighting(LightingTarget::Colour))
        .map(|texture| halves(&read(context, texture)));
    (raw, lit)
}

#[test]
fn gpu_shadowed_observation_keeps_the_forward_frame_and_lights_only_written_pixels() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let width = view().viewport().width;
    for terrain in [false, true] {
        let mut source = scene(&[[0, 0, 1_000]], 600_000, None);
        if terrain {
            source = super::terrain::terrain_scene(&source, false, false);
        }
        let (expected, none) = capture(&context, &source, SceneOpaquePath::Forward, false);
        assert!(none.is_none());
        let (idle, unlit) = capture(
            &context,
            &source,
            SceneOpaquePath::GBUFFER_OBSERVATION,
            false,
        );
        assert!(
            idle == expected,
            "terrain {terrain}: the observed frame differs"
        );
        assert!(unlit.unwrap().iter().all(|value| value.to_bits() == 0));
        let (colour, lit) = capture(
            &context,
            &source,
            SceneOpaquePath::GBUFFER_OBSERVATION,
            true,
        );
        assert!(
            colour == expected,
            "terrain {terrain}: the lit frame differs"
        );
        let lit = lit.unwrap();
        let mut shaded = Vec::new();
        for (pixel, texel) in lit.chunks_exact(4).enumerate() {
            let x = u32::try_from(pixel).unwrap() % width;
            if x >= COVERED * BLOCK {
                assert!(
                    texel.iter().all(|value| value.to_bits() == 0),
                    "terrain {terrain}: pixel {pixel} is unwritten but lit"
                );
            } else {
                assert_eq!(
                    texel[3].to_bits(),
                    1.0_f32.to_bits(),
                    "terrain {terrain}: pixel {pixel} coverage"
                );
                shaded.push(texel[0]);
            }
        }
        // The caster's shadow on the written ground: some written pixels receive less sun.
        let brightest = shaded.iter().copied().fold(0.0_f32, f32::max);
        let darkest = shaded.iter().copied().fold(f32::INFINITY, f32::min);
        assert!(darkest > 0.0, "terrain {terrain}: ambient light");
        assert!(
            darkest < 0.9 * brightest,
            "terrain {terrain}: no shadow in the lit target ({darkest} {brightest})"
        );
    }
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
/// same-frame depth fit, which records the frame as one capture.
fn depth_fitted_frame(
    context: &GpuContext,
    source: &RenderScene,
    view: &SceneView,
) -> FrameRenderer {
    let size = [view.viewport().width, view.viewport().height];
    let effects = source
        .effects()
        .unwrap()
        .clone()
        .with_opaque_path(SceneOpaquePath::GBUFFER_OBSERVATION)
        .unwrap();
    let mut frame = FrameRenderer::hdr(
        context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        Some(&effects),
    )
    .unwrap();
    frame
        .register(context.device(), Box::new(producer(view, &effects)))
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
        .render_depth_fitted_with(context, view, (&color, &depth), 0, &mut frame)
        .unwrap_or_else(|error| panic!("the depth-fitted frame failed: {error:?}"))
        .expect("the same-frame fit adopts");
    assert_eq!((fit.source_frame, fit.adopted_frame), (0, 0));
    frame
}

/// Every G-buffer target is zero outside the producer's columns; base colour and the
/// geometric normal are set inside them.
fn assert_gbuffer_written(context: &GpuContext, frame: &FrameRenderer, width: u32, label: &str) {
    for target in GbufferTarget::ALL {
        let texture = frame
            .feature_target(FeatureTarget::Gbuffer(target))
            .unwrap_or_else(|| panic!("{label}: {target:?} is not allocated"));
        let bytes = read_as(context, texture, target.format());
        let texel = usize::try_from(target.format().block_copy_size(None).unwrap()).unwrap();
        for (pixel, value) in bytes.chunks_exact(texel).enumerate() {
            let written = value.iter().any(|&byte| byte != 0);
            if !covered(pixel, width) {
                assert!(
                    !written,
                    "{label}: {target:?} pixel {pixel} is unwritten but set"
                );
            } else if matches!(target, GbufferTarget::G1 | GbufferTarget::G3) {
                assert!(written, "{label}: {target:?} pixel {pixel} was not written");
            }
        }
    }
}

/// Every lighting target the frame allocates is zero outside the producer's columns, and the
/// lit colour covers and lights the pixels inside them. The reflection products exist exactly
/// with `reflections`.
fn assert_lighting_written(
    context: &GpuContext,
    frame: &FrameRenderer,
    width: u32,
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
                assert!(
                    texel.iter().all(|value| value.to_bits() == 0),
                    "reflections {reflections}: {target:?} pixel {pixel} is unwritten but lit"
                );
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
        let mut source = scene(&[[0, 0, 1_000]], 600_000, None);
        if reflections {
            source = reflecting(&source);
        }
        let frame = depth_fitted_frame(&context, &source, &view);
        assert_gbuffer_written(
            &context,
            &frame,
            width,
            &format!("reflections {reflections}"),
        );
        assert_lighting_written(&context, &frame, width, reflections);
    }
}
