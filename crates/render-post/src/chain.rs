#[path = "geometry_prefix.rs"]
mod geometry_prefix;

use wgpu::util::DeviceExt;

use crate::{FrameConfig, FrameTargets, PostError};

/// Opaque caller-authored ABI. The chain allocates these once; resize never resets history.
#[derive(Clone, Debug)]
pub struct BufferSpec<'a> {
    pub label: &'a str,
    pub size: u64,
    pub usage: wgpu::BufferUsages,
    pub initial: &'a [u8],
}

/// Explicit owner for fixed history/config buffers and stage-local sized allocations.
#[derive(Default, Debug)]
pub struct Resources {
    pub buffers: Vec<wgpu::Buffer>,
    pub textures: Vec<wgpu::Texture>,
}

impl Resources {
    pub fn new(device: &wgpu::Device, specs: &[BufferSpec<'_>]) -> Result<Self, PostError> {
        let mut resources = Self::default();
        for spec in specs {
            if spec.size == 0
                || spec.size > device.limits().max_buffer_size
                || u64::try_from(spec.initial.len()).unwrap_or(u64::MAX) > spec.size
                || spec.size % wgpu::COPY_BUFFER_ALIGNMENT != 0
            {
                return Err(PostError::BufferSize(spec.size));
            }
            let buffer = if spec.initial.is_empty() {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(spec.label),
                    size: spec.size,
                    usage: spec.usage,
                    mapped_at_creation: false,
                })
            } else {
                let mut bytes = vec![
                    0;
                    usize::try_from(spec.size)
                        .map_err(|_| PostError::BufferSize(spec.size))?
                ];
                bytes[..spec.initial.len()].copy_from_slice(spec.initial);
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(spec.label),
                    contents: &bytes,
                    usage: spec.usage,
                })
            };
            resources.buffers.push(buffer);
        }
        Ok(resources)
    }
}

#[path = "chain_stage.rs"]
mod stage;
pub use stage::{Operation, Stage, StageTarget};

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
            match &stage.operation {
                Operation::Fullscreen {
                    target: StageTarget::Output,
                    ..
                } => {
                    outputs += 1;
                    output_seen = true;
                }
                Operation::Compute { dispatch, .. }
                    if dispatch.iter().any(|&count| {
                        count == 0 || count > device.limits().max_compute_workgroups_per_dimension
                    }) =>
                {
                    return Err(PostError::Dispatch(*dispatch));
                }
                _ => {}
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
    frame_slot: usize,
    timing: std::rc::Rc<std::cell::RefCell<Option<crate::PassTimer>>>,
}

impl<F: StageFactory> PostChain<F> {
    pub fn new(
        device: &wgpu::Device,
        config: FrameConfig,
        persistent: Resources,
        factory: F,
    ) -> Result<Self, PostError> {
        factory.validate_config(device, config)?;
        let targets = FrameTargets::new(device, config)?;
        let configured = build_slots(&factory, device, &targets, &persistent)?;
        Ok(Self {
            targets,
            persistent,
            factory,
            configured,
            features: Vec::new(),
            capture_resources: None,
            geometry_prefix: None,
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
        use crate::contracts::{ResourceLifetime, Semantic};
        let registry = self.capture_resources.get_or_insert_with(Default::default);
        textures
            .iter()
            .map(|texture| {
                let key = registry.import(
                    texture,
                    ResourceLifetime::FrameLocal,
                    Semantic::Unspecified,
                    None,
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
        let targets = FrameTargets::new(device, config)?;
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
        self.targets.encode_resolve_timed(encoder, stamp.as_ref());
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

    fn prepare_features(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        borrowed: &mut [crate::BorrowedFeature<'_>],
        views: &[crate::AttachmentView],
    ) -> Result<(), crate::FrameError> {
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
        context.set_views(views);
        for feature in borrowed.iter_mut() {
            feature.prepared.prepare(&mut context)?;
        }
        for feature in &mut self.features {
            feature
                .prepared
                .as_mut()
                .expect("validated feature build")
                .prepare(&mut context)?;
        }
        for feature in borrowed.iter_mut() {
            feature.prepared.commit(&mut context);
        }
        for feature in &mut self.features {
            feature
                .prepared
                .as_mut()
                .expect("validated feature build")
                .commit(&mut context);
        }
        self.upload_slots[self.frame_slot] = context.finish();
        Ok(())
    }

    /// Encode a complete borrowed subplan without invoking retained frame features.
    /// Used for metadata prefixes; the same validation and raster compiler apply.
    pub fn encode_geometry_prefix(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        plan: &crate::FramePlan<'_>,
        borrowed: &mut [crate::BorrowedFeature<'_>],
    ) -> Result<(), crate::FrameError> {
        self.geometry_prefix = None;
        let retained = std::mem::take(&mut self.features);
        let result = self.encode_geometry(device, encoder, plan, borrowed);
        self.features = retained;
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
        let crate::FramePlan {
            passes,
            views,
            requested,
        } = *plan;
        let declarations: Vec<_> = borrowed
            .iter()
            .map(|feature| feature.declaration.clone())
            .chain(
                self.features
                    .iter()
                    .map(|feature| feature.declaration.clone()),
            )
            .collect();
        crate::frame::validate_frame_inputs(passes, &declarations, requested, views)?;
        crate::frame::validate_views(passes, views)?;
        let completed = self.completed_geometry_prefix(encoder, plan)?;
        let mut local_registry = crate::contracts::ResourceRegistry::default();
        let registry = self
            .capture_resources
            .as_mut()
            .unwrap_or(&mut local_registry);
        let raster =
            crate::contracts::stock::compile(device, passes, views, &declarations, registry)?;
        self.prepare_features(device, encoder, borrowed, views)?;
        for (index, declaration) in passes.iter().enumerate() {
            if completed.contains(&declaration.slot) {
                continue;
            }
            let context = crate::DrawContext {
                slot: declaration.slot,
            };
            let stamp = self.stamp(crate::PassId::Geometry(declaration.slot));
            raster.encode_stock_pass(index, encoder, stamp.as_ref(), |pass| {
                for feature in borrowed.iter().filter(|feature| {
                    feature
                        .declaration
                        .passes
                        .iter()
                        .any(|pass| pass.slot == context.slot)
                }) {
                    feature.prepared.draw(&context, pass);
                }
                for feature in self.features.iter().filter(|feature| {
                    feature
                        .declaration
                        .passes
                        .iter()
                        .any(|pass| pass.slot == context.slot)
                }) {
                    feature
                        .prepared
                        .as_ref()
                        .expect("validated feature build")
                        .draw(&context, pass);
                }
            })?;
        }
        self.finish_feature_draws(device, encoder, borrowed)
    }

    fn finish_feature_draws(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        borrowed: &mut [crate::BorrowedFeature<'_>],
    ) -> Result<(), crate::FrameError> {
        let mut context = crate::PrepareContext::new(device, encoder)?;
        for feature in borrowed {
            feature.prepared.finish_draws(&mut context);
        }
        for feature in &mut self.features {
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
    (0..factory.frame_slots())
        .map(|slot| {
            let set = factory.build_for_slot(device, targets, persistent, slot)?;
            set.validate(device)?;
            Ok(set)
        })
        .collect()
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
