use std::ops::Range;

use super::{CheckedBindings, ContractError, RasterPass, ResourceRegistry};

/// Immutable draw packet. Providers return packets, never encoder callbacks.
#[derive(Clone, Debug)]
pub struct CheckedDraw {
    pipeline: wgpu::RenderPipeline,
    pub(super) bindings: Vec<CheckedBindings>,
    targets: Vec<Option<wgpu::ColorTargetState>>,
    depth: Option<wgpu::DepthStencilState>,
    samples: u32,
    geometry: super::DrawGeometry,
    vertex_abi: Vec<super::geometry::VertexAbi>,
}
impl CheckedDraw {
    /// Retains the actual creation contract, rather than accepting caller-reported metadata.
    /// The descriptor layout is replaced with a layout built from the checked groups.
    pub fn new(
        device: &wgpu::Device,
        descriptor: &wgpu::RenderPipelineDescriptor<'_>,
        bindings: Vec<CheckedBindings>,
        vertices: Range<u32>,
        instances: Range<u32>,
    ) -> Result<Self, ContractError> {
        Self::with_geometry(
            device,
            descriptor,
            bindings,
            super::DrawGeometry {
                vertex_buffers: Vec::new(),
                index: None,
                elements: vertices,
                base_vertex: 0,
                instances,
            },
        )
    }
    /// Registers the pipeline vertex ABI and all actual geometry bindings.
    pub fn with_geometry(
        device: &wgpu::Device,
        descriptor: &wgpu::RenderPipelineDescriptor<'_>,
        bindings: Vec<CheckedBindings>,
        geometry: super::DrawGeometry,
    ) -> Result<Self, ContractError> {
        if descriptor.multiview_mask.is_some()
            || bindings.len() > device.limits().max_bind_groups as usize
        {
            return Err(ContractError::Pipeline);
        }
        if matches!(
            descriptor.primitive.topology,
            wgpu::PrimitiveTopology::LineStrip | wgpu::PrimitiveTopology::TriangleStrip
        ) && geometry
            .index
            .as_ref()
            .is_some_and(|index| descriptor.primitive.strip_index_format != Some(index.format))
        {
            return Err(ContractError::Pipeline);
        }
        let vertex_abi = geometry.register(device, descriptor.vertex.buffers)?;
        super::layout_limits::validate(device, &bindings.iter().collect::<Vec<_>>())?;
        let layouts: Vec<_> = bindings.iter().map(|group| Some(group.layout())).collect();
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &layouts,
            ..Default::default()
        });
        let descriptor = wgpu::RenderPipelineDescriptor {
            layout: Some(&layout),
            ..descriptor.clone()
        };
        Ok(Self {
            pipeline: device.create_render_pipeline(&descriptor),
            bindings,
            targets: descriptor
                .fragment
                .as_ref()
                .map_or_else(Vec::new, |fragment| fragment.targets.to_vec()),
            depth: descriptor.depth_stencil.clone(),
            samples: descriptor.multisample.count,
            geometry,
            vertex_abi,
        })
    }
    pub(super) fn validate(
        &self,
        registry: &ResourceRegistry,
        pass: &RasterPass,
        buffers: &[super::BufferUse],
    ) -> Result<(), ContractError> {
        self.geometry.validate(registry, buffers)?;
        self.geometry.check_ranges(&self.vertex_abi)?;
        if self.targets.len() != pass.colors.len() {
            return Err(ContractError::Pipeline);
        }
        for (target, attachment) in self.targets.iter().zip(&pass.colors) {
            match (target, attachment) {
                (Some(target), Some(attachment)) => {
                    let allocation = registry.allocation(attachment.view.allocation())?;
                    if target.format != attachment.view.format()
                        || self.samples != allocation.texture.sample_count()
                    {
                        return Err(ContractError::Pipeline);
                    }
                }
                (None, None) => {}
                _ => return Err(ContractError::Pipeline),
            }
        }
        match (&self.depth, &pass.depth) {
            (Some(state), Some(attachment)) => {
                let texture = &registry.allocation(attachment.view.allocation())?.texture;
                if state.format != attachment.view.format()
                    || self.samples != texture.sample_count()
                    || (attachment.read_only && state.depth_write_enabled == Some(true))
                {
                    return Err(ContractError::Pipeline);
                }
            }
            (None, None) => {}
            _ => return Err(ContractError::Pipeline),
        }
        for binding in &self.bindings {
            binding.validate(registry, pass)?;
        }
        Ok(())
    }
    pub(super) fn encode(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        for (index, binding) in self.bindings.iter().enumerate() {
            pass.set_bind_group(
                u32::try_from(index).expect("bounded binding count"),
                binding.group(),
                binding.dynamic_offsets(),
            );
        }
        self.geometry.encode(pass);
    }
}
