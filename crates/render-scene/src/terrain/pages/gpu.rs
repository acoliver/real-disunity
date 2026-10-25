//! Material composition expressed as an existing executor operation.
//!
//! The caller supplies registered buffers and admits their allocation/upload
//! costs before encoding. This module owns no queue, cache or submission path.

use super::composition::{CompositionError, LayerSample, compose_texel};
use worldrunner_render_post::{
    StageId,
    contracts::{
        AccessMode, BufferBinding, BufferBindingKind, BufferUse, BufferUseKind, BufferView,
        CheckedBindings, ContractError, RasterId, ResourceRegistry, TypedOperation, TypedWork,
    },
};

pub const COMPOSITION_WGSL: &str = include_str!("composition.wgsl");

/// Validated source data for a variable number of contributing layers per texel.
#[derive(Clone, Debug)]
pub struct CompositionBatch {
    ranges: Vec<u8>,
    samples: Vec<u8>,
    texels: u32,
}

/// Registered ranges for this batch, including the two packed output products.
#[derive(Clone, Debug)]
pub struct CompositionViews {
    pub ranges: BufferView,
    pub samples: BufferView,
    pub output: BufferView,
}

impl CompositionBatch {
    pub fn new(texels: &[Vec<LayerSample>]) -> Result<Self, CompositionError> {
        let count = u32::try_from(texels.len()).map_err(|_| CompositionError::ImageExtent)?;
        if count == 0 {
            return Err(CompositionError::ImageExtent);
        }
        let mut batch = Self {
            ranges: Vec::new(),
            samples: Vec::new(),
            texels: count,
        };
        for layers in texels {
            compose_texel(layers)?;
            let first = u32::try_from(batch.samples.len() / 24)
                .map_err(|_| CompositionError::ImageExtent)?;
            let count = u32::try_from(layers.len()).expect("validated layer count");
            first
                .checked_add(count)
                .ok_or(CompositionError::ImageExtent)?;
            batch
                .ranges
                .extend([first, count].into_iter().flat_map(u32::to_le_bytes));
            for sample in layers {
                batch.push_sample(*sample);
            }
        }
        Ok(batch)
    }

    pub fn ranges(&self) -> &[u8] {
        &self.ranges
    }
    pub fn samples(&self) -> &[u8] {
        &self.samples
    }
    pub fn output_bytes(&self) -> u64 {
        u64::from(self.texels) * 8
    }

    fn push_sample(&mut self, sample: LayerSample) {
        self.samples
            .extend(u32::from_le_bytes(sample.color).to_le_bytes());
        for normal in sample.normal {
            self.samples.extend(i32::from(normal).to_le_bytes());
        }
        self.samples
            .extend(u32::from(sample.roughness).to_le_bytes());
        self.samples.extend(u32::from(sample.weight).to_le_bytes());
    }

    /// Inputs must be produced by uploads of this batch's byte slices.
    /// Output is unpublished scratch until the owning frame is submitted.
    pub fn operation(
        &self,
        device: &wgpu::Device,
        registry: &ResourceRegistry,
        views: CompositionViews,
    ) -> Result<TypedOperation, ContractError> {
        self.validate_views(&views)?;
        let group = bindings(device, registry, &views)?;
        let pipeline = pipeline(device, &group);
        let mut operation = TypedOperation::new(
            RasterId::new(
                StageId::new("terrain material composition", 0),
                views.output.version(),
            ),
            TypedWork::Compute {
                pipeline,
                bindings: vec![group],
                workgroups: [self.texels.div_ceil(64), 1, 1],
            },
        );
        operation.buffers = vec![
            BufferUse::read(views.ranges, BufferUseKind::Storage),
            BufferUse::read(views.samples, BufferUseKind::Storage),
            BufferUse {
                view: views.output,
                before: None,
                access: AccessMode::Write,
                kind: BufferUseKind::Storage,
            },
        ];
        Ok(operation)
    }

    fn validate_views(&self, views: &CompositionViews) -> Result<(), ContractError> {
        for (view, bytes) in [
            (&views.ranges, self.ranges.len() as u64),
            (&views.samples, self.samples.len() as u64),
            (&views.output, self.output_bytes()),
        ] {
            if view.range().end - view.range().start != bytes {
                return Err(ContractError::Range(view.allocation()));
            }
        }
        Ok(())
    }
}

fn bindings(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    views: &CompositionViews,
) -> Result<CheckedBindings, ContractError> {
    let buffers = [
        views.ranges.clone(),
        views.samples.clone(),
        views.output.clone(),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, view)| BufferBinding {
        binding: u32::try_from(index).expect("three bindings"),
        visibility: wgpu::ShaderStages::COMPUTE,
        view,
        kind: BufferBindingKind::Storage(if index == 2 {
            AccessMode::Write
        } else {
            AccessMode::Read
        }),
        dynamic_offset: None,
    })
    .collect::<Vec<_>>();
    CheckedBindings::build_with_buffers(device, registry, &[], &buffers)
}

fn pipeline(device: &wgpu::Device, group: &CheckedBindings) -> wgpu::ComputePipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[Some(group.layout())],
        ..Default::default()
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("terrain material composition"),
        source: wgpu::ShaderSource::Wgsl(COMPOSITION_WGSL.into()),
    });
    worldrunner_render_post::compute_pipeline(
        device,
        worldrunner_render_post::ComputeDescriptor {
            label: Some("terrain material composition"),
            layout: &layout,
            module: &module,
            entry: "compose",
            constants: &[],
        },
    )
}
