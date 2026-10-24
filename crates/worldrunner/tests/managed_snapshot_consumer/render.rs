use super::{
    extraction::{Extraction, hash},
    native::{Fixture, Session},
    receipt::{ViewReceipt, provenance},
    stage::{FORMAT, Factory},
    store,
    view::{Target, canonical_position},
};
use worldrunner::scene::{
    gpu::{
        SceneView, device::GpuContext, draw::SceneGpu, dynamic::DynamicQuad, frame::FrameUniform,
        offscreen::OffscreenTarget, renderer::ScenePipelines, wgpu,
    },
    post,
    resources::CanonicalTextureStore,
};

pub struct Renderer<'a> {
    context: &'a GpuContext,
    pub target: Target,
    chain: post::PostChain<Factory>,
    pipelines: ScenePipelines,
    scene: SceneGpu,
    textures: CanonicalTextureStore,
}

impl<'a> Renderer<'a> {
    pub fn new(context: &'a GpuContext) -> Self {
        let target = Target::new();
        let chain = post::PostChain::new(
            context.device(),
            post::FrameConfig::new(target.viewport, FORMAT, 1),
            post::Resources::default(),
            Factory,
        )
        .unwrap();
        let pipelines = ScenePipelines::new(context.device(), FORMAT);
        let scene = SceneGpu::empty(context.device(), context.queue(), &pipelines);
        let textures = CanonicalTextureStore::new(context.device(), context.queue());
        Self {
            context,
            target,
            chain,
            pipelines,
            scene,
            textures,
        }
    }

    pub fn capture(
        &mut self,
        fixture: &Fixture,
        session: &Session,
        source: &Extraction,
        object: u64,
    ) -> Result<ViewReceipt, String> {
        source.validate(fixture, session, true)?;
        assert_eq!(
            source.managed,
            super::observation::Observation::read(fixture, session)
        );
        let view = self.prepare(source, object)?;
        let resolved_view = self.target.resolve();
        let (pixels, submission) = self.render(&view, || {});
        source.validate(fixture, session, true)?;
        assert_eq!(
            source.managed,
            super::observation::Observation::read(fixture, session)
        );
        let gpu_completion_hash = hash(&pixels);
        let id = format!(
            "native:{}:{}:{}:{gpu_completion_hash}",
            source.stamp.world, source.stamp.snapshot_id, self.target.resize_sequence
        );
        let retained_texture =
            store::retain(&mut self.textures, self.target.viewport, &id, &pixels);
        let mut provenance = provenance();
        provenance["gpu_submission"] = serde_json::json!(submission);
        provenance["adapter"] =
            serde_json::json!(format!("{:?}", self.context.adapter().get_info()));
        Ok(ViewReceipt {
            stamp: source.stamp.clone(),
            managed: source.managed.metadata(),
            raw_snapshot: source.raw().to_vec(),
            resolved_view,
            target: self.target.clone(),
            captured_format: FORMAT,
            texture_revision: source.stamp.content_revision,
            gpu_completion_hash,
            provenance,
            pixels,
            retained_texture,
        })
    }

    pub fn capture_interleaved(
        &mut self,
        fixture: &mut Fixture,
        session: &Session,
        source: &Extraction,
    ) -> Result<(), String> {
        source.validate(fixture, session, true)?;
        let view = self.prepare(source, session.object)?;
        let (pixels, submission) = self.render(&view, || fixture.advance(session));
        assert!(!pixels.is_empty());
        let admission = source.validate(fixture, session, true);
        let evidence = serde_json::json!({"source_stamp":source.stamp,
            "gpu_submission":submission,"completed_rgba_sha256":hash(&pixels),
            "completed_bytes":pixels.len(),"current_admission":admission,
            "native_after":fixture.request(session.world, &serde_json::json!({"op":"session.status"})),
            "ordering":"submit frame-1 source; actual managed frame-2 advance; completed Readback; native current validation",
            "scope":"rejected candidate; no accepted fresh-source GPU receipt"});
        let directory = std::path::PathBuf::from(std::env::var_os("SNAPSHOT_ARTIFACTS").unwrap());
        std::fs::write(
            directory.join("interleaved-rejected.json"),
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        admission.map(|_| ())
    }

    fn prepare(&mut self, source: &Extraction, object: u64) -> Result<SceneView, String> {
        let position = canonical_position(source.position(object)?);
        self.scene
            .set_dynamic_quads(
                self.context.device(),
                self.context.queue(),
                &self.pipelines,
                &[DynamicQuad {
                    position,
                    tint: [1.0, 0.0, 0.0, 1.0],
                    rect: [-0.25, -0.25, 0.25, 0.25],
                    ..DynamicQuad::default()
                }],
            )
            .map_err(|error| error.to_string())?;
        let resolved_view = self.target.resolve();
        let view = SceneView::from_resolved(resolved_view);
        self.scene
            .set_frame(self.context.queue(), FrameUniform::unlit(&view));
        Ok(view)
    }

    fn render(&mut self, view: &SceneView, after_submit: impl FnOnce()) -> (Vec<u8>, String) {
        let context = self.context;
        let size = self.target.viewport;
        self.chain.resize(context.device(), size).unwrap();
        let depth = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
        let output = post::color_texture(
            context.device(),
            "managed snapshot caller output",
            size,
            FORMAT,
        );
        let mut encoder = context
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        draw(
            &mut encoder,
            self.chain.targets().scene(),
            depth.depth_view(),
            &self.scene,
            &self.pipelines,
        );
        self.chain.encode(
            &mut encoder,
            &output.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        let mut readback = post::Readback::new(
            context.device(),
            post::FrameConfig::new(size, FORMAT, 1),
            1_048_576,
        )
        .unwrap();
        readback.encode(&mut encoder, &output).unwrap();
        let submission = context.queue().submit([encoder.finish()]);
        let submission_identity = format!("{submission:?}");
        after_submit();
        (
            readback
                .read_encoded(context.device(), Some(submission))
                .unwrap(),
            submission_identity,
        )
    }
}

fn draw(
    encoder: &mut wgpu::CommandEncoder,
    color: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    scene: &SceneGpu,
    pipelines: &ScenePipelines,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("managed snapshot canonical scene"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: color,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        ..Default::default()
    });
    scene.draw(&mut pass, pipelines);
}
