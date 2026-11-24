//! Public regression for camera-only TAA on the stock depth prefix.
#[path = "stock_composition/fixture.rs"]
mod fixture;

use fixture::{SIZE, view};
use std::time::Duration;
use worldrunner_render_plan::{RenderScene, SceneAntiAlias, SceneEffects};
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, FramePlan, ResourceRef, ToneMap, color_texture,
};
use worldrunner_render_scene::draw::PreparedScene;
use worldrunner_render_scene::{
    SceneView,
    compositor::{FrameRenderer, PrefixContribution, PrefixScene, prepare_stock_scene},
    device::GpuContext,
    draw::SceneGpu,
    frame::FrameUniform,
    offscreen::OffscreenTarget,
    renderer::ScenePipelines,
    test_support::SceneTestSession,
    wgpu,
};

struct CameraTaaFrame {
    context: GpuContext,
    renderer: FrameRenderer,
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    output: wgpu::Texture,
    effects: SceneEffects,
}

impl CameraTaaFrame {
    fn new(context: GpuContext) -> Self {
        let source = camera_taa_scene();
        let effects = source.effects().unwrap().clone();
        let renderer = FrameRenderer::hdr(
            &context,
            SIZE,
            OffscreenTarget::COLOR_FORMAT,
            1,
            ToneMap::Reinhard,
            Some(&effects),
        )
        .unwrap();
        let pipelines =
            ScenePipelines::with_ssao(context.device(), wgpu::TextureFormat::Rgba16Float, 1)
                .unwrap();
        let gpu = SceneGpu::upload(
            context.device(),
            context.submission(),
            PreparedScene::new(&source).unwrap(),
            &pipelines,
            None,
        )
        .unwrap();
        let output = color_texture(
            context.device(),
            "camera-only TAA output",
            SIZE,
            OffscreenTarget::COLOR_FORMAT,
        );
        Self {
            context,
            renderer,
            gpu,
            pipelines,
            output,
            effects,
        }
    }

    fn encode_frame(&mut self, camera: &SceneView) {
        let device = self.context.device().clone();
        let authority = self.context.submission().clone();
        let uniform = FrameUniform::new(camera, Some(&self.effects)).unwrap();
        let mut encoder = authority.begin(&device).unwrap();
        self.encode_prefix(&device, &mut encoder, &uniform);
        self.encode_suffix(&device, &mut encoder, &uniform);
        self.renderer
            .encode_output(
                &mut encoder,
                &self
                    .output
                    .create_view(&wgpu::TextureViewDescriptor::default()),
            )
            .unwrap();
        encoder.finish().submit().unwrap();
    }

    fn complete_velocity_frame(&mut self, camera: &SceneView) {
        let device = self.context.device().clone();
        let authority = self.context.submission().clone();
        let uniform = FrameUniform::new(camera, Some(&self.effects)).unwrap();
        let mut encoder = authority.begin(&device).unwrap();
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            self.renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            self.renderer.targets().depth().texture(),
        )
        .unwrap();
        self.renderer
            .encoding()
            .encode_scene(
                &device,
                &mut encoder,
                (&mut self.gpu, &self.pipelines),
                (&color, &depth),
                uniform,
            )
            .unwrap();
        self.renderer
            .encode_output(
                &mut encoder,
                &self
                    .output
                    .create_view(&wgpu::TextureViewDescriptor::default()),
            )
            .unwrap();
        encoder.finish().submit().unwrap();
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })
            .unwrap();
    }

    fn encode_prefix(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        uniform: &FrameUniform,
    ) {
        let prefix = self
            .renderer
            .stock_depth_prefix(
                device,
                PrefixScene {
                    gpu: &mut self.gpu,
                    pipelines: &self.pipelines,
                },
            )
            .unwrap();
        assert_no_scene_velocity(&prefix.plan());
        prefix
            .encode(
                &mut self.renderer,
                device,
                encoder,
                PrefixScene {
                    gpu: &mut self.gpu,
                    pipelines: &self.pipelines,
                },
                *uniform,
                PrefixContribution {
                    plan: &empty_plan(),
                    features: &mut [],
                },
            )
            .unwrap();
    }

    fn encode_suffix(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        uniform: &FrameUniform,
    ) {
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            self.renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            self.renderer.targets().depth().texture(),
        )
        .unwrap();
        let mut stock = prepare_stock_scene(
            &mut self.renderer,
            device,
            encoder,
            (&mut self.gpu, &self.pipelines),
            (&color, &depth),
            *uniform,
        )
        .unwrap();
        assert_no_scene_velocity(&stock.plan());
        let plan = stock.plan();
        let (passes, views) = (plan.passes.to_vec(), plan.views.to_vec());
        let (declaration, frame) = (stock.declaration(), stock.frame());
        let offscreen = stock.take_offscreen_operations();
        self.renderer
            .encode_frame(
                device,
                encoder,
                (&passes, &views),
                offscreen.as_ref().map(|work| work as _),
                &mut [BorrowedFeature {
                    declaration,
                    prepared: &mut stock,
                }],
                &(frame, None),
            )
            .unwrap();
    }
}

/// The quad scene under temporal anti-aliasing.
fn camera_taa_scene() -> RenderScene {
    let effects = fixture::effects_with(SceneAntiAlias::Taa {
        history_micro: 900_000,
        clip_scale_micro: 1_000_000,
    });
    let source = fixture::scene();
    source
        .with_objects_and_effects(source.geometry().objects().to_vec(), Some(effects))
        .unwrap()
}

fn assert_no_scene_velocity(plan: &FramePlan<'_>) {
    let velocity = ResourceRef::new("scene velocity");
    for pass in plan.passes {
        assert_ne!(
            pass.color.as_ref().map(|attachment| &attachment.resource),
            Some(&velocity)
        );
        assert!(
            pass.additional_colors
                .iter()
                .all(|attachment| attachment.resource != velocity),
            "{} has no scene-velocity colour attachment",
            pass.entry()
        );
    }
}

fn empty_plan() -> FramePlan<'static> {
    FramePlan {
        passes: &[],
        views: &[],
        requested: &[],
        operations: &[],
    }
}

#[test]
fn public_stock_depth_prefix_plain_pipelines_keep_camera_only_taa() {
    let _session = SceneTestSession::acquire();
    let context = GpuContext::shared_for_test_with_features(wgpu::Features::empty())
        .expect("camera-only TAA regression requires a real adapter; never skip");
    let validation = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let mut frame = CameraTaaFrame::new(context);
    for camera in [view(0), view(1_500)] {
        let jitter = frame.renderer.projection_jitter_index();
        frame.encode_frame(&camera);
        assert_eq!(frame.renderer.projection_jitter_index(), jitter + 1);
        assert!(frame.renderer.has_temporal_history());
    }
    let draws = frame.gpu.statistics().passes;
    assert!(draws.prepass_objects.meshes.calls > 0, "{draws:?}");
    assert_eq!(draws.prepass_objects.meshes.instances, 1, "{draws:?}");
    assert!(draws.opaque_objects.meshes.calls > 0, "{draws:?}");
    frame
        .context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
    assert!(pollster::block_on(validation.pop()).is_none());
}

#[test]
fn public_stock_depth_prefix_drops_completed_velocity_for_plain_pipelines() {
    let _session = SceneTestSession::acquire();
    let context = GpuContext::shared_for_test_with_features(wgpu::Features::empty())
        .expect("velocity transition regression requires a real adapter; never skip");
    let device = context.device().clone();
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut frame = CameraTaaFrame::new(context);
    frame.pipelines =
        ScenePipelines::with_ssao_taa_velocity(&device, wgpu::TextureFormat::Rgba16Float, 1)
            .unwrap();
    assert!(frame.pipelines.writes_velocity());
    frame.complete_velocity_frame(&view(0));
    assert_eq!(frame.renderer.projection_jitter_index(), 1);
    assert!(frame.renderer.has_temporal_history());
    let velocity = frame.renderer.targets().velocity_texture().unwrap().clone();
    frame.pipelines =
        ScenePipelines::with_ssao(&device, wgpu::TextureFormat::Rgba16Float, 1).unwrap();
    assert!(!frame.pipelines.writes_velocity());
    for camera in [view(1_500), view(0)] {
        let jitter = frame.renderer.projection_jitter_index();
        frame.encode_frame(&camera);
        assert_eq!(frame.renderer.projection_jitter_index(), jitter + 1);
        assert!(frame.renderer.has_temporal_history());
        assert_eq!(frame.renderer.targets().velocity_texture(), Some(&velocity));
    }
    let draws = frame.gpu.statistics().passes;
    assert!(draws.prepass_objects.meshes.calls > 0, "{draws:?}");
    assert_eq!(draws.prepass_objects.meshes.instances, 1, "{draws:?}");
    assert!(draws.opaque_objects.meshes.calls > 0, "{draws:?}");
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
    assert!(pollster::block_on(validation.pop()).is_none());
}
