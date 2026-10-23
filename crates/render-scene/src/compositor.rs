//! Stock composition uses the same feature and attachment boundary as external draws.
use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, BorrowedFeature, CopyOutput,
    FeatureDeclaration, FrameConfig, FrameError, FrameTargets, PassDeclaration, PassSlot,
    PostChain, PostError, PreparedFeature, Resources, StageFactory, StageSet, ToneMapOutput,
};

/// Retained executor for encoder-only composition. Submission belongs to the host.
/// Submit a successful encoder before preparing another frame on shared resources.
/// On error, no uploads or draws are appended and resident state is unchanged.
/// A successful encoder must not be abandoned: its uploads have been committed.
pub struct FrameRenderer {
    chain: PostChain<FrameOutput>,
}
impl std::fmt::Debug for FrameRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameRenderer").finish_non_exhaustive()
    }
}
impl FrameRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        Self::with_output(
            device,
            FrameConfig::new([1, 1], crate::offscreen::OffscreenTarget::COLOR_FORMAT, 1),
            crate::offscreen::OffscreenTarget::COLOR_FORMAT,
        )
        .expect("stock frame contract")
    }
    pub fn with_output(
        device: &wgpu::Device,
        config: FrameConfig,
        output_format: wgpu::TextureFormat,
    ) -> Result<Self, worldrunner_render_post::PostError> {
        Ok(Self {
            chain: PostChain::new(
                device,
                config,
                Resources::default(),
                FrameOutput::Copy(CopyOutput::new(device, output_format)),
            )?,
        })
    }
    /// HDR exposure is taken from the scene contract, never applied by geometry.
    pub fn hdr(
        context: &crate::device::GpuContext,
        size: [u32; 2],
        output_format: wgpu::TextureFormat,
        samples: u32,
        tone_map: worldrunner_render_post::ToneMap,
        effects: Option<&worldrunner_render_plan::SceneEffects>,
    ) -> Result<Self, PostError> {
        if !matches!(samples, 1 | 4) {
            return Err(PostError::SampleCount(samples));
        }
        if let Some(effects) = effects {
            crate::frame::validate_stock_effects(effects)
                .map_err(|_| PostError::UnsupportedProfile("scene effects"))?;
        }
        let config = FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, samples);
        config.validate_adapter(context.adapter())?;
        let exposure = effects.map_or(1.0, |effects| {
            f64::from(effects.grading().exposure_micro) / 1_000_000.0
        });
        let factory = ToneMapOutput::new(output_format, exposure, tone_map)?;
        Ok(Self {
            chain: PostChain::new(
                context.device(),
                config,
                Resources::default(),
                FrameOutput::Hdr(factory),
            )?,
        })
    }

    /// A fixed-exposure profile is immutable. Rebuild it when scene exposure changes.
    /// Geometry stays unexposed only when this executor owns the post multiplier.
    pub fn prepare_uniform(
        &self,
        mut frame: crate::frame::FrameUniform,
    ) -> Result<crate::frame::FrameUniform, FrameError> {
        if let FrameOutput::Hdr(factory) = self.chain.factory() {
            if !factory.matches_exposure(frame.exposure) {
                return Err(FrameError::FixedExposureMismatch);
            }
            frame.exposure = 1.0;
        }
        Ok(frame)
    }

    pub fn targets(&self) -> &worldrunner_render_post::FrameTargets {
        self.chain.targets()
    }
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        size: [u32; 2],
    ) -> Result<bool, worldrunner_render_post::PostError> {
        self.chain.resize(device, size)
    }
    pub fn encode_output(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        self.chain.targets().encode_resolve(encoder);
        self.chain.encode(encoder, output);
    }
    pub fn register(
        &mut self,
        device: &wgpu::Device,
        feature: Box<dyn worldrunner_render_post::FrameFeature>,
    ) -> Result<worldrunner_render_post::FeatureId, FrameError> {
        self.chain.register(device, feature)
    }
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        passes: &[PassDeclaration],
        views: &[AttachmentView],
        features: &mut [BorrowedFeature<'_>],
    ) -> Result<(), FrameError> {
        self.chain.encode_geometry(
            device,
            encoder,
            &worldrunner_render_post::FramePlan {
                passes,
                views,
                requested: &[],
            },
            features,
        )
    }
}

pub(crate) fn attachment_resource(
    name: &'static str,
    view: &AttachmentView,
) -> worldrunner_render_post::ResourceRef {
    worldrunner_render_post::ResourceRef {
        name,
        version: 0,
        ..view.resource()
    }
}

pub(crate) fn attachment(
    resource: worldrunner_render_post::ResourceRef,
    view: &AttachmentView,
    depth: bool,
) -> AttachmentDeclaration {
    let texture = view.texture();
    AttachmentDeclaration {
        resource,
        format: view.format(),
        size: view.size(),
        samples: texture.sample_count(),
        load: if depth {
            AttachmentLoad::ClearDepth(1.0)
        } else {
            AttachmentLoad::ClearColor(wgpu::Color::BLACK)
        },
        store: wgpu::StoreOp::Store,
    }
}

pub(crate) struct SceneFeature<'a> {
    pub gpu: &'a mut crate::draw::SceneGpu,
    pub pipelines: &'a crate::renderer::ScenePipelines,
    pub frame: crate::frame::FrameUniform,
}
impl SceneFeature<'_> {
    pub fn declaration(pipelines: &crate::renderer::ScenePipelines) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::Opaque);
        declaration.passes[0].color_format = Some(pipelines.color_format);
        declaration.passes[0].samples = pipelines.samples;
        declaration.passes[0].depth_format = Some(wgpu::TextureFormat::Depth32Float);
        declaration
    }
}
impl PreparedFeature for SceneFeature<'_> {
    fn prepare(
        &mut self,
        _context: &mut worldrunner_render_post::PrepareContext<'_>,
    ) -> Result<(), FrameError> {
        Ok(())
    }
    fn commit(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        self.gpu.prepare_instances(context);
        self.gpu.commit_frame(context, self.frame);
    }
    fn draw(&self, _: &worldrunner_render_post::DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        self.gpu.draw(pass, self.pipelines);
    }
}

/// Encodes a resident unshadowed scene through the feature boundary.
/// Checked attachments retain their selected mip, array layer and view format.
/// The submission and abandonment rules of [`FrameRenderer`] apply.
pub fn encode_scene(
    renderer: &mut FrameRenderer,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    scene: (&mut crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
    attachments: (&AttachmentView, &AttachmentView),
    frame: crate::frame::FrameUniform,
) -> Result<(), FrameError> {
    let (gpu, pipelines) = scene;
    let frame = renderer.prepare_uniform(frame)?;
    let (color, depth) = attachments;
    let color_id = attachment_resource("scene color", color);
    let depth_id = attachment_resource("scene depth", depth);
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(attachment(color_id, color, false));
    opaque.depth = Some(attachment(depth_id, depth, true));
    let mut feature = SceneFeature {
        gpu,
        pipelines,
        frame,
    };
    renderer.encode(
        device,
        encoder,
        &[opaque],
        &[
            color.with_resource(color_id)?,
            depth.with_resource(depth_id)?,
        ],
        &mut [BorrowedFeature {
            declaration: SceneFeature::declaration(pipelines),
            prepared: &mut feature,
        }],
    )
}

enum FrameOutput {
    Copy(CopyOutput),
    Hdr(ToneMapOutput),
}
impl StageFactory for FrameOutput {
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        persistent: &Resources,
    ) -> Result<StageSet, PostError> {
        match self {
            Self::Copy(factory) => factory.build(device, targets, persistent),
            Self::Hdr(factory) => factory.build(device, targets, persistent),
        }
    }
}

/// Opt-in attachment and output policy. Scene anti-aliasing effects remain independent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameProfile {
    #[default]
    Legacy,
    Hdr {
        samples: u32,
        tone_map: worldrunner_render_post::ToneMap,
    },
}
impl FrameProfile {
    pub const fn target(self) -> (wgpu::TextureFormat, u32) {
        match self {
            Self::Legacy => (wgpu::TextureFormat::Rgba8UnormSrgb, 1),
            Self::Hdr { samples, .. } => (wgpu::TextureFormat::Rgba16Float, samples),
        }
    }
}
