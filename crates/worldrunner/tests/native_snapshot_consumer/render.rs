#[path = "../support/resident.rs"]
mod resident;
use super::{
    capture::Capture,
    extraction::{Extraction, hash},
    native::{Fixture, Session},
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
        let scene = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
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
    ) -> Result<Capture, String> {
        source.validate(fixture, session, true)?;
        let position = canonical_position(source.position(object)?);
        self.scene
            .set_dynamic_quads(
                self.context.device(),
                self.context.submission(),
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
        let view = SceneView::from_resolved(resolved_view, [0, -10_000, 0]);
        self.scene
            .set_frame(self.context.queue(), FrameUniform::unlit(&view));
        let pixels = self.render(&view);
        source.validate(fixture, session, true)?;
        let gpu_completion_hash = hash(&pixels);
        let id = format!(
            "native:{}:{}:{}:{gpu_completion_hash}",
            source.stamp.world, source.stamp.snapshot_id, self.target.resize_sequence
        );
        let retained_texture =
            store::retain(&mut self.textures, self.target.viewport, &id, &pixels);
        Ok(Capture {
            stamp: source.stamp.clone(),
            captured_snapshot: source.captured_value()["snapshot"].clone(),
            resolved_view,
            target: self.target.clone(),
            captured_format: FORMAT,
            texture_revision: source.stamp.content_revision,
            gpu_completion_hash,
            pixels,
            retained_texture,
        })
    }

    fn render(&mut self, view: &SceneView) -> Vec<u8> {
        let context = self.context;
        let size = self.target.viewport;
        self.chain.resize(context.device(), size).unwrap();
        let depth = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
        let output = post::color_texture(
            context.device(),
            "native snapshot caller output",
            size,
            FORMAT,
        );
        let mut readback = post::Readback::new(
            context.device(),
            post::FrameConfig::new(size, FORMAT, 1),
            1_048_576,
        )
        .unwrap();
        let prepared = context
            .submission()
            .prepare(context.device(), |encoder, _| {
                resident::encode(
                    context.device(),
                    encoder,
                    (&self.scene, &self.pipelines),
                    (self.chain.targets().scene(), depth.depth_view()),
                )
                .unwrap();
                self.chain.encode(
                    encoder,
                    &output.create_view(&wgpu::TextureViewDescriptor::default()),
                );
                readback.encode(encoder, &output)
            })
            .unwrap();
        let submission = prepared.submit().unwrap();
        readback
            .read_encoded(context.device(), Some(submission))
            .unwrap()
    }
}
