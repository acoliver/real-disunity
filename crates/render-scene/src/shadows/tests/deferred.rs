//! The G-buffer observation through the shadowed scene: the lighting pass draws with the
//! shadow receivers' sets, the visible frame stays the forward frame, and only pixels a
//! producer wrote are lit.
use super::*;
use crate::compositor::{FeatureTarget, FrameRenderer};
use crate::gbuffer::lighting::LightingTarget;
use crate::gbuffer::reference::Surface;
use crate::terrain::deferred_tests::{BLOCK, COLUMNS, Producer, halves};
use worldrunner_render_post::{FrameConfig, Readback, ToneMap};

/// Columns of blocks the producer writes, from the left edge.
const COVERED: u32 = 20;

fn read(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    Readback::new(
        context.device(),
        FrameConfig::new(
            [texture.width(), texture.height()],
            wgpu::TextureFormat::Rgba16Float,
            1,
        ),
        2_000_000,
    )
    .unwrap()
    .read(context.device(), context.submission(), texture)
    .unwrap()
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
        let uniform = crate::frame::FrameUniform::new(&view, Some(&effects)).unwrap();
        let producer = Producer {
            blocks: vec![up; usize::try_from(COLUMNS * COLUMNS).unwrap()],
            view: glam::Mat3::from_mat4(uniform.view),
            covered: COVERED,
        };
        frame
            .register(context.device(), Box::new(producer))
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
