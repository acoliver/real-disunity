//! Stock composition uses the same feature and attachment boundary as external draws.
mod capture;
mod contours;
#[cfg(test)]
mod overlay_target_gpu_tests;
#[cfg(test)]
mod overlay_tests;
pub mod terms;
pub use capture::FeatureCapture;
pub use capture::FeatureTarget;
mod clock;
mod configuration;
mod depth_prefix;
pub(crate) mod depth_reuse;
mod environment;
pub(crate) mod forward_order;
mod frozen;
#[cfg(test)]
mod frozen_tests;
mod game;
mod game_screen;
#[cfg(test)]
mod game_screen_tests;
pub use game_screen::GameScreenComposite;
mod game_screen_frame;
pub use game_screen_frame::GameScreenFrame;
pub use game_screen_frame::GameScreenPass;
mod hdr;
pub(crate) mod offscreen;
mod output;
mod overlay;
mod overlay_depth;
#[cfg(test)]
pub(crate) mod plan_trace;
mod programs;
pub(crate) mod publication;
mod snapshots;
mod stock_plan;
mod temporal;
mod timing;
mod validation;
mod water_draws;
pub use crate::scene_metadata::receiver::ReceiverGradientReservation;
pub use configuration::ConfiguredFrameRenderer;
pub use configuration::FrameEncoding;
pub use configuration::RendererConfiguration;
pub use game::BRDF_TABLE_SAMPLES;
pub use game::BRDF_TABLE_SIZE;
pub use game::CascadeSelection;
pub use game::ColorOutput;
pub use game::DepthAccess;
pub use game::EngineConstants;
pub use game::EnginePlaceholder;
pub use game::EnvironmentConsumer;
pub use game::FULLSCREEN_TRIANGLE_WGSL;
pub use game::FrameCamera;
pub use game::FrameCameraHandle;
pub use game::FrameEnvironment;
pub use game::FrameEnvironmentHandle;
pub use game::FrameInputError;
pub use game::FrameOcclusion;
pub use game::FrameOcclusionHandle;
pub use game::FrameSceneMeshError;
pub use game::FrameSceneMeshes;
pub use game::FrameSceneMeshesHandle;
pub use game::FrameSceneTextureError;
pub use game::FrameSceneTextures;
pub use game::FrameSceneTexturesHandle;
pub use game::FrameShadowHandle;
pub use game::FrameShadows;
pub use game::FrameSnapshotError;
pub use game::FrameSnapshotHandle;
pub use game::FrameSnapshots;
pub use game::FrameTerrainError;
pub use game::FrameTerrainHandle;
pub use game::FrameWaterNormalError;
pub use game::GameBlockDestination;
pub use game::GameBuffer;
pub use game::GameBufferDeclaration;
pub use game::GameBufferUsage;
pub use game::GameBuild;
pub use game::GameComputeSpec;
pub use game::GameFeatureFactory;
pub use game::GameLevel;
pub use game::GamePassSpec;
pub use game::GamePasses;
pub use game::GameReadback;
pub use game::GameResourceError;
pub use game::GameResources;
pub use game::GameSampler;
pub use game::GameTargets;
pub use game::GameTexture;
pub use game::GameTextureDeclaration;
pub use game::GameTextureLevel;
pub use game::LayerShape;
pub use game::LevelAccess;
pub use game::OcclusionDepth;
pub use game::PassTarget;
pub use game::PostWaterDepth;
pub use game::SceneMeshSlot;
pub use game::ShadowCascade;
pub use game::SnapshotImage;
pub use game::SnapshotReads;
pub use game::SnapshotStatistics;
pub use game::StockReplacement;
pub use game::TargetDeclaration;
pub use game::TargetExtent;
pub use game::TargetLifetime;
pub use game::TerrainMaterialArrayError;
pub use game::TerrainMaterialChannel;
#[cfg(test)]
pub(crate) use game::declaring_stock;
pub use game::join_pass;
pub use game::prepass_writes_frame_depth;
#[cfg(test)]
pub(crate) use game::terrain_probe;
use output::FrameOutput;
pub use output::FrameProfile;
pub(crate) use stock_plan::SceneInputs;
pub(crate) use stock_plan::ShadowSchedule;
pub use stock_plan::StockPlanStatistics;
pub(crate) use stock_plan::unshadowed;
use validation::validate_clear;
use validation::validate_reflection_extent;
use worldrunner_render_post::AntiAliasing;
use worldrunner_render_post::AttachmentDeclaration;
use worldrunner_render_post::AttachmentLoad;
use worldrunner_render_post::AttachmentView;
use worldrunner_render_post::BorrowedFeature;
use worldrunner_render_post::CopyOutput;
use worldrunner_render_post::FrameConfig;
use worldrunner_render_post::FrameError;
use worldrunner_render_post::MsaaSamples;
use worldrunner_render_post::PassDeclaration;
use worldrunner_render_post::PostChain;
use worldrunner_render_post::PostError;
use worldrunner_render_post::PostResolve;
use worldrunner_render_post::Resources;

/// Retained executor for transaction-bound composition. Submission belongs to the host.
/// Select an admitted slot before preparation and reuse it only after completion.
/// Encoding requires the authority-owned encoder. Cancellation discards publication.
pub struct FrameRenderer {
    chain: PostChain<FrameOutput>,
    contours: crate::contours::Targets,
    contour_format: wgpu::TextureFormatFeatures,
    overlay: overlay::OverlayStage,
    output_format: wgpu::TextureFormat,
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
    pub(crate) receiver_gradients: ReceiverGradientReservation,
    #[cfg(test)]
    pub(crate) receiver_reference: Option<crate::scene_metadata::receiver::ReceiverReference>,
    animation_time: Option<u32>,
    frame_delta: f32,
    post_time: std::rc::Rc<std::cell::Cell<f32>>,
    exposure_reset: std::rc::Rc<std::cell::Cell<bool>>,
    temporal: std::rc::Rc<std::cell::RefCell<temporal::TemporalHistory>>,
    pub(crate) ssao: Option<crate::scene_metadata::SceneMetadata>,
    pub(crate) ssr: Option<crate::ssr::Ssr>,
    ssao_requested: Option<worldrunner_render_plan::SceneSsao>,
    ssr_requested: Option<worldrunner_render_plan::SceneSsr>,
    /// The stock plans of this renderer and the ones it replaces on reconfiguration.
    plan_cache: std::rc::Rc<std::cell::RefCell<stock_plan::PlanCache>>,
    /// What the game inventory installed on this generation.
    game: game::Installed,
    /// The camera of the frame being encoded, which the game's features read.
    game_camera: FrameCameraHandle,
    /// The streamed terrain of the frame being encoded, which the game's features read.
    game_terrain: FrameTerrainHandle,
    /// Where every route lends the scene's resident textures to the game's features.
    game_textures: game::FrameSceneTexturesHandle,
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
    /// What the stock frame plan cache did: schedules rebuilt, binding tables resolved and
    /// frames that reused both.
    pub fn stock_plan_statistics(&self) -> StockPlanStatistics {
        self.plan_cache.borrow().statistics()
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
            contours: crate::contours::Targets::default(),
            contour_format: crate::contours::FORMAT.guaranteed_format_features(device.features()),
            overlay: overlay::OverlayStage::default(),
            output_format,
            environment_conventions: [None; 2],
            ssao: None,
            ssr: None,
            ssao_requested: None,
            ssr_requested: None,
            plan_cache: std::rc::Rc::default(),
            game: game::Installed::default(),
            game_camera: FrameCameraHandle::default(),
            game_terrain: FrameTerrainHandle::default(),
            game_textures: game::FrameSceneTexturesHandle::default(),
            water: None,
            water_state: std::rc::Rc::default(),
            snapshots: None,
            snapshot_ready: std::rc::Rc::default(),
            frame_slot: 0,
            clear_color: wgpu::Color::BLACK,
            post_requested: worldrunner_render_post::HdrPostSettings::default(),
            elapsed_seconds: 0.0,
            selected_time: None,
            receiver_gradients: ReceiverGradientReservation::Opposing,
            #[cfg(test)]
            receiver_reference: None,
            animation_time: None,
            frame_delta: 0.0,
            post_time: std::rc::Rc::new(std::cell::Cell::new(0.0)),
            exposure_reset: std::rc::Rc::new(std::cell::Cell::new(false)),
            temporal: std::rc::Rc::default(),
            chain: PostChain::new(
                device,
                config,
                Resources::default(),
                FrameOutput::Copy(CopyOutput::new(device, output_format)),
                worldrunner_render_post::SceneDepth::Independent,
            )?,
        })
    }
    /// HDR exposure is taken from the scene contract, never applied by geometry.
    ///
    /// `samples` must be 1, 2, 4 or 8, and post antialiasing comes only from `effects`.
    /// Use [`Self::hdr_antialiased`] to select both with one typed descriptor.
    pub fn hdr(
        context: &crate::device::GpuContext,
        size: [u32; 2],
        output_format: wgpu::TextureFormat,
        samples: u32,
        tone_map: worldrunner_render_post::ToneMap,
        effects: Option<&worldrunner_render_plan::SceneEffects>,
    ) -> Result<Self, PostError> {
        let antialiasing = AntiAliasing::new(MsaaSamples::try_from(samples)?, PostResolve::None);
        Self::hdr_antialiased(
            context,
            size,
            output_format,
            antialiasing,
            tone_map,
            effects,
        )
    }

    /// As [`Self::hdr`], with the multisample count and post resolve chosen by `antialiasing`.
    ///
    /// A sample count the adapter cannot render for the scene formats fails with
    /// [`PostError::SampleCount`] before any target is allocated; it is never reduced.
    /// A resolve other than [`PostResolve::None`] conflicts with a resolve
    /// authored in `effects` and is rejected rather than merged.
    ///
    /// A TAA resolve turns on sub-pixel projection jitter and previous-frame view data
    /// ([`PostResolve::effective_jitter`]); see [`Self::set_projection_jitter`].
    pub fn hdr_antialiased(
        context: &crate::device::GpuContext,
        size: [u32; 2],
        output_format: wgpu::TextureFormat,
        antialiasing: AntiAliasing,
        tone_map: worldrunner_render_post::ToneMap,
        effects: Option<&worldrunner_render_plan::SceneEffects>,
    ) -> Result<Self, PostError> {
        hdr::build(
            context,
            size,
            output_format,
            antialiasing,
            tone_map,
            effects,
        )
    }

    fn install_reflections(&mut self, device: &wgpu::Device) -> Result<(), FrameError> {
        if let Some(settings) = self.ssr_requested.filter(|settings| settings.enabled) {
            crate::ssr::validate_extent(self.targets().config(), &settings, &device.limits())?;
            let config = self.targets().config();
            let velocity = self.taa_resident();
            let screen = self
                .ssao
                .as_mut()
                .expect("enabled reflections own prepass metadata");
            self.ssr = Some(crate::ssr::Ssr::with_velocity(
                device, config, settings, screen, velocity,
            )?);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn install_water(
        &mut self,
        context: &crate::device::GpuContext,
        settings: worldrunner_render_plan::SceneWaterBody,
    ) -> Result<(), FrameError> {
        self.install_water_bodies(context.device(), context.submission(), vec![settings])
    }

    /// Installs independent water surfaces in the existing transparent pass.
    /// An empty set leaves the feature disabled. Replacing a resident set requires a rebuild.
    /// Water draws its bodies on a runtime of its own, made from `authority`; every frame it
    /// draws into must belong to that authority.
    pub fn install_water_bodies(
        &mut self,
        device: &wgpu::Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        settings: Vec<worldrunner_render_plan::SceneWaterBody>,
    ) -> Result<(), FrameError> {
        if self.water.is_some() {
            return Err(FrameError::WaterSettings("water is already installed"));
        }
        if settings.is_empty() {
            return Ok(());
        }
        let water = crate::water::Water::new(device, authority, self.targets().config(), settings)?;
        *self.snapshot_ready.borrow_mut() = Default::default();
        self.snapshots = Some(water.snapshots.clone());
        self.register_water(device)?;
        self.water = Some(water);
        Ok(())
    }

    pub(crate) fn bind_water_inputs(
        &mut self,
        inputs: &[[Option<wgpu::TextureView>; 3]],
        counter: crate::draw::pass_statistics::PassCounter,
    ) {
        if let Some(water) = &mut self.water {
            water.surface_inputs = inputs.to_vec();
            water.draw_count = counter;
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
    /// Existing pixel-centre linear metre depth and normal coverage for shadow
    /// statistics. Callers rebind after resize; no additional prepass is allocated.
    #[cfg(test)]
    pub(crate) fn depth_statistics_source(
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
        config.validate_device(device)?;
        let mut replacement = self
            .ssao
            .as_ref()
            .filter(|ssao| ssao.size != size)
            .map(|ssao| ssao.rebuilt(device, size))
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
            self.ensure_game_snapshots(device);
            *self.snapshot_ready.borrow_mut() = Default::default();
            self.reset_temporal_history();
        }
        if let Some(replacement) = replacement {
            self.ssao = Some(replacement);
        }
        self.share_metadata_depth(device);
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
            self.prepare_taa(factory, encoder)?;
            let reset = self.exposure_reset.clone();
            publication::encode(encoder, &reset.clone(), move || {
                reset.set(false);
            })
            .map_err(FrameError::from)?;
        }
        self.encode_resolve(encoder);
        self.chain.encode(encoder, output);
        self.overlay.encode(encoder, output);
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
    /// Resolves retained scene samples into the resolved scene target, timed with the frame's
    /// passes. [`Self::encode_output`] does this first; a host that reads the resolved scene
    /// without an output encodes it alone.
    pub fn encode_resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        self.chain.encode_resolve(encoder);
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
        frame: &crate::frame::FrameUniform,
        shadow: Option<&wgpu::BindGroup>,
        encoder: &wgpu::CommandEncoder,
        depth: crate::renderer::depth::DepthMode,
    ) -> Result<(), FrameError> {
        if let Some(water) = self.stock_water() {
            crate::water::surface::validate_inputs(water)?;
        }
        self.publish_snapshots(encoder, crate::water::planning::SnapshotRequest::default());
        let feature = self
            .stock_water()
            .map(|water| {
                water.feature(
                    encoder,
                    frame,
                    (self.elapsed_seconds, self.frame_slot),
                    shadow,
                    depth,
                )
            })
            .transpose()
            .map_err(|error| FrameError::FeaturePreparation {
                feature: "water",
                reason: error.to_string(),
            })?;
        self.water_state.borrow_mut().feature = feature;
        Ok(())
    }
    /// Camera-aware composition for stock and external features, including
    /// declaration-requested snapshots. The optional binding in `input` supplies shared shadows.
    ///
    /// `head` is work that records commands of its own, encoded by the executor immediately
    /// before the first pass of the ordered frame. A `None` head supplies no work; it does not
    /// discover operations retained by borrowed features. Forward any
    /// [`StockSceneFeature::take_offscreen_operations`] result here.
    ///
    /// # Errors
    /// Validation and preparation errors, or an error returned while recording `head`. Abandon
    /// the frame on failure.
    pub fn encode_frame(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        plan: (&[PassDeclaration], &[AttachmentView]),
        head: Option<&dyn worldrunner_render_post::FrameOperations>,
        features: &mut [BorrowedFeature<'_>],
        input: &(crate::frame::FrameUniform, Option<&wgpu::BindGroup>),
    ) -> Result<(), FrameError> {
        let config = self.targets().config();
        let (frame, shadow) = *input;
        self.game_camera
            .set(&FrameCamera::of(&frame, self.frame_slot));
        self.validate_environment_conventions(frame.specular_convention)?;
        let listed = self.with_prefix_prepass(encoder, plan)?;
        let (passes, views) = listed.as_ref().map_or(plan, |(passes, views)| {
            (passes.as_slice(), views.as_slice())
        });
        worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?;
        self.publish_game_inputs();
        let depth_mode = if passes
            .iter()
            .find(|pass| pass.slot == worldrunner_render_post::PassSlot::Opaque)
            .and_then(|pass| pass.depth.as_ref())
            .is_some_and(|depth| depth.format.has_stencil_aspect())
        {
            crate::renderer::depth::DepthMode::Shared
        } else {
            crate::renderer::depth::DepthMode::Independent
        };
        self.set_water_input(&frame, shadow, encoder, depth_mode)?;
        let declarations: Vec<_> = features
            .iter()
            .map(|feature| feature.declaration.clone())
            .collect();
        let environment = stock_plan::Environment {
            config,
            frame,
            slot: self.frame_slot,
            ssao: self.ssao.as_ref(),
            ssr: self.ssr.as_ref(),
            water: self.game.replaces.stock_water(self.water.as_ref()),
            velocity: self.chain.targets().velocity_texture().cloned(),
            requested: (self.ssao_requested, self.ssr_requested),
            overlay_depth: self.overlay.needs_depth(),
            shadow: shadow.is_some(),
        };
        let authored = stock_plan::Authored {
            passes,
            views,
            features: &declarations,
        };
        let parts = stock_plan::Parts {
            cache: &self.plan_cache,
            chain: &mut self.chain,
            snapshots: &mut self.snapshots,
            overlay: &mut self.overlay,
        };
        let mut planned = stock_plan::plan_frame(device, parts, &environment, &authored)?;
        let (schedule, views) = (planned.schedule.clone(), planned.views.clone());
        let views = views.as_slice();
        self.publish_game_snapshots(schedule.request);
        let head = head.map(|work| worldrunner_render_post::OperationsGroup {
            before: schedule.first,
            work,
        });
        {
            let mut combined = planned.features(features);
            frozen::encode_frozen_plan(
                &mut self.chain,
                device,
                encoder,
                &worldrunner_render_post::FramePlan {
                    passes: &schedule.passes,
                    views,
                    requested: &[],
                    operations: head.as_slice(),
                },
                &mut combined,
            )?;
        }
        self.record_game_work(encoder)?;
        self.publish_snapshots(encoder, schedule.request);
        self.stage_temporal_history(encoder, &frame)
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
        self.remember_taa_block(None);
        if self.ssao.is_some() || self.stock_water().is_some() || self.ssr.is_some() {
            return Err(FrameError::UnsupportedEffect(
                "SSAO requires stock geometry composition",
            ));
        }
        self.publish_game_inputs();
        self.chain.encode_geometry(
            device,
            encoder,
            &worldrunner_render_post::FramePlan {
                passes,
                views,
                requested: &[],
                operations: &[],
            },
            features,
        )?;
        self.record_game_work(encoder)
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
        read_only: false,
    }
}

pub use depth_prefix::StockDepthPrefix;
pub(crate) use scene::ColorDepth;
pub(crate) use scene::SceneFeature;
pub use scene::StockSceneFeature;
pub(crate) use scene::declare_velocity;
pub use scene::prepare_stock_scene;
mod scene;

#[cfg(test)]
#[path = "compositor_post_tests.rs"]
mod post_tests;

#[cfg(test)]
#[path = "compositor_repeated_opaque_tests.rs"]
mod repeated_opaque_tests;
