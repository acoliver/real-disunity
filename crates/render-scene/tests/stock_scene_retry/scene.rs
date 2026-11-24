use super::fixture;
use worldrunner_render_plan::RenderScene;
use worldrunner_render_post::{AttachmentView, FrameError, ResourceRef, ToneMap, color_texture};
use worldrunner_render_scene::draw::PreparedScene;
use worldrunner_render_scene::{
    compositor::{FeatureTarget, FrameRenderer},
    device::GpuContext,
    draw::{ObjectRecording, SceneGpu},
    frame::FrameUniform,
    offscreen::OffscreenTarget,
    renderer::ScenePipelines,
    wgpu,
};

pub(super) struct Scene {
    pub frame: FrameRenderer,
    pub gpu: SceneGpu,
    pub output: wgpu::Texture,
    pub(super) pipelines: ScenePipelines,
    pub(super) uniform: FrameUniform,
}

impl Scene {
    pub(super) fn new(context: &GpuContext, source: &RenderScene) -> Self {
        let view = fixture::view(source);
        let pipelines = ScenePipelines::with_taa_velocity(
            context.device(),
            wgpu::TextureFormat::Rgba16Float,
            1,
        )
        .unwrap();
        let gpu = SceneGpu::upload(
            context.device(),
            context.submission(),
            PreparedScene::new(source).unwrap(),
            &pipelines,
            None,
        )
        .unwrap();
        let mut frame = FrameRenderer::hdr(
            context,
            [view.viewport().width, view.viewport().height],
            OffscreenTarget::COLOR_FORMAT,
            1,
            ToneMap::Reinhard,
            source.effects(),
        )
        .unwrap();
        frame.set_frame_time(std::time::Duration::from_secs_f32(0.125));
        Self {
            frame,
            gpu,
            output: color_texture(
                context.device(),
                "public stock retry output",
                [view.viewport().width, view.viewport().height],
                OffscreenTarget::COLOR_FORMAT,
            ),
            pipelines,
            uniform: FrameUniform::new(&view, source.effects()).unwrap(),
        }
    }

    pub(super) fn move_retained_instance(&mut self) {
        let handle = self.gpu.instance_handles().next().unwrap();
        let mut moved = *self.gpu.instance(handle).unwrap().0.transform();
        moved.position_millimeters[0] += 250;
        self.gpu.set_instance_transform(handle, moved).unwrap();
    }

    pub(super) fn complete(&mut self, context: &GpuContext) {
        let jitter = self.frame.projection_jitter_index();
        self.try_complete(context).unwrap();
        assert_eq!(self.frame.projection_jitter_index(), jitter + 1);
        assert!(self.frame.has_temporal_history());
        let draws = self.gpu.statistics().passes.prepass_objects;
        assert_eq!(draws.recording, ObjectRecording::Compacted);
        assert_eq!(draws.meshes.calls, 2);
        assert_eq!(draws.meshes.instances, 3);
    }

    pub(super) fn try_complete(&mut self, context: &GpuContext) -> Result<(), FrameError> {
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            self.frame.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            self.frame.targets().depth().texture(),
        )
        .unwrap();
        let mut encoder = context.submission().begin(context.device()).unwrap();
        self.frame.encoding().encode_scene(
            context.device(),
            &mut encoder,
            (&mut self.gpu, &self.pipelines),
            worldrunner_render_scene::compositor::ColorDepth {
                color: &color,
                depth: &depth,
            },
            self.uniform,
        )?;
        self.frame
            .encode_output(
                &mut encoder,
                &self
                    .output
                    .create_view(&wgpu::TextureViewDescriptor::default()),
            )
            .unwrap();
        encoder.finish().submit().unwrap();
        Ok(())
    }

    pub(super) fn seed_confidence(&self, context: &GpuContext) {
        let capture = self
            .frame
            .feature_capture(FeatureTarget::SsrConfidence)
            .unwrap();
        assert_eq!(capture.texture.format(), wgpu::TextureFormat::Rgba16Float);
        let view = capture.texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: capture.layer,
            array_layer_count: Some(1),
            ..Default::default()
        });
        let mut encoder = context.submission().begin(context.device()).unwrap();
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            a: 0.5,
                            ..wgpu::Color::BLACK
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        encoder.finish().submit().unwrap();
    }
}
