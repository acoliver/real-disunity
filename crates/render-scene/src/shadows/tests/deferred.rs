//! The G-buffer observation through the shadowed scene: with the depth prepass the scene's
//! producers store the ground and the caster, the lighting pass draws with the shadow
//! receivers' sets and lights exactly the stored pixels, and the visible frame stays the forward
//! frame. Without the prepass the lighting pass, which runs before Opaque, lights nothing.
use super::*;
use crate::compositor::{FeatureTarget, FrameRenderer};
use crate::gbuffer::GbufferTarget;
use crate::gbuffer::lighting::LightingTarget;
use crate::terrain::deferred_tests::halves;
use worldrunner_render_post::{FrameConfig, Readback, ToneMap};

pub(super) fn read(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    Readback::new(
        context.device(),
        FrameConfig::new([texture.width(), texture.height()], texture.format(), 1),
        2_000_000,
    )
    .unwrap()
    .read(context.device(), context.submission(), texture)
    .unwrap()
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
fn with_prepass(source: &RenderScene, prepass: bool) -> RenderScene {
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
}

pub(super) fn capture(
    context: &GpuContext,
    source: &RenderScene,
    view: &SceneView,
    path: SceneOpaquePath,
) -> Result<Captured, ShadowError> {
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
    renderer.encode_with(
        context,
        view,
        (&color, &depth),
        &mut encoder,
        Some(&mut frame),
    )?;
    frame
        .encode_output(&mut encoder, output.color_view())
        .unwrap();
    output.encode_readback(&mut encoder).unwrap();
    let submission = encoder.finish().submit().unwrap();
    output.read_encoded(context, submission).unwrap();
    Ok(Captured {
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
    })
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
            let lit = observed.lit.unwrap();
            let g3 = observed.g3.unwrap();
            let mut shaded = Vec::new();
            for (pixel, (texel, code)) in lit.chunks_exact(4).zip(&g3).enumerate() {
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
                continue;
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
    }
}
