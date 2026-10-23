//! Stock composition uses the same feature and attachment boundary as external draws.
use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, BorrowedFeature, CopyOutput,
    FeatureDeclaration, FrameConfig, FrameError, FrameTargets, PassDeclaration, PassSlot,
    PostChain, PostError, PreparedFeature, Resources, StageFactory, StageSet, ToneMapOutput,
};

/// Retained executor for encoder-only composition. Submission belongs to the host.
/// Select a distinct admitted slot before preparing each overlapping frame, then submit
/// successful encoders in preparation order before reusing their slots after completion.
/// On error, no uploads or draws are appended and resident state is unchanged.
/// A successful encoder must not be abandoned: its uploads have been committed.
pub struct FrameRenderer {
    chain: PostChain<FrameOutput>,
    pub(crate) water: Option<crate::water::Water>,
    water_state: std::rc::Rc<std::cell::RefCell<crate::water::WaterState>>,
    pub(crate) snapshots: Option<[crate::water::snapshots::Snapshots; 3]>,
    snapshot_ready: [crate::water::planning::SnapshotRequest; 3],
    frame_slot: usize,
    elapsed_seconds: f32,
    pub(crate) ssao: Option<crate::ssao::Ssao>,
}
impl std::fmt::Debug for FrameRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameRenderer").finish_non_exhaustive()
    }
}
impl FrameRenderer {
    /// Select the host's admitted frame slot before preparing any features.
    pub fn select_frame_slot(&mut self, slot: usize) {
        self.chain.select_frame_slot(slot);
        self.frame_slot = slot;
        self.snapshot_ready[slot] = crate::water::planning::SnapshotRequest::default();
    }
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
            ssao: None,
            water: None,
            water_state: std::rc::Rc::default(),
            snapshots: None,
            snapshot_ready: Default::default(),
            frame_slot: 0,
            elapsed_seconds: 0.0,
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
            if let Some(settings) = effects.ssao() {
                crate::ssao::validate_settings(settings)?;
            }
            crate::frame::validate_stock_effects(effects)
                .map_err(|_| PostError::UnsupportedProfile("scene effects"))?;
        }
        let config = FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, samples);
        config.validate_adapter(context.adapter())?;
        let exposure = effects.map_or(1.0, |effects| {
            f64::from(effects.grading().exposure_micro) / 1_000_000.0
        });
        let factory = ToneMapOutput::new(output_format, exposure, tone_map)?;
        let mut renderer = Self {
            water: None,
            water_state: std::rc::Rc::default(),
            snapshots: None,
            snapshot_ready: Default::default(),
            frame_slot: 0,
            elapsed_seconds: 0.0,
            ssao: effects
                .and_then(|effects| effects.ssao())
                .map(|settings| crate::ssao::Ssao::new(context.device(), size, *settings))
                .transpose()?,
            chain: PostChain::new(
                context.device(),
                config,
                Resources::default(),
                FrameOutput::Hdr(factory),
            )?,
        };
        if let Some(settings) = effects.and_then(|effects| effects.water()) {
            renderer.install_water(context.device(), *settings)?;
        }
        Ok(renderer)
    }

    pub(crate) fn install_water(
        &mut self,
        device: &wgpu::Device,
        settings: worldrunner_render_plan::SceneWaterPlane,
    ) -> Result<(), FrameError> {
        let water = crate::water::Water::new(device, self.targets().config(), settings)?;
        self.snapshot_ready = Default::default();
        self.snapshots = Some(water.snapshots.clone());
        self.chain.register(
            device,
            Box::new(crate::water::WaterRegistration {
                state: self.water_state.clone(),
                samples: self.targets().config().samples,
            }),
        )?;
        self.water = Some(water);
        Ok(())
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

    pub(crate) fn validate_ssao(
        &self,
        pipelines: &crate::renderer::ScenePipelines,
        requested: Option<&worldrunner_render_plan::SceneSsao>,
    ) -> Result<(), FrameError> {
        if pipelines.ssao != self.ssao.is_some() {
            return Err(FrameError::UnsupportedEffect(
                "SSAO pipeline/profile mismatch",
            ));
        }
        if self.ssao.as_ref().map(|ssao| &ssao.settings) != requested {
            return Err(FrameError::SsaoSettings(
                "SSAO resident/profile settings mismatch",
            ));
        }
        Ok(())
    }

    /// Supplies animation time for the next prepared frame, in seconds.
    pub fn set_frame_time(&mut self, seconds: f32) -> Result<(), FrameError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(FrameError::WaterSettings(
                "frame time must be finite and nonnegative",
            ));
        }
        self.elapsed_seconds = seconds;
        Ok(())
    }
    pub(crate) fn validate_water(
        &self,
        requested: Option<&worldrunner_render_plan::SceneWaterPlane>,
    ) -> Result<(), FrameError> {
        if self.water.as_ref().map(|water| &water.settings) != requested {
            return Err(FrameError::WaterSettings(
                "resident/profile settings mismatch; water requires HDR",
            ));
        }
        Ok(())
    }
    /// Opaque-only read inputs for the currently selected admitted frame slot.
    /// Returns both views only when both were produced by the last successful encode
    /// in this slot. Selecting a slot or resizing clears readiness. Views cannot be
    /// used as writable attachments by later features.
    pub fn opaque_snapshot(&self) -> Option<(wgpu::TextureView, wgpu::TextureView)> {
        self.snapshots
            .as_ref()
            .filter(|_| {
                self.snapshot_ready[self.frame_slot].color
                    && self.snapshot_ready[self.frame_slot].depth
            })
            .map(|snapshots| {
                let snapshot = &snapshots[self.frame_slot];
                (
                    snapshot
                        .color
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                    snapshot
                        .depth
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                )
            })
    }
    /// Positive view-space metres after water. SSAO and opaque snapshots remain
    /// pre-water; later depth consumers declare `ResourceRef::new("post-water depth")`.
    pub fn post_water_depth(&self) -> Option<wgpu::TextureView> {
        self.snapshots
            .as_ref()
            .filter(|_| self.snapshot_ready[self.frame_slot].final_depth)
            .map(|snapshots| {
                snapshots[self.frame_slot]
                    .final_depth
                    .create_view(&wgpu::TextureViewDescriptor::default())
            })
    }
    pub fn targets(&self) -> &worldrunner_render_post::FrameTargets {
        self.chain.targets()
    }
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        size: [u32; 2],
    ) -> Result<bool, worldrunner_render_post::PostError> {
        FrameConfig::new(
            size,
            self.targets().config().format,
            self.targets().config().samples,
        )
        .validate(&device.limits())?;
        let replacement = self
            .ssao
            .as_ref()
            .filter(|ssao| ssao.size != size)
            .map(|ssao| crate::ssao::Ssao::new(device, size, ssao.settings))
            .transpose()?;
        let water = self
            .water
            .as_ref()
            .filter(|water| water.snapshots[0].size != size)
            .map(|water| {
                crate::water::Water::new(
                    device,
                    FrameConfig {
                        size,
                        ..self.targets().config()
                    },
                    water.settings,
                )
            })
            .transpose()?;
        let changed = self.chain.resize(device, size)?;
        if let Some(water) = water {
            self.snapshots = Some(water.snapshots.clone());
            self.water = Some(water);
        } else if changed && self.snapshots.is_some() {
            self.snapshots = Some(std::array::from_fn(|_| {
                crate::water::snapshots::Snapshots::new(device, self.targets().config())
            }));
        }
        if changed {
            self.snapshot_ready = Default::default();
        }
        if let Some(replacement) = replacement {
            self.ssao = Some(replacement);
        }
        Ok(changed)
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
    fn set_water_input(
        &mut self,
        device: &wgpu::Device,
        frame: crate::frame::FrameUniform,
        shadow: Option<&wgpu::BindGroup>,
    ) {
        self.snapshot_ready[self.frame_slot] = crate::water::planning::SnapshotRequest::default();
        self.water_state.borrow_mut().feature = self.water.as_ref().map(|water| {
            water.feature(
                device,
                frame,
                (self.elapsed_seconds, self.frame_slot),
                shadow,
            )
        });
    }
    /// Camera-aware composition for stock and external features, including
    /// declaration-requested snapshots. The optional binding supplies shared shadows.
    pub fn encode_frame(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        plan: (&[PassDeclaration], &[AttachmentView]),
        features: &mut [BorrowedFeature<'_>],
        input: (crate::frame::FrameUniform, Option<&wgpu::BindGroup>),
    ) -> Result<(), FrameError> {
        let (frame, shadow) = input;
        let (passes, views) = plan;
        let mut passes = passes.to_vec();
        let mut views = views.to_vec();
        self.set_water_input(device, frame, shadow);
        let mut combined: Vec<_> = features
            .iter_mut()
            .map(|feature| BorrowedFeature {
                declaration: feature.declaration.clone(),
                prepared: &mut *feature.prepared,
            })
            .collect();
        let mut screen = self
            .ssao
            .as_ref()
            .map(|ssao| crate::ssao::passes::ScreenFeature { ssao, frame });
        if let Some(screen) = &mut screen {
            let declaration = screen.schedule(&mut passes, &mut views)?;
            combined.push(BorrowedFeature {
                declaration,
                prepared: screen,
            });
        }
        let retained = self.chain.plan_features(&mut passes, &mut views)?;
        let reads: Vec<_> = combined
            .iter()
            .map(|feature| &feature.declaration)
            .chain(retained.iter())
            .flat_map(|feature| &feature.passes)
            .flat_map(|pass| &pass.reads)
            .copied()
            .collect();
        let config = self.targets().config();
        let (request, mut snapshot_feature) = crate::water::planning::snapshot_feature(
            device,
            &mut self.snapshots,
            (config, frame, self.frame_slot),
            (&mut passes, &mut views),
            &reads,
            self.water.is_some(),
        )?;
        if let Some((declaration, feature)) = &mut snapshot_feature {
            combined.push(BorrowedFeature {
                declaration: declaration.clone(),
                prepared: feature,
            });
        }
        passes.sort_by_key(|pass| pass.slot);
        self.chain.encode_geometry(
            device,
            encoder,
            &worldrunner_render_post::FramePlan {
                passes: &passes,
                views: &views,
                requested: &[],
            },
            &mut combined,
        )?;
        self.snapshot_ready[self.frame_slot] = request;
        Ok(())
    }
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        passes: &[PassDeclaration],
        views: &[AttachmentView],
        features: &mut [BorrowedFeature<'_>],
    ) -> Result<(), FrameError> {
        self.snapshot_ready[self.frame_slot] = crate::water::planning::SnapshotRequest::default();
        if self.ssao.is_some() || self.water.is_some() {
            return Err(FrameError::UnsupportedEffect(
                "SSAO requires stock geometry composition",
            ));
        }
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
    pub screen: Option<crate::ssao::Geometry>,
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
    fn draw(
        &self,
        context: &worldrunner_render_post::DrawContext,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        if let Some(screen) = &self.screen {
            if context.slot == PassSlot::DepthNormalPrepass {
                self.gpu
                    .draw_prepass(pass, &screen.pipelines, &screen.group);
                return;
            }
            pass.set_bind_group(3, &screen.ambient, &[]);
        }
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
    renderer.validate_ssao(pipelines, gpu.ssao.as_ref())?;
    renderer.validate_water(gpu.water.as_ref())?;
    let frame = renderer.prepare_uniform(frame)?;
    let (color, depth) = attachments;
    let color_id = attachment_resource("scene color", color);
    let depth_id = attachment_resource("scene depth", depth);
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(attachment(color_id, color, false));
    opaque.depth = Some(attachment(depth_id, depth, true));
    let mut declaration = SceneFeature::declaration(pipelines);
    if renderer.ssao.is_some() {
        crate::ssao::passes::geometry_declaration(&mut declaration);
    }
    let mut feature = SceneFeature {
        screen: renderer.ssao.as_ref().map(|ssao| ssao.geometry.clone()),
        gpu,
        pipelines,
        frame,
    };
    renderer.encode_frame(
        device,
        encoder,
        (
            &[opaque],
            &[
                color.with_resource(color_id)?,
                depth.with_resource(depth_id)?,
            ],
        ),
        &mut [BorrowedFeature {
            declaration,
            prepared: &mut feature,
        }],
        (frame, None),
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
