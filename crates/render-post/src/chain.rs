use wgpu::util::DeviceExt;

use crate::{FrameConfig, FrameTargets, PostError, fullscreen_pass};

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

/// Output is supplied per encode, allowing the same chain to target a surface or capture.
#[derive(Clone, Debug)]
pub enum StageTarget {
    Output,
    Texture(wgpu::TextureView),
}

/// A linear authored sequence, not a render graph. Bind groups carry explicit caller layouts.
#[derive(Debug)]
pub enum Operation {
    Clear(wgpu::Buffer),
    CopyBuffer {
        source: wgpu::Buffer,
        destination: wgpu::Buffer,
        bytes: u64,
    },
    Compute {
        pipeline: wgpu::ComputePipeline,
        groups: Vec<wgpu::BindGroup>,
        dispatch: [u32; 3],
    },
    Fullscreen {
        pipeline: wgpu::RenderPipeline,
        groups: Vec<wgpu::BindGroup>,
        target: StageTarget,
    },
}

#[derive(Debug)]
pub struct Stage {
    pub label: &'static str,
    pub enabled: bool,
    pub operation: Operation,
}

impl Stage {
    pub fn fullscreen(
        label: &'static str,
        pipeline: &wgpu::RenderPipeline,
        groups: &[&wgpu::BindGroup],
        target: StageTarget,
    ) -> Self {
        Self {
            label,
            enabled: true,
            operation: Operation::Fullscreen {
                pipeline: pipeline.clone(),
                groups: groups.iter().map(|group| (*group).clone()).collect(),
                target,
            },
        }
    }

    fn encode(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        if !self.enabled {
            return;
        }
        match &self.operation {
            Operation::Clear(buffer) => encoder.clear_buffer(buffer, 0, None),
            Operation::CopyBuffer {
                source,
                destination,
                bytes,
            } => encoder.copy_buffer_to_buffer(source, 0, destination, 0, *bytes),
            Operation::Compute {
                pipeline,
                groups,
                dispatch,
            } => {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some(self.label),
                    timestamp_writes: None,
                });
                pass.set_pipeline(pipeline);
                for (index, group) in groups.iter().enumerate() {
                    pass.set_bind_group(
                        u32::try_from(index).expect("validated GPU group index"),
                        group,
                        &[],
                    );
                }
                pass.dispatch_workgroups(dispatch[0], dispatch[1], dispatch[2]);
            }
            Operation::Fullscreen {
                pipeline,
                groups,
                target,
            } => {
                let view = match target {
                    StageTarget::Output => output,
                    StageTarget::Texture(view) => view,
                };
                fullscreen_pass(
                    encoder,
                    Some(self.label),
                    wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    },
                    pipeline,
                    &groups.iter().collect::<Vec<_>>(),
                );
            }
        }
    }
}

#[derive(Default, Debug)]
pub struct StageSet {
    pub stages: Vec<Stage>,
    pub resources: Resources,
}

impl StageSet {
    fn validate(&self, device: &wgpu::Device) -> Result<(), PostError> {
        let mut outputs = 0;
        let mut output_seen = false;
        for stage in self.stages.iter().filter(|stage| stage.enabled) {
            if output_seen {
                return Err(PostError::OutputStage);
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
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        persistent: &Resources,
    ) -> Result<StageSet, PostError>;
}

/// Owns scene/MSAA resolve, fixed history, sized resources and stage execution order.
pub struct PostChain<F> {
    targets: FrameTargets,
    persistent: Resources,
    factory: F,
    configured: StageSet,
    features: Vec<crate::frame::RegisteredFeature>,
    upload_slots: [Vec<wgpu::Buffer>; 3],
    frame_slot: usize,
}

impl<F: StageFactory> PostChain<F> {
    pub fn new(
        device: &wgpu::Device,
        config: FrameConfig,
        persistent: Resources,
        factory: F,
    ) -> Result<Self, PostError> {
        let targets = FrameTargets::new(device, config)?;
        let configured = factory.build(device, &targets, &persistent)?;
        configured.validate(device)?;
        Ok(Self {
            targets,
            persistent,
            factory,
            configured,
            features: Vec::new(),
            upload_slots: std::array::from_fn(|_| Vec::new()),
            frame_slot: 0,
        })
    }

    /// Build replacements before switching; fixed history remains unchanged on resize/failure.
    pub fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) -> Result<bool, PostError> {
        if self.targets.config().size == size {
            return Ok(false);
        }
        let targets = FrameTargets::new(
            device,
            FrameConfig {
                size,
                ..self.targets.config()
            },
        )?;
        let configured = self.factory.build(device, &targets, &self.persistent)?;
        configured.validate(device)?;
        for feature in &mut self.features {
            feature.prepared = None;
        }
        self.targets = targets;
        self.configured = configured;
        Ok(true)
    }

    /// Select only a reserved slot whose preceding submission has completed.
    /// Immutable staging allocations remain owned by this slot until its next use.
    pub fn select_frame_slot(&mut self, slot: usize) {
        assert!(slot < self.upload_slots.len(), "frame slot out of range");
        self.frame_slot = slot;
    }

    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        for stage in &self.configured.stages {
            stage.encode(encoder, output);
        }
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
        let configured = factory.build(device, &self.targets, &self.persistent)?;
        configured.validate(device)?;
        self.factory = factory;
        self.configured = configured;
        Ok(())
    }
}

impl<F: StageFactory> PostChain<F> {
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

    fn prepare_features(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        borrowed: &mut [crate::BorrowedFeature<'_>],
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
        let mut context = crate::PrepareContext::new(device, encoder);
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
        crate::validate_frame(passes, &declarations, requested)?;
        crate::frame::validate_views(passes, views)?;
        self.prepare_features(device, encoder, borrowed)?;
        for declaration in passes {
            let context = crate::DrawContext {
                slot: declaration.slot,
            };
            crate::frame::encode_pass(encoder, declaration, views, |pass| {
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
            });
        }
        Ok(())
    }
}
