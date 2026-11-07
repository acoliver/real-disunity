use super::*;
use crate::compositor::{FrameRenderer, encode_scene};
use crate::draw::{EffectEnabled, PassExecution, PassInstallation, ReceiverBinding};
use worldrunner_render_post::ToneMap;

#[test]
fn gpu_reflection_only_receiver_observations_match_with_and_without_shadows() {
    let context = context();
    for terrain in [false, true] {
        let base = scene(&[[0, 0, 1_000]], 600_000, None);
        let base = if terrain {
            super::terrain::terrain_scene(&base, false, true)
        } else {
            base
        };
        let effects = base
            .effects()
            .unwrap()
            .clone()
            .with_ssr(SceneSsr {
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let source = RenderScene::new(SceneParts {
            resources: base.resources().clone(),
            geometry: base.geometry().clone(),
            terrain: base.terrain().cloned(),
            effects: Some(effects),
        })
        .unwrap();
        let mut frame = reflection_frame(&context, &source);
        let mut resident = ShadowedScene::with_target(
            &context,
            &source,
            settings(),
            wgpu::TextureFormat::Rgba16Float,
            1,
        )
        .unwrap();
        assert!(!resident.pipelines.ssao);
        super::ssao::encode(&context, &mut resident, &mut frame).unwrap();
        let shadowed = resident.gpu.statistics().effects.ssao;
        assert_eq!(shadowed.installation, PassInstallation::Installed);
        assert_eq!(shadowed.enabled, EffectEnabled::Disabled);
        assert_eq!(shadowed.receivers.meshes, ReceiverBinding::Bound);
        assert_eq!(
            shadowed.receivers.terrain,
            ReceiverBinding::from_bound(terrain)
        );
        assert_eq!(unshadowed(&context, &source, &mut frame), shadowed);
    }
}

fn unshadowed(
    context: &GpuContext,
    source: &RenderScene,
    frame: &mut FrameRenderer,
) -> crate::draw::SsaoStatistics {
    let pipelines = ScenePipelines::new(context.device(), wgpu::TextureFormat::Rgba16Float);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), source, &pipelines).unwrap();
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
    let mut encoder = context.submission().begin(context.device()).unwrap();
    encode_scene(
        frame,
        context.device(),
        &mut encoder,
        (&mut gpu, &pipelines),
        (&color, &depth),
        FrameUniform::new(&view(), source.effects()).unwrap(),
    )
    .unwrap();
    encoder.finish().submit().unwrap();
    gpu.statistics().effects.ssao
}

fn reflection_frame(context: &GpuContext, source: &RenderScene) -> FrameRenderer {
    FrameRenderer::hdr(
        context,
        [view().viewport().width, view().viewport().height],
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap()
}

#[test]
fn gpu_interactive_depth_fit_observes_one_prepass() {
    depth_fit_observation(crate::shadows::sdsm::AdoptionPolicy::Interactive {
        latency_frames: 2,
        contraction_frames: 0,
    });
}

#[test]
fn gpu_same_frame_depth_fit_observes_one_prepass() {
    depth_fit_observation(crate::shadows::sdsm::AdoptionPolicy::SameFrameCapture);
}

fn depth_fit_observation(policy: crate::shadows::sdsm::AdoptionPolicy) {
    let context = context();
    let (mut frame, mut resident) = super::sdsm::screen_renderers(&context);
    resident
        .set_depth_fitting(
            &context,
            Some(crate::shadows::sdsm::SdsmSettings {
                policy,
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
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    resident
        .render_depth_fitted_with(&context, &view(), (&color, &depth), 3, &mut frame)
        .unwrap();
    assert_eq!(
        resident.gpu.statistics().effects.ssao.prepass,
        PassExecution::Ran
    );
    crate::scene_metadata::EXECUTED_PASSES.with_borrow(|passes| {
        assert_eq!(
            passes
                .iter()
                .filter(|&&slot| slot == PassSlot::DepthNormalPrepass)
                .count(),
            1
        );
    });
    skipped_frame(&context, &mut resident, &mut frame);
}

fn skipped_frame(context: &GpuContext, resident: &mut ShadowedScene, frame: &mut FrameRenderer) {
    use worldrunner_render_post::{BorrowedFeature, PassDeclaration};
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
    let depth_mode = if depth.format().has_stencil_aspect() {
        crate::renderer::depth::DepthMode::Shared
    } else {
        crate::renderer::depth::DepthMode::Independent
    };
    let pipelines = ScenePipelines::new(context.device(), wgpu::TextureFormat::Rgba16Float)
        .for_depth(depth_mode);
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(frame.color_attachment(color.resource(), &color));
    pass.depth = Some(crate::compositor::attachment(
        depth.resource(),
        &depth,
        true,
    ));
    let mut feature = crate::compositor::SceneFeature {
        publications: crate::draw::DrawPublications::default(),
        frame: FrameUniform::new(&view(), Some(&resident.effects)).unwrap(),
        gpu: &mut resident.gpu,
        pipelines: &pipelines,
        screen: None,
        programs: None,
    };
    let mut encoder = context.submission().begin(context.device()).unwrap();
    FrameRenderer::new(context.device())
        .encode(
            context.device(),
            &mut encoder,
            &[pass],
            &[color, depth],
            &mut [BorrowedFeature {
                declaration: crate::compositor::SceneFeature::declaration(&pipelines),
                prepared: &mut feature,
            }],
        )
        .unwrap();
    encoder.finish().submit().unwrap();
    assert_eq!(
        resident.gpu.statistics().effects.ssao.prepass,
        PassExecution::Skipped
    );
}
