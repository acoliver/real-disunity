//! Depth fitting without SSAO or SSR still draws the prepass; Opaque must reuse its depth.
use super::*;
use crate::compositor::FrameRenderer;
use crate::renderer::depth::DepthMode;
use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
use worldrunner_render_post::{Readback, ToneMap};

fn fitting_source() -> RenderScene {
    let base = scene(&[[0, 0, 1000], [400, 500, 800]], 600_000, None);
    let effects = SceneEffects::new(fixture::effects_parts(600_000))
        .unwrap()
        .with_depth_fitting(SceneDepthFitting {
            near_step_mm: 100,
            far_step_mm: 1000,
            minimum_span_mm: 1000,
            minimum_span_policy: SceneDepthMinimumSpan::ExtendNear,
            active_maximum_mm: None,
            splits: SceneDepthSplits::Practical {
                lambda_micro: 500_000,
            },
            adoption: SceneDepthAdoption::FrameIndexed {
                latency_frames: 2,
                contraction_frames: 0,
            },
        })
        .unwrap();
    assert!(effects.ssao().is_none() && effects.ssr().is_none());
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

/// Renders one same-frame fitted capture and returns the scene colour. `independent` gives the
/// frame its own Opaque depth, the reference the paired depth must reproduce.
fn fitted_color(context: &GpuContext, independent: bool) -> Vec<u8> {
    let source = fitting_source();
    let size = [view().viewport().width, view().viewport().height];
    let mut frame = FrameRenderer::hdr(
        context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    if independent {
        frame.independent_depth_for_test(context.device());
    }
    let mut renderer = ShadowedScene::with_target(
        context,
        &source,
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
                range: policy(),
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
    let metadata = frame
        .ssao
        .as_ref()
        .expect("depth fitting installs the frame's prepass metadata");
    assert_eq!(metadata.depth == *depth.texture(), !independent);
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    let fit = renderer
        .render_depth_fitted_with(context, &view(), (&color, &depth), 0, &mut frame)
        .unwrap()
        .expect("a same-frame capture adopts its own fit");
    assert_eq!((fit.source_frame, fit.adopted_frame), (0, 0));
    crate::scene_metadata::EXECUTED_PASSES.with_borrow(|passes| {
        for slot in [
            worldrunner_render_post::PassSlot::DepthNormalPrepass,
            worldrunner_render_post::PassSlot::DepthPyramid(0),
        ] {
            assert_eq!(
                passes.iter().filter(|&&pass| pass == slot).count(),
                1,
                "duplicate or missing producer: {slot:?}"
            );
        }
    });
    assert_eq!(
        frame.forward_depth(&renderer.gpu, &depth),
        if independent {
            DepthMode::Independent
        } else {
            DepthMode::Reuse
        }
    );
    let mut readback = Readback::new(
        context.device(),
        worldrunner_render_post::FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        16 * 1024 * 1024,
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    readback.encode(&mut encoder, color.texture()).unwrap();
    let submission = encoder.finish().submit().unwrap();
    readback
        .read_encoded(context.device(), Some(submission))
        .unwrap()
}

#[test]
fn gpu_depth_fitting_without_screen_effects_reuses_its_prepass_depth_in_opaque() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let reuse = fitted_color(&context, false);
    assert!(reuse.chunks_exact(8).any(|pixel| pixel[..6] != [0; 6]));
    assert!(
        reuse == fitted_color(&context, true),
        "paired depth must shade what independent depth shades"
    );
}

/// One same-frame fitted capture with an ambient term replay, the passes it ran and, for the
/// computed reference, its filtered occlusion.
struct Fitted {
    color: Vec<u8>,
    terms: Vec<Vec<u8>>,
    passes: Vec<PassSlot>,
    fit: crate::shadows::sdsm::AdoptedFit,
    occlusion: Option<Vec<u8>>,
}

fn read(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    Readback::new(
        context.device(),
        worldrunner_render_post::FrameConfig::new(
            [texture.width(), texture.height()],
            texture.format(),
            1,
        ),
        16 * 1024 * 1024,
    )
    .unwrap()
    .read(context.device(), context.submission(), texture)
    .unwrap()
}

/// Renders the depth-fitting-only frame. `computed` gives it the metadata such frames had
/// before: neutral occlusion computed by the kernel and filter over the whole depth hierarchy.
fn depth_fitted_frame(context: &GpuContext, computed: bool) -> Fitted {
    use crate::compositor::FeatureTarget;
    use crate::compositor::terms::{CaptureTerm, TermCapture};
    let source = fitting_source();
    let size = [view().viewport().width, view().viewport().height];
    let mut frame = FrameRenderer::hdr(
        context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let screen = frame.ssao.as_ref().expect("depth fitting owns the prepass");
    assert!(!screen.passes.computes_occlusion());
    assert_eq!(screen.passes.depth_levels().len(), 1);
    assert_eq!(
        [
            screen.passes.filtered.width(),
            screen.passes.filtered.height()
        ],
        [1, 1]
    );
    assert!(
        frame
            .feature_target(FeatureTarget::AmbientOcclusion)
            .is_none()
    );
    if computed {
        frame.computed_neutral_occlusion_for_test(context.device());
        assert!(
            frame
                .feature_target(FeatureTarget::AmbientOcclusion)
                .is_some()
        );
    }
    let mut renderer = ShadowedScene::with_target(
        context,
        &source,
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
                range: policy(),
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
    let terms = [CaptureTerm::AmbientDiffuse, CaptureTerm::DirectDiffuse]
        .map(|term| TermCapture::new(context.device(), size, term));
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    let fit = renderer
        .render_depth_fitted_host_terms(
            context,
            &view(),
            (&color, &depth),
            (Some(0), true, &terms),
            &mut frame,
        )
        .unwrap()
        .expect("a same-frame capture adopts its own fit");
    let passes = crate::scene_metadata::EXECUTED_PASSES.with_borrow(Clone::clone);
    assert_eq!(frame.forward_depth(&renderer.gpu, &depth), DepthMode::Reuse);
    let occlusion = computed.then(|| {
        read(
            context,
            &frame.ssao.as_ref().expect("metadata").passes.filtered,
        )
    });
    Fitted {
        color: read(context, color.texture()),
        terms: terms
            .iter()
            .map(|term| read(context, &term.texture))
            .collect(),
        passes,
        fit,
        occlusion,
    }
}

/// The stages a depth-fitting-only frame no longer runs: nothing reads them there.
fn occlusion_stage(slot: PassSlot) -> bool {
    matches!(
        slot,
        PassSlot::Ssao
            | PassSlot::SsaoBlur
            | PassSlot::SsaoBlurHorizontal
            | PassSlot::OcclusionMetadata(_)
    ) || matches!(slot, PassSlot::DepthPyramid(level) if level > 0)
}

/// A frame whose prepass only shadow depth fitting reads skips the occlusion kernel, its filter
/// and the depth hierarchy past level 0, and binds a constant neutral occlusion instead. It must
/// shade, replay its ambient term and fit its cascades exactly as the frame that computed them.
#[test]
fn gpu_depth_fitting_alone_skips_unread_occlusion_stages_with_identical_results() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let skipped = depth_fitted_frame(&context, false);
    let reference = depth_fitted_frame(&context, true);
    let occlusion = reference.occlusion.as_ref().expect("computed occlusion");
    assert!(!occlusion.is_empty());
    assert!(
        occlusion
            .chunks_exact(4)
            .all(|texel| texel == 1.0f32.to_le_bytes()),
        "the neutral kernel and filter write exactly 1.0 at every pixel"
    );
    for slot in [
        PassSlot::DepthNormalPrepass,
        PassSlot::DepthPyramid(0),
        PassSlot::Opaque,
    ] {
        assert_eq!(
            skipped.passes.iter().filter(|&&pass| pass == slot).count(),
            1,
            "{slot:?}"
        );
    }
    assert!(!skipped.passes.iter().copied().any(occlusion_stage));
    for slot in [
        PassSlot::DepthPyramid(1),
        PassSlot::Ssao,
        PassSlot::SsaoBlur,
    ] {
        assert!(reference.passes.contains(&slot), "reference ran {slot:?}");
    }
    let kept: Vec<_> = reference
        .passes
        .iter()
        .copied()
        .filter(|&slot| !occlusion_stage(slot))
        .collect();
    assert_eq!(skipped.passes, kept, "no other pass changed");
    assert!(
        skipped
            .color
            .chunks_exact(8)
            .any(|pixel| pixel[..6] != [0; 6])
    );
    assert!(skipped.color == reference.color, "scene colour differs");
    assert!(
        skipped.terms[0]
            .chunks_exact(8)
            .any(|pixel| pixel[..6] != [0; 6]),
        "the ambient term samples the occlusion and must not be empty"
    );
    for (index, (term, expected)) in skipped.terms.iter().zip(&reference.terms).enumerate() {
        assert!(term == expected, "term {index} differs");
    }
    assert_eq!(skipped.fit, reference.fit);
}

/// Frames that sample occlusion keep computing it when depth fitting also draws the prepass:
/// SSAO receivers, and reflections' application occlusion.
#[test]
fn gpu_depth_fitting_with_ssao_or_reflections_still_computes_occlusion() {
    use crate::compositor::FeatureTarget;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let fitting = *fitting_source().effects().unwrap().depth_fitting().unwrap();
    let ssao = SceneSsao {
        intensity_micro: 1_000_000,
        ..crate::ssr::neutral_ao()
    };
    let with_ssao = SceneEffects::new(SceneEffectsParts {
        ssao: Some(ssao),
        ..fixture::effects_parts(600_000)
    })
    .unwrap()
    .with_depth_fitting(fitting)
    .unwrap();
    let with_ssr = SceneEffects::new(fixture::effects_parts(600_000))
        .unwrap()
        .with_depth_fitting(fitting)
        .unwrap()
        .with_ssr(SceneSsr {
            enabled: true,
            ..Default::default()
        })
        .unwrap();
    for (effects, settings) in [(with_ssao, ssao), (with_ssr, crate::ssr::neutral_ao())] {
        let frame = FrameRenderer::hdr(
            &context,
            [64, 48],
            OffscreenTarget::COLOR_FORMAT,
            1,
            ToneMap::Reinhard,
            Some(&effects),
        )
        .unwrap();
        let screen = frame.ssao.as_ref().expect("prepass metadata");
        assert!(screen.passes.computes_occlusion());
        assert_eq!(screen.settings, settings);
        assert!(screen.passes.depth_levels().len() > 1);
        assert!(
            frame
                .feature_target(FeatureTarget::AmbientOcclusion)
                .is_some()
        );
    }
}
