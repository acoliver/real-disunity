mod frozen;
#[path = "geometry_prefix.rs"]
mod geometry_prefix;
mod resources;

use crate::{FrameConfig, FrameTargets, PostError};
pub use resources::{BufferSpec, Resources};

#[path = "chain_stage.rs"]
mod stage;
pub use stage::{Operation, Stage, StageTarget, Variant};

#[derive(Default, Debug)]
pub struct StageSet {
    pub stages: Vec<Stage>,
    pub resources: Resources,
}

impl StageSet {
    fn validate(&self, device: &wgpu::Device) -> Result<(), PostError> {
        let mut identities = Vec::new();
        let mut outputs = 0;
        let mut output_seen = false;
        for stage in self.stages.iter().filter(|stage| stage.enabled) {
            if output_seen {
                return Err(PostError::OutputStage);
            }
            if stage.operation.is_pass() {
                if identities.contains(&stage.id) {
                    return Err(PostError::UnsupportedProfile(
                        "duplicate post stage identity",
                    ));
                }
                identities.push(stage.id);
            }
            if stage.operation.writes_output() {
                outputs += 1;
                output_seen = true;
            }
            if let Some(dispatch) = stage.operation.direct_dispatch()
                && dispatch.iter().any(|&count| {
                    count == 0 || count > device.limits().max_compute_workgroups_per_dimension
                })
            {
                return Err(PostError::Dispatch(dispatch));
            }
        }
        if outputs != 1 {
            return Err(PostError::OutputStage);
        }
        Ok(())
    }
}

/// Creates bindings and a linear schedule after allocation/resize, never encodes or submits.
/// Kernel code, exposure ABI, adaptation rules and LUT contents belong to the provider.
pub trait StageFactory {
    /// Validate stage-specific working-target constraints before any target allocation.
    fn validate_config(
        &self,
        _device: &wgpu::Device,
        _config: FrameConfig,
    ) -> Result<(), PostError> {
        Ok(())
    }
    fn frame_slots(&self) -> usize {
        1
    }
    /// Whether the frame targets carry a velocity attachment for a TAA resolve.
    fn needs_velocity(&self) -> bool {
        false
    }
    /// Whether the frame targets carry the reactive and composition mask of temporal
    /// reconstruction.
    fn needs_temporal_masks(&self) -> bool {
        false
    }
    /// Called once after every slot of a build was attempted, with whether all of them built and
    /// validated. A factory that keeps state shared by the slots promotes or drops what this
    /// build created, so a rejected replacement leaves the live chain's state untouched.
    fn settle_build(&self, _accepted: bool) {}
    fn build_for_slot(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        persistent: &Resources,
        _slot: usize,
    ) -> Result<StageSet, PostError> {
        self.build(device, targets, persistent)
    }

    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        persistent: &Resources,
    ) -> Result<StageSet, PostError>;
}

/// The mask has a consumer only when TAA reconstructs the output grid, that is at a render
/// extent that differs from the output; equal extents are the native TAA path.
fn target_options<F: StageFactory>(factory: &F, config: FrameConfig) -> crate::TargetOptions {
    crate::TargetOptions {
        velocity: factory.needs_velocity(),
        temporal_mask: factory.needs_temporal_masks()
            && config.size != config.output_extent()
            && crate::taa_mask_gpu::masks_enabled(),
    }
}

/// Retained query storage and unread results, shared only across replacements.
#[derive(Clone)]
pub struct TimingState(std::rc::Rc<std::cell::RefCell<Option<crate::PassTimer>>>);

/// Owns scene/MSAA resolve, fixed history, sized resources and stage execution order.
pub struct PostChain<F> {
    targets: FrameTargets,
    persistent: Resources,
    factory: F,
    configured: Vec<StageSet>,
    features: Vec<crate::frame::RegisteredFeature>,
    upload_slots: [Vec<wgpu::Buffer>; 3],
    capture_resources: Option<crate::contracts::ResourceRegistry>,
    geometry_prefix: Option<geometry_prefix::GeometryPrefix>,
    /// The declarations [`Self::plan_prefix_features`] planned for the next geometry prefix, by
    /// feature index.
    planned_prefix: Option<Vec<(usize, crate::FeatureDeclaration)>>,
    frame_slot: usize,
    timing: std::rc::Rc<std::cell::RefCell<Option<crate::PassTimer>>>,
}

impl std::fmt::Debug for TimingState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TimingState")
            .field(
                "support",
                &self.0.borrow().as_ref().map(crate::PassTimer::support),
            )
            .finish()
    }
}
impl TimingState {
    /// Encodes a compiled list with the frame's pass timer.
    pub fn encode_checked(
        &self,
        compiled: &crate::contracts::CompiledOperations,
        registry: &crate::contracts::ResourceRegistry,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), crate::contracts::ContractError> {
        let mut timer = self.0.borrow_mut();
        compiled.encode_checked(registry, encoder, timer.as_mut())
    }
    /// Attach a provider compute pass to the active frame, if timing is enabled.
    pub fn begin_compute_pass<'a>(
        &self,
        encoder: &'a mut wgpu::CommandEncoder,
        id: crate::StageId,
    ) -> wgpu::ComputePass<'a> {
        let stamp = self
            .0
            .borrow_mut()
            .as_mut()
            .and_then(|timer| timer.stamp(crate::PassId::Stage(id)));
        encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(id.name),
            timestamp_writes: stamp.as_ref().map(crate::timing::Stamp::compute),
        })
    }
}

impl<F: StageFactory> PostChain<F> {
    /// Creates frame targets with the depth ownership representation `depth` selects.
    pub fn new(
        device: &wgpu::Device,
        config: FrameConfig,
        persistent: Resources,
        factory: F,
        depth: crate::SceneDepth,
    ) -> Result<Self, PostError> {
        factory.validate_config(device, config)?;
        let targets = FrameTargets::new(device, config, target_options(&factory, config), depth)?;
        let configured = build_slots(&factory, device, &targets, &persistent)?;
        Ok(Self {
            targets,
            persistent,
            factory,
            configured,
            features: Vec::new(),
            capture_resources: None,
            geometry_prefix: None,
            planned_prefix: None,
            upload_slots: std::array::from_fn(|_| Vec::new()),
            frame_slot: 0,
            timing: std::rc::Rc::new(std::cell::RefCell::new(None)),
        })
    }

    /// Reserve physical identities shared by both segments of a stock capture.
    pub fn capture_write_keys(
        &mut self,
        textures: &[&wgpu::Texture],
    ) -> Result<
        Vec<worldrunner_render_gpu::submission::capture::WriteKey>,
        crate::contracts::ContractError,
    > {
        let textures: Vec<_> = textures.iter().map(|texture| (*texture, None)).collect();
        self.capture_write_keys_initial(&textures)
    }

    /// As [`Self::capture_write_keys`], for textures whose allocation the frame imports with an
    /// initial version: `Some(0)` for a texture that holds contents an earlier submission left,
    /// as the stock compile imports it when any of its views declares them. A capture and the
    /// frame it submits must import a texture the same way, or the frame's own import conflicts
    /// with the capture's.
    pub fn capture_write_keys_initial(
        &mut self,
        textures: &[(&wgpu::Texture, Option<u32>)],
    ) -> Result<
        Vec<worldrunner_render_gpu::submission::capture::WriteKey>,
        crate::contracts::ContractError,
    > {
        use crate::contracts::{ResourceLifetime, Semantic};
        let registry = self.capture_resources.get_or_insert_with(Default::default);
        textures
            .iter()
            .map(|(texture, initial)| {
                let key = registry.import(
                    texture,
                    ResourceLifetime::FrameLocal,
                    Semantic::Unspecified,
                    *initial,
                    &[],
                )?;
                Ok(
                    worldrunner_render_gpu::submission::capture::WriteKey::Resource {
                        id: key.id(),
                        generation: key.generation(),
                    },
                )
            })
            .collect()
    }
    pub fn clear_capture_resources(&mut self) {
        self.capture_resources = None;
        self.geometry_prefix = None;
    }

    /// Build replacements before switching; fixed history remains unchanged on resize/failure.
    pub fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) -> Result<bool, PostError> {
        if self.targets.config().size == size {
            return Ok(false);
        }
        let config = FrameConfig {
            size,
            ..self.targets.config()
        };
        self.replace_targets(device, config)?;
        Ok(true)
    }

    /// Replace both extents atomically. Failed validation preserves targets and bindings.
    pub fn resize_extents(
        &mut self,
        device: &wgpu::Device,
        render: [u32; 2],
        output: [u32; 2],
    ) -> Result<(), PostError> {
        let config = FrameConfig {
            size: render,
            output_size: Some(output),
            ..self.targets.config()
        };
        self.replace_targets(device, config)
    }

    fn replace_targets(
        &mut self,
        device: &wgpu::Device,
        config: FrameConfig,
    ) -> Result<(), PostError> {
        config.validate_device(device)?;
        self.factory.validate_config(device, config)?;
        let targets = FrameTargets::new(
            device,
            config,
            target_options(&self.factory, config),
            self.targets.scene_depth(),
        )?;
        let configured = build_slots(&self.factory, device, &targets, &self.persistent)?;
        for feature in &mut self.features {
            feature.prepared = None;
        }
        self.geometry_prefix = None;
        self.targets = targets;
        self.configured = configured;
        Ok(())
    }

    /// Select only a reserved slot whose preceding submission has completed.
    /// Immutable staging allocations remain owned by this slot until its next use.
    pub fn select_frame_slot(&mut self, slot: usize) {
        assert!(slot < self.upload_slots.len(), "frame slot out of range");
        self.frame_slot = slot;
    }

    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        for stage in &self.configured[self.frame_slot % self.configured.len()].stages {
            let stamp = if stage.enabled && stage.operation.is_pass() {
                self.stamp(crate::PassId::Stage(stage.id))
            } else {
                None
            };
            stage.encode(encoder, output, stamp.as_ref());
        }
    }

    /// Retain unread query results when only attachment configuration changes.
    pub fn timing_state(&self) -> TimingState {
        TimingState(self.timing.clone())
    }
    pub fn restore_timing(&mut self, state: TimingState) {
        self.timing = state.0;
    }

    /// Enables timing only after device capability negotiation.
    pub fn configure_timing(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mode: crate::TimingMode,
    ) -> Result<crate::TimingSupport, crate::TimingError> {
        let timer = crate::PassTimer::new(device, queue, mode)?;
        let support = timer.support();
        *self.timing.borrow_mut() = Some(timer);
        Ok(support)
    }
    pub fn begin_timing(
        &self,
        frame_id: u64,
    ) -> Result<(), worldrunner_render_gpu::submission::TransactionError> {
        worldrunner_render_gpu::submission::Publications::current()
            .ok_or(worldrunner_render_gpu::submission::TransactionError::Missing)?;
        if let Some(timer) = self.timing.borrow_mut().as_mut() {
            timer.begin_frame(frame_id);
        }
        Ok(())
    }
    pub fn finish_timing(
        &self,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), worldrunner_render_gpu::submission::TransactionError> {
        worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?;
        if let Some(timer) = self.timing.borrow_mut().as_mut() {
            timer.finish_frame(encoder);
        }
        Ok(())
    }
    pub fn take_timings(&self) -> Result<Vec<crate::FrameTiming>, crate::TimingError> {
        self.timing
            .borrow_mut()
            .as_mut()
            .map_or_else(|| Ok(Vec::new()), crate::PassTimer::take_ready)
    }
    pub fn dropped_timing_frames(&self) -> u64 {
        self.timing
            .borrow()
            .as_ref()
            .map_or(0, crate::PassTimer::dropped_frames)
    }
    pub fn unavailable_timing_passes(&self) -> u64 {
        self.timing
            .borrow()
            .as_ref()
            .map_or(0, crate::PassTimer::unavailable_passes)
    }
    fn stamp(&self, pass: crate::PassId) -> Option<crate::timing::Stamp> {
        self.timing
            .borrow_mut()
            .as_mut()
            .and_then(|timer| timer.stamp(pass))
    }
    /// Resolve is an executor pass, including when used by encoder-only hosts.
    pub fn encode_resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        let stamp = self
            .targets
            .resolve()
            .and_then(|_| self.stamp(crate::PassId::Resolve));
        self.targets.encode_resolve(encoder, stamp.as_ref());
    }

    pub const fn targets(&self) -> &FrameTargets {
        &self.targets
    }
    pub const fn persistent(&self) -> &Resources {
        &self.persistent
    }
    pub const fn factory(&self) -> &F {
        &self.factory
    }

    /// Reconfigure shaders/options and all dependent bindings without resetting fixed history.
    pub fn reconfigure(&mut self, device: &wgpu::Device, factory: F) -> Result<(), PostError> {
        factory.validate_config(device, self.targets.config())?;
        let configured = build_slots(&factory, device, &self.targets, &self.persistent)?;
        self.geometry_prefix = None;
        self.factory = factory;
        self.configured = configured;
        Ok(())
    }
}

impl<F: StageFactory> PostChain<F> {
    pub fn registered_feature_count(&self) -> usize {
        self.features.len()
    }

    /// Registration retains declarations without building resources.
    /// Registration order breaks ties within each named slot.
    pub fn register(
        &mut self,
        _device: &wgpu::Device,
        feature: Box<dyn crate::FrameFeature>,
    ) -> Result<crate::FeatureId, crate::FrameError> {
        let declaration = feature.declare();
        let id = crate::FeatureId(self.features.len());
        self.features.push(crate::frame::RegisteredFeature {
            declaration,
            prepared: None,
            source: feature,
            prefix: false,
        });
        Ok(id)
    }

    /// Collects retained feature plans before resource validation or preparation.
    pub fn plan_features(
        &mut self,
        passes: &mut Vec<crate::PassDeclaration>,
        views: &mut Vec<crate::AttachmentView>,
    ) -> Result<Vec<crate::FeatureDeclaration>, crate::FrameError> {
        let declarations = self
            .features
            .iter()
            .map(|feature| feature.source.plan(passes, views))
            .collect::<Result<Vec<_>, _>>()?;
        for (feature, declaration) in self.features.iter_mut().zip(&declarations) {
            feature.declaration = declaration.clone();
        }
        Ok(declarations)
    }

    /// Records the declarations that [`Self::plan_features`] returned for an earlier frame, for a
    /// frame that reuses that plan: validation reads the retained declarations, and a reused plan
    /// does not plan the features again.
    pub fn adopt_planned_features(&mut self, declarations: &[crate::FeatureDeclaration]) {
        debug_assert_eq!(self.features.len(), declarations.len());
        for (feature, declaration) in self.features.iter_mut().zip(declarations) {
            feature.declaration = declaration.clone();
        }
    }

    /// Prepares every feature of the frame and records what they staged for the frame's
    /// preparation, returning what they staged for its compute passes.
    fn prepare_features(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        borrowed: &mut [crate::BorrowedFeature<'_>],
        (plan, raster, declarations): (
            &crate::FramePlan<'_>,
            &crate::contracts::StockRaster,
            &[crate::FeatureDeclaration],
        ),
        completed: &[crate::PassEntry],
    ) -> Result<crate::frame::ComputeStaging, crate::FrameError> {
        let replacements = self
            .features
            .iter()
            .map(|feature| {
                if feature.prepared.is_none() {
                    feature
                        .source
                        .build(&crate::BuildContext {
                            device,
                            targets: &self.targets,
                        })
                        .map(Some)
                } else {
                    Ok(None)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (feature, replacement) in self.features.iter_mut().zip(replacements) {
            if let Some(prepared) = replacement {
                feature.prepared = Some(prepared);
            }
        }
        let mut context = crate::PrepareContext::new(device, encoder)?;
        context.open_preparation_of(plan, completed);
        context.open_compute_passes(declarations);
        for feature in borrowed.iter_mut() {
            feature.prepared.prepare(&mut context)?;
        }
        for feature in self.active_features(completed) {
            feature
                .prepared
                .as_mut()
                .expect("validated feature build")
                .prepare(&mut context)?;
        }
        context.close_preparation(raster)?;
        for feature in borrowed.iter_mut() {
            feature.prepared.commit(&mut context);
        }
        for feature in self.active_features(completed) {
            feature
                .prepared
                .as_mut()
                .expect("validated feature build")
                .commit(&mut context);
        }
        let compute = context.take_compute_passes();
        self.upload_slots[self.frame_slot] = context.finish_preparation()?;
        Ok(compute)
    }

    /// Encode a complete borrowed subplan without invoking retained frame features, apart from
    /// the prefix participants that [`Self::plan_prefix_features`] planned into it.
    /// Used for metadata prefixes; the same validation and raster compiler apply.
    pub fn encode_geometry_prefix(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        plan: &crate::FramePlan<'_>,
        borrowed: &mut [crate::BorrowedFeature<'_>],
    ) -> Result<(), crate::FrameError> {
        self.geometry_prefix = None;
        let parked = self.park_non_prefix_features();
        let result = self.encode_geometry(device, encoder, plan, borrowed);
        self.restore_parked_features(parked);
        if result.is_ok() {
            let frame = worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?;
            self.geometry_prefix = Some(geometry_prefix::GeometryPrefix::new(frame, plan));
        }
        result
    }

    /// Validates the complete frame before any feature prepares or draws.
    /// Borrowed resident features precede retained registrations in a slot.
    pub fn encode_geometry(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        plan: &crate::FramePlan<'_>,
        borrowed: &mut [crate::BorrowedFeature<'_>],
    ) -> Result<(), crate::FrameError> {
        let declarations = self.declarations(borrowed);
        crate::frame::validate_plan(
            plan.passes,
            &declarations,
            plan.requested,
            plan.views,
            plan.operations,
        )?;
        self.encode_validated(device, encoder, plan, borrowed, &declarations)
    }

    /// Encodes a plan that has passed validation.
    fn encode_validated(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        plan: &crate::FramePlan<'_>,
        borrowed: &mut [crate::BorrowedFeature<'_>],
        declarations: &[crate::FeatureDeclaration],
    ) -> Result<(), crate::FrameError> {
        let crate::FramePlan {
            passes,
            views,
            operations,
            ..
        } = *plan;
        let completed = self.completed_geometry_prefix(encoder, plan)?;
        let mut local_registry = crate::contracts::ResourceRegistry::default();
        let registry = self
            .capture_resources
            .as_mut()
            .unwrap_or(&mut local_registry);
        let raster =
            crate::contracts::stock::compile(device, passes, views, declarations, registry)?;
        let compute = self.prepare_features(
            device,
            encoder,
            borrowed,
            (plan, &raster, declarations),
            &completed,
        )?;
        let timing = self.timing_state();
        let mut groups = crate::frame::OperationsSchedule::new(passes, operations);
        for (index, declaration) in passes.iter().enumerate() {
            let entry = declaration.entry();
            if completed.contains(&entry) {
                continue;
            }
            groups.encode_before(index, &timing, encoder)?;
            let stamp = self.stamp(crate::PassId::Geometry(entry));
            if declaration.compute.is_some() {
                raster.encode_stock_compute(index, entry, encoder, stamp.as_ref(), |pass| {
                    compute.record(entry, pass)
                })?;
                continue;
            }
            let context = crate::DrawContext::new(entry);
            raster.encode_stock_pass(index, entry, encoder, stamp.as_ref(), |pass| {
                for feature in borrowed.iter().filter(|feature| {
                    feature
                        .declaration
                        .passes
                        .iter()
                        .any(|pass| pass.entry() == entry)
                }) {
                    feature.prepared.draw(&context, pass);
                }
                for feature in self.features.iter().filter(|feature| {
                    feature
                        .declaration
                        .passes
                        .iter()
                        .any(|pass| pass.entry() == entry)
                }) {
                    feature
                        .prepared
                        .as_ref()
                        .expect("validated feature build")
                        .draw(&context, pass);
                }
            })?;
        }
        groups.encode_rest(&timing, encoder)?;
        self.finish_feature_draws(device, encoder, borrowed, &completed)
    }

    fn finish_feature_draws(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        borrowed: &mut [crate::BorrowedFeature<'_>],
        completed: &[crate::PassEntry],
    ) -> Result<(), crate::FrameError> {
        let mut context = crate::PrepareContext::new(device, encoder)?;
        for feature in borrowed {
            feature.prepared.finish_draws(&mut context);
        }
        for feature in self.active_features(completed) {
            feature
                .prepared
                .as_mut()
                .expect("validated feature build")
                .finish_draws(&mut context);
        }
        self.upload_slots[self.frame_slot].extend(context.finish());
        Ok(())
    }
}

fn build_slots<F: StageFactory>(
    factory: &F,
    device: &wgpu::Device,
    targets: &FrameTargets,
    persistent: &Resources,
) -> Result<Vec<StageSet>, PostError> {
    if !matches!(factory.frame_slots(), 1 | 3) {
        return Err(PostError::UnsupportedProfile("post frame slot count"));
    }
    let built = (0..factory.frame_slots())
        .map(|slot| {
            let mut set = factory.build_for_slot(device, targets, persistent, slot)?;
            set.validate(device)?;
            for stage in &mut set.stages {
                stage.label = stage.id.diagnostic_label();
            }
            Ok(set)
        })
        .collect::<Result<Vec<_>, PostError>>();
    factory.settle_build(built.is_ok());
    built
}

#[cfg(test)]
#[path = "post_effect_tests.rs"]
mod post_effect_tests;

#[cfg(test)]
#[path = "chain_aa_tests.rs"]
mod aa_tests;

#[cfg(test)]
#[path = "chain_bloom_tests.rs"]
mod bloom_tests;

#[cfg(test)]
#[path = "hdr_taa_tests.rs"]
mod taa_wiring_tests;
