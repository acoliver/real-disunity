//! Stock composition uses the same feature and attachment boundary as external draws.
mod adapters;
mod capture;
pub use capture::FeatureTarget;
mod clock;
mod configuration;
mod depth_prefix;
mod environment;
mod output;
pub(crate) mod publication;
mod validation;
pub use adapters::encode_resident;
pub use configuration::{ConfiguredFrameRenderer, FrameEncoding, RendererConfiguration};
use output::FrameOutput;
pub use output::FrameProfile;

use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, BorrowedFeature, CopyOutput,
    FeatureDeclaration, FrameConfig, FrameError, PassDeclaration, PostChain, PostError, Resources,
    ToneMapOutput,
};

/// Retained executor for transaction-bound composition. Submission belongs to the host.
/// Select an admitted slot before preparation and reuse it only after completion.
/// Encoding requires the authority-owned encoder. Cancellation discards publication.
pub struct FrameRenderer {
    chain: PostChain<FrameOutput>,
    environment_conventions: [Option<worldrunner_render_plan::SceneSpecularConvention>; 2],
    pub(crate) water: Option<crate::water::Water>,
    water_state: std::rc::Rc<std::cell::RefCell<crate::water::WaterState>>,
    pub(crate) snapshots: Option<[crate::water::snapshots::Snapshots; 3]>,
    snapshot_ready: std::rc::Rc<std::cell::RefCell<[crate::water::planning::SnapshotRequest; 3]>>,
    frame_slot: usize,
    clear_color: wgpu::Color,
    post_requested: worldrunner_render_post::HdrPostSettings,
    elapsed_seconds: f32,
    pub(crate) selected_time: Option<std::time::Duration>,
    animation_time: Option<u32>,
    frame_delta: f32,
    post_time: std::rc::Rc<std::cell::Cell<f32>>,
    exposure_reset: std::rc::Rc<std::cell::Cell<bool>>,
    pub(crate) ssao: Option<crate::scene_metadata::SceneMetadata>,
    pub(crate) ssr: Option<crate::ssr::Ssr>,
    ssao_requested: Option<worldrunner_render_plan::SceneSsao>,
    ssr_requested: Option<worldrunner_render_plan::SceneSsr>,
}
impl std::fmt::Debug for FrameRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameRenderer").finish_non_exhaustive()
    }
}
impl FrameRenderer {
    pub fn set_clear_color(&mut self, color: wgpu::Color) -> Result<(), PostError> {
        validate_clear(color)?;
        self.clear_color = color;
        Ok(())
    }
    pub(crate) fn color_attachment(
        &self,
        resource: worldrunner_render_post::ResourceRef,
        view: &AttachmentView,
    ) -> AttachmentDeclaration {
        let mut declaration = attachment(resource, view, false);
        declaration.load = AttachmentLoad::ClearColor(self.clear_color);
        declaration
    }
    /// Select the host's admitted frame slot before preparing any features.
    pub fn select_frame_slot(&mut self, slot: usize) {
        self.chain.select_frame_slot(slot);
        self.frame_slot = slot;
        self.snapshot_ready.borrow_mut()[slot] = crate::water::planning::SnapshotRequest::default();
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
            environment_conventions: [None; 2],
            ssao: None,
            ssr: None,
            ssao_requested: None,
            ssr_requested: None,
            water: None,
            water_state: std::rc::Rc::default(),
            snapshots: None,
            snapshot_ready: std::rc::Rc::default(),
            frame_slot: 0,
            clear_color: wgpu::Color::BLACK,
            post_requested: worldrunner_render_post::HdrPostSettings::default(),
            elapsed_seconds: 0.0,
            selected_time: None,
            animation_time: None,
            frame_delta: 0.0,
            post_time: std::rc::Rc::new(std::cell::Cell::new(0.0)),
            exposure_reset: std::rc::Rc::new(std::cell::Cell::new(false)),
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
        validate_hdr_effects(effects)?;
        let config = FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, samples);
        config.validate_adapter(context.adapter())?;
        validate_reflection_extent(
            context.device(),
            config,
            effects.and_then(|effects| effects.ssr()),
        )?;
        let exposure = effects.map_or(1.0, |effects| {
            f64::from(effects.grading().exposure_micro) / 1_000_000.0
        });
        let post = effects
            .map(crate::post::settings)
            .transpose()?
            .unwrap_or_default();
        post.validate_frame(config)?;
        let factory = ToneMapOutput::new(output_format, exposure, tone_map)?
            .with_effects(context.device(), post)?;
        let ssao_requested = effects.and_then(|effects| effects.ssao()).copied();
        let ssr_requested = effects.and_then(|effects| effects.ssr()).copied();
        let mut renderer = Self {
            environment_conventions: [None; 2],
            ssao_requested,
            ssr_requested,
            ssr: None,
            water: None,
            water_state: std::rc::Rc::default(),
            snapshots: None,
            snapshot_ready: std::rc::Rc::default(),
            frame_slot: 0,
            clear_color: wgpu::Color::BLACK,
            post_requested: post,
            elapsed_seconds: 0.0,
            selected_time: None,
            animation_time: None,
            frame_delta: 0.0,
            post_time: std::rc::Rc::new(std::cell::Cell::new(0.0)),
            exposure_reset: std::rc::Rc::new(std::cell::Cell::new(false)),
            ssao: effects
                .and_then(|effects| effects.ssao())
                .map(|settings| {
                    crate::scene_metadata::SceneMetadata::new(context.device(), size, *settings)
                })
                .transpose()?,
            chain: PostChain::new(
                context.device(),
                config,
                Resources::default(),
                FrameOutput::Hdr(factory),
            )?,
        };
        renderer.install_reflections(context.device(), size)?;
        if let Some(effects) = effects {
            renderer.install_water_bodies(context.device(), effects.water_bodies().to_vec())?;
        }
        Ok(renderer)
    }

    fn install_reflections(
        &mut self,
        device: &wgpu::Device,
        size: [u32; 2],
    ) -> Result<(), FrameError> {
        if let Some(settings) = self.ssr_requested.filter(|settings| settings.enabled) {
            crate::ssr::validate_extent(self.targets().config(), &settings, &device.limits())?;
            if self.ssao.is_none() {
                self.ssao = Some(crate::scene_metadata::SceneMetadata::new(
                    device,
                    size,
                    crate::ssr::neutral_ao(),
                )?);
            }
            let config = self.targets().config();
            let screen = self.ssao.as_mut().expect("reflection prepass");
            self.ssr = Some(crate::ssr::Ssr::new(device, config, settings, screen)?);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn install_water(
        &mut self,
        device: &wgpu::Device,
        settings: worldrunner_render_plan::SceneWaterBody,
    ) -> Result<(), FrameError> {
        self.install_water_bodies(device, vec![settings])
    }

    /// Installs independent water surfaces in the existing transparent pass.
    /// An empty set leaves the feature disabled. Replacing a resident set requires a rebuild.
    pub fn install_water_bodies(
        &mut self,
        device: &wgpu::Device,
        settings: Vec<worldrunner_render_plan::SceneWaterBody>,
    ) -> Result<(), FrameError> {
        if self.water.is_some() {
            return Err(FrameError::WaterSettings("water is already installed"));
        }
        if settings.is_empty() {
            return Ok(());
        }
        let water = crate::water::Water::new(device, self.targets().config(), settings)?;
        *self.snapshot_ready.borrow_mut() = Default::default();
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

    pub(crate) fn bind_water_inputs(&mut self, inputs: &[[Option<wgpu::TextureView>; 2]]) {
        if let Some(water) = &mut self.water {
            water.surface_inputs = inputs.to_vec();
        }
    }
    /// Opaque-only read inputs for the currently selected admitted frame slot.
    /// Returns both views only when both were produced by the last successful encode
    /// in this slot. Selecting a slot or resizing clears readiness. Views cannot be
    /// used as writable attachments by later features.
    pub fn opaque_snapshot(&self) -> Option<(wgpu::TextureView, wgpu::TextureView)> {
        self.snapshots
            .as_ref()
            .filter(|_| {
                self.snapshot_ready.borrow()[self.frame_slot].color
                    && self.snapshot_ready.borrow()[self.frame_slot].depth
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
            .filter(|_| self.snapshot_ready.borrow()[self.frame_slot].final_depth)
            .map(|snapshots| {
                snapshots[self.frame_slot]
                    .final_depth
                    .create_view(&wgpu::TextureViewDescriptor::default())
            })
    }
    /// Submitted per-sample water receivers: view normal and water flag (alpha),
    /// dielectric F0 and environment roughness, and replaceable environment specular.
    /// Sample-frequency normal/coverage, material (pixel offsets RG, scalar F0 B,
    /// environment-coordinate roughness A), and specular (camera transmittance A).
    pub fn water_receivers(
        &self,
    ) -> Option<(wgpu::TextureView, wgpu::TextureView, wgpu::TextureView)> {
        self.water
            .as_ref()
            .filter(|_| self.snapshot_ready.borrow()[self.frame_slot].final_depth)
            .map(|water| {
                let [normal, material, specular] = &water.receivers[self.frame_slot];
                (
                    normal.create_view(&wgpu::TextureViewDescriptor::default()),
                    material.create_view(&wgpu::TextureViewDescriptor::default()),
                    specular.create_view(&wgpu::TextureViewDescriptor::default()),
                )
            })
    }
    /// Existing pixel-centre linear metre depth and normal coverage for shadow
    /// statistics. Callers rebind after resize; no additional prepass is allocated.
    pub fn depth_statistics_source(
        &self,
    ) -> Result<Option<(AttachmentView, AttachmentView)>, FrameError> {
        use crate::frame::metadata::FrameProduct;
        self.ssao
            .as_ref()
            .map(|source| {
                let levels = source.passes.depth_levels();
                Ok((
                    FrameProduct::linear_depth(0).attachment(levels[0])?,
                    FrameProduct::normals().attachment(&source.normals)?,
                ))
            })
            .transpose()
    }

    pub fn targets(&self) -> &worldrunner_render_post::FrameTargets {
        self.chain.targets()
    }
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        size: [u32; 2],
    ) -> Result<bool, worldrunner_render_post::PostError> {
        let config = FrameConfig {
            size,
            ..self.targets().config()
        };
        validate_reflection_extent(device, config, self.ssr.as_ref().map(|ssr| &ssr.settings))?;
        config.validate(&device.limits())?;
        let mut replacement = self
            .ssao
            .as_ref()
            .filter(|ssao| ssao.size != size)
            .map(|ssao| crate::scene_metadata::SceneMetadata::new(device, size, ssao.settings))
            .transpose()?;
        let water = self
            .water
            .as_ref()
            .filter(|water| water.snapshots[0].size != size)
            .map(|water| {
                water.resized(
                    device,
                    worldrunner_render_post::FrameConfig {
                        size,
                        ..self.targets().config()
                    },
                )
            })
            .transpose()?;
        let reflection =
            if let (Some(screen), Some(ssr)) = (replacement.as_mut(), self.ssr.as_ref()) {
                Some(ssr.resized(
                    device,
                    FrameConfig {
                        size,
                        ..self.targets().config()
                    },
                    screen,
                )?)
            } else {
                None
            };
        let changed = self.chain.resize(device, size)?;
        if let Some(reflection) = reflection {
            self.ssr = Some(reflection);
        }
        if let Some(water) = water {
            self.snapshots = Some(water.snapshots.clone());
            self.water = Some(water);
        } else if changed && self.snapshots.is_some() {
            self.snapshots = Some(std::array::from_fn(|_| {
                crate::water::snapshots::Snapshots::new(device, self.targets().config())
            }));
        }
        if changed {
            *self.snapshot_ready.borrow_mut() = Default::default();
        }
        if let Some(replacement) = replacement {
            self.ssao = Some(replacement);
        }
        Ok(changed)
    }
    pub fn encode_output(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
    ) -> Result<(), PostError> {
        worldrunner_render_gpu::submission::Publications::for_encoder(encoder)
            .map_err(FrameError::from)?;
        if let FrameOutput::Hdr(factory) = self.chain.factory() {
            factory.prepare_effects(
                encoder,
                self.frame_slot,
                self.frame_delta,
                self.exposure_reset.get(),
            )?;
            let reset = self.exposure_reset.clone();
            publication::encode(encoder, &reset.clone(), move || {
                reset.set(false);
            })
            .map_err(FrameError::from)?;
        }
        self.chain.encode_resolve(encoder);
        self.chain.encode(encoder, output);
        self.chain
            .finish_timing(encoder)
            .map_err(FrameError::from)?;
        let time = self.post_time.clone();
        let seconds = self.elapsed_seconds;
        publication::encode(encoder, &time.clone(), move || {
            time.set(seconds);
        })
        .map_err(FrameError::from)?;
        Ok(())
    }
    /// Timing is explicitly begun before geometry and resolved after output.
    pub fn configure_timing(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mode: worldrunner_render_post::TimingMode,
    ) -> Result<worldrunner_render_post::TimingSupport, worldrunner_render_post::TimingError> {
        self.chain.configure_timing(device, queue, mode)
    }
    pub fn begin_timing(
        &self,
        frame_id: u64,
    ) -> Result<(), worldrunner_render_gpu::submission::TransactionError> {
        self.chain.begin_timing(frame_id)
    }
    pub fn take_timings(
        &self,
    ) -> Result<Vec<worldrunner_render_post::FrameTiming>, worldrunner_render_post::TimingError>
    {
        self.chain.take_timings()
    }
    pub fn unavailable_timing_passes(&self) -> u64 {
        self.chain.unavailable_timing_passes()
    }
    pub fn dropped_timing_frames(&self) -> u64 {
        self.chain.dropped_timing_frames()
    }
    /// Geometry-only custom hosts finish explicitly; output composition does this automatically.
    pub fn finish_timing(
        &self,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), worldrunner_render_gpu::submission::TransactionError> {
        self.chain.finish_timing(encoder)
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
        frame: &crate::frame::FrameUniform,
        shadow: Option<&wgpu::BindGroup>,
        encoder: &wgpu::CommandEncoder,
    ) -> Result<(), FrameError> {
        if let Some(water) = &self.water {
            crate::water::surface::validate_inputs(water)?;
        }
        self.publish_snapshots(encoder, crate::water::planning::SnapshotRequest::default());
        self.water_state.borrow_mut().feature = self.water.as_ref().map(|water| {
            water.feature(
                device,
                frame,
                (self.elapsed_seconds, self.frame_slot),
                shadow,
            )
        });
        Ok(())
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
        let config = self.targets().config();
        let (frame, shadow) = input;
        self.validate_environment_conventions(frame.specular_convention)?;
        let (passes, views) = plan;
        let mut passes = passes.to_vec();
        let mut views = views.to_vec();
        worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?;
        self.set_water_input(device, &frame, shadow, encoder)?;
        let mut combined = reborrow_features(features);
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
        let water = self
            .water
            .as_ref()
            .map(|water| crate::ssr::water::Inputs::new(water, self.frame_slot));
        let mut reflection = self
            .ssr
            .as_ref()
            .map(|ssr| ssr.feature(&frame, (&passes, &views), config, water))
            .transpose()?;
        if let Some(reflection) = &mut reflection {
            let declaration = reflection.schedule(&mut passes, &mut views)?;
            combined.push(BorrowedFeature {
                declaration,
                prepared: reflection,
            });
        }
        let retained = self.chain.plan_features(&mut passes, &mut views)?;
        let (request, mut snapshot_feature) = Self::plan_snapshots(
            device,
            &mut self.snapshots,
            &(config, frame, self.frame_slot, self.water.is_some()),
            (&mut passes, &mut views),
            (&combined, &retained),
        )?;
        if let Some((declaration, feature)) = &mut snapshot_feature {
            combined.push(BorrowedFeature {
                declaration: declaration.clone(),
                prepared: feature,
            });
        }
        passes.sort_by_key(|pass| pass.slot);
        encode_planned(
            &mut self.chain,
            device,
            encoder,
            (&passes, &views),
            &mut combined,
        )?;
        self.publish_snapshots(encoder, request);
        Ok(())
    }
    fn publish_snapshots(
        &self,
        encoder: &wgpu::CommandEncoder,
        request: crate::water::planning::SnapshotRequest,
    ) {
        let ready = self.snapshot_ready.clone();
        let slot = self.frame_slot;
        publication::encode(encoder, &ready.clone(), move || {
            ready.borrow_mut()[slot] = request;
        })
        .expect("checked transaction encoder");
    }
    fn plan_snapshots(
        device: &wgpu::Device,
        snapshots: &mut Option<[crate::water::snapshots::Snapshots; 3]>,
        input: &(FrameConfig, crate::frame::FrameUniform, usize, bool),
        plan: (&mut Vec<PassDeclaration>, &mut Vec<AttachmentView>),
        features: (&[BorrowedFeature<'_>], &[FeatureDeclaration]),
    ) -> Result<
        (
            crate::water::planning::SnapshotRequest,
            Option<(FeatureDeclaration, crate::water::snapshots::SnapshotFeature)>,
        ),
        FrameError,
    > {
        let reads: Vec<_> = features
            .0
            .iter()
            .map(|feature| &feature.declaration)
            .chain(features.1.iter())
            .flat_map(|feature| &feature.passes)
            .flat_map(|pass| &pass.reads)
            .copied()
            .collect();
        let (config, frame, slot, water) = *input;
        let result = crate::water::planning::snapshot_feature(
            device,
            snapshots,
            (config, &frame, slot),
            plan,
            &reads,
            water,
        )?;
        Ok(result)
    }
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        passes: &[PassDeclaration],
        views: &[AttachmentView],
        features: &mut [BorrowedFeature<'_>],
    ) -> Result<(), FrameError> {
        worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?;
        self.publish_snapshots(encoder, crate::water::planning::SnapshotRequest::default());
        if self.ssao.is_some() || self.water.is_some() || self.ssr.is_some() {
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

fn reborrow_features<'a>(features: &'a mut [BorrowedFeature<'_>]) -> Vec<BorrowedFeature<'a>> {
    features
        .iter_mut()
        .map(|feature| BorrowedFeature {
            declaration: feature.declaration.clone(),
            prepared: &mut *feature.prepared,
        })
        .collect()
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

pub(crate) use scene::SceneFeature;
pub use scene::encode_scene;
mod scene;

#[cfg(test)]
#[path = "compositor_post_tests.rs"]
mod post_tests;

pub(super) fn validate_clear(color: wgpu::Color) -> Result<(), PostError> {
    if [color.r, color.g, color.b, color.a]
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(PostError::UnsupportedProfile(
            "clear color must be finite linear RGBA in 0..=1",
        ));
    }
    Ok(())
}

fn encode_planned(
    chain: &mut PostChain<FrameOutput>,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    plan: (&[PassDeclaration], &[AttachmentView]),
    features: &mut [BorrowedFeature<'_>],
) -> Result<(), FrameError> {
    chain.encode_geometry(
        device,
        encoder,
        &worldrunner_render_post::FramePlan {
            passes: plan.0,
            views: plan.1,
            requested: &[],
        },
        features,
    )
}

fn validate_reflection_extent(
    device: &wgpu::Device,
    config: FrameConfig,
    settings: Option<&worldrunner_render_plan::SceneSsr>,
) -> Result<(), FrameError> {
    if let Some(settings) = settings.filter(|settings| settings.enabled) {
        crate::ssr::validate_extent(config, settings, &device.limits())?;
    }
    Ok(())
}

fn validate_hdr_effects(
    effects: Option<&worldrunner_render_plan::SceneEffects>,
) -> Result<(), PostError> {
    if let Some(effects) = effects {
        if let Some(settings) = effects.ssao() {
            crate::ssao::validate_settings(settings)?;
        }
        crate::frame::validate_stock_effects(effects)
            .map_err(|_| PostError::UnsupportedProfile("scene effects"))?;
    }
    if let Some(settings) = effects.and_then(|effects| effects.ssr()) {
        crate::ssr::validate_settings(settings)?;
    }
    Ok(())
}
